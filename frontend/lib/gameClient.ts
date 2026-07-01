// frontend/lib/gameClient.ts
//
// The on-chain transaction layer. One class that, given an AnchorProvider and
// the deploy summary, builds + sends every game action with the right accounts.
// Program IDs come from each IDL's embedded address (anchor >= 0.30); PDAs are
// derived locally; mints/collections/fee wallet come from deploy.json.
//
//   npm i @coral-xyz/anchor @solana/web3.js @solana/spl-token
//   Copy the five IDLs from `target/idl/*` into frontend/lib/idl/.

import { Program, AnchorProvider, BN } from "@coral-xyz/anchor";
import { PublicKey, Keypair, SystemProgram, Transaction } from "@solana/web3.js";
import { getAssociatedTokenAddressSync, TOKEN_2022_PROGRAM_ID } from "@solana/spl-token";

import coreIdl from "./idl/game_core.json";
import resourcesIdl from "./idl/game_resources.json";
import assetsIdl from "./idl/game_assets.json";
import progressionIdl from "./idl/game_progression.json";
import marketplaceIdl from "./idl/game_marketplace.json";

export const MPL_CORE = new PublicKey("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");

export type Deploy = {
  jadeMint: string; soulMint: string;
  collections: { heroes: string; gear: string };
  // fee wallet (treasury + royalty recipient)
  feeAddress: string;
};

export type InvItem = {
  asset: string; collection: string; slot: string; quality: number;
  plus: number; stars: number; name: string;
};

const ARMOR = ["helm", "chest", "legs", "gloves", "belt"];
export const guaranteedCap = (slot: string, q: number) =>
  q >= 3 ? 0 : ARMOR.includes(slot) ? 4 : slot === "weapon" ? 6 : 0;
const slotClass = (slot: string) =>
  ARMOR.includes(slot) ? { armor: {} } : slot === "weapon" ? { weapon: {} } : { accessory: {} };

export class GameClient {
  core; resources; assets; progression; marketplace;
  jade: PublicKey; soul: PublicKey; heroes: PublicKey; gear: PublicKey; fee: PublicKey;
  coreConfig: PublicKey; assetsConfig: PublicKey; assetsAuth: PublicKey;
  progConfig: PublicKey; upgradeAuth: PublicKey; market: PublicKey; escrow: PublicKey; soulCtrl: PublicKey;
  backend: string;

  constructor(private provider: AnchorProvider, d: Deploy, backendUrl: string) {
    this.core = new Program(coreIdl as any, provider);
    this.resources = new Program(resourcesIdl as any, provider);
    this.assets = new Program(assetsIdl as any, provider);
    this.progression = new Program(progressionIdl as any, provider);
    this.marketplace = new Program(marketplaceIdl as any, provider);

    this.jade = new PublicKey(d.jadeMint);
    this.soul = new PublicKey(d.soulMint);
    this.heroes = new PublicKey(d.collections.heroes);
    this.gear = new PublicKey(d.collections.gear);
    this.fee = new PublicKey(d.feeAddress);
    this.backend = backendUrl;

    const P = (seeds: (Buffer | Uint8Array)[], pid: PublicKey) => PublicKey.findProgramAddressSync(seeds, pid)[0];
    this.coreConfig = P([Buffer.from("config")], this.core.programId);
    this.assetsConfig = P([Buffer.from("assets_config")], this.assets.programId);
    this.assetsAuth = P([Buffer.from("authority")], this.assets.programId);
    this.progConfig = P([Buffer.from("prog_config")], this.progression.programId);
    this.upgradeAuth = P([Buffer.from("upgrade_auth")], this.progression.programId);
    this.market = P([Buffer.from("market")], this.marketplace.programId);
    this.escrow = P([Buffer.from("escrow")], this.marketplace.programId);
    this.soulCtrl = P([Buffer.from("resource"), this.soul.toBuffer()], this.resources.programId);
  }

  get me() { return this.provider.publicKey!; }
  private ata(owner: PublicKey, mint: PublicKey) { return getAssociatedTokenAddressSync(mint, owner, true, TOKEN_2022_PROGRAM_ID); }
  private listingPda(asset: PublicKey) { return PublicKey.findProgramAddressSync([Buffer.from("listing"), asset.toBuffer()], this.marketplace.programId)[0]; }

  // ── reads ──
  async inventory(owner = this.me): Promise<InvItem[]> {
    const r = await fetch(`${this.backend}/inventory/${owner.toBase58()}`);
    const { items } = await r.json();
    return items.map((a: any) => {
      const attr = Object.fromEntries((a.plugins?.attributes?.data?.attribute_list ?? a.attributes ?? []).map((x: any) => [x.key, x.value]));
      const coll = (a.grouping ?? []).find((g: any) => g.group_key === "collection")?.group_value;
      return { asset: a.id, collection: coll, slot: attr.slot ?? "weapon",
        quality: Number(attr.quality ?? 0), plus: Number(attr.plus ?? 0),
        stars: Number(attr.stars ?? 1), name: a.content?.metadata?.name ?? "Item" };
    });
  }

  // ── free hero mint (class-based) ──
  async mintHero(name: string, uri: string, heroClass: "warrior" | "pyromancer" | "archer" | "rogue" | "guardian" | "wizard") {
    const asset = Keypair.generate();
    const variant: any = { [heroClass]: {} };
    await this.assets.methods.mintHero(name, uri, variant)
      .accounts({
        assetsConfig: this.assetsConfig, buyer: this.me, asset: asset.publicKey, collection: this.heroes,
        updateAuthority: this.assetsAuth, mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
      }).signers([asset]).rpc();
    return asset.publicKey.toBase58();
  }

  // ── paid gear mint (Jade -> fee wallet) ──

  async mintGear(name: string, uri: string, params: { slot: string; itemLevel: number; quality: number; affixes: { key: string; value: string }[] }) {
    const asset = Keypair.generate();
    await this.assets.methods.mintGear(name, uri, params)
      .accounts({
        assetsConfig: this.assetsConfig, buyer: this.me, asset: asset.publicKey, collection: this.gear,
        updateAuthority: this.assetsAuth, paymentMint: this.jade,
        buyerTokenAccount: this.ata(this.me, this.jade), treasuryTokenAccount: this.ata(this.fee, this.jade),
        tokenProgram: TOKEN_2022_PROGRAM_ID, mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
      }).signers([asset]).rpc();
    return asset.publicKey.toBase58();
  }

  // ── enhancement: auto-routes guaranteed vs chance ──
  async enhance(item: InvItem): Promise<{ sig: string; mode: "guaranteed" | "chance"; resolveSig?: string }> {
    const asset = new PublicKey(item.asset);
    const target = item.plus + 1;
    const common = {
      asset, collection: new PublicKey(item.collection),
      coreProgram: this.core.programId, coreConfig: this.coreConfig, tokenMint: this.jade,
      playerTokenAccount: this.ata(this.me, this.jade), tokenProgram: TOKEN_2022_PROGRAM_ID,
    };
    if (target <= guaranteedCap(item.slot, item.quality)) {
      const sig = await this.progression.methods.upgradeGuaranteed(slotClass(item.slot), item.quality)
        .accounts({
          player: this.me, config: this.progConfig, ...common,
          assetsProgram: this.assets.programId, assetsConfig: this.assetsConfig,
          upgradeAuthority: this.upgradeAuth, assetsUpdateAuthority: this.assetsAuth,
          mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
        }).rpc();
      return { sig, mode: "guaranteed" };
    }
    // chance: open pending on-chain, then ask the backend to reveal + resolve
    const [pending] = PublicKey.findProgramAddressSync([Buffer.from("pending"), asset.toBuffer()], this.progression.programId);
    const clientSeed = Array.from(crypto.getRandomValues(new Uint8Array(32)));
    const sig = await this.progression.methods.attemptChance(slotClass(item.slot), item.quality, clientSeed)
      .accounts({ player: this.me, config: this.progConfig, pending, ...common, systemProgram: SystemProgram.programId }).rpc();
    const rr = await fetch(`${this.backend}/upgrade/resolve`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ asset: item.asset, collection: item.collection }),
    });
    const { sig: resolveSig } = await rr.json();
    return { sig, mode: "chance", resolveSig };
  }

  // ── awakening (burns Soul) ──
  async awaken(item: InvItem) {
    const asset = new PublicKey(item.asset);
    return this.progression.methods.awakenItem()
      .accounts({
        player: this.me, config: this.progConfig, asset, collection: new PublicKey(item.collection),
        resourcesProgram: this.resources.programId, soulController: this.soulCtrl, soulMint: this.soul,
        playerSoulAccount: this.ata(this.me, this.soul), tokenProgram: TOKEN_2022_PROGRAM_ID,
        assetsProgram: this.assets.programId, assetsConfig: this.assetsConfig,
        upgradeAuthority: this.upgradeAuth, assetsUpdateAuthority: this.assetsAuth,
        mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
      }).rpc();
  }

  // ── socket a gem (burns 1 gem token, records its mint in the socket slot) ──
  async socketGem(item: InvItem, socketIndex: number, gemMint: string) {
    const gem = new PublicKey(gemMint);
    const gemCtrl = P([Buffer.from("resource"), gem.toBuffer()], this.resources.programId);
    return this.progression.methods.socketGem(socketIndex)
      .accounts({
        player: this.me, config: this.progConfig, asset: new PublicKey(item.asset), collection: new PublicKey(item.collection),
        resourcesProgram: this.resources.programId, gemController: gemCtrl, gemMint: gem,
        playerGemAccount: this.ata(this.me, gem), tokenProgram: TOKEN_2022_PROGRAM_ID,
        assetsProgram: this.assets.programId, assetsConfig: this.assetsConfig,
        upgradeAuthority: this.upgradeAuth, assetsUpdateAuthority: this.assetsAuth,
        mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
      }).rpc();
  }

  // ── free a socket (gem is consumed on removal, not refunded) ──
  async unsocketGem(item: InvItem, socketIndex: number) {
    return this.progression.methods.unsocketGem(socketIndex)
      .accounts({
        player: this.me, config: this.progConfig, asset: new PublicKey(item.asset), collection: new PublicKey(item.collection),
        assetsProgram: this.assets.programId, assetsConfig: this.assetsConfig,
        upgradeAuthority: this.upgradeAuth, assetsUpdateAuthority: this.assetsAuth,
        mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
      }).rpc();
  }

  // ── allocate a leveling point into a stat ──
  async allocateStat(heroAsset: string, stat: "strength" | "agility" | "intelligence" | "vitality", amount = 1) {
    const variant: any = { [stat === "intelligence" ? "intelligence" : stat]: {} };
    return this.progression.methods.allocateStat(variant, amount)
      .accounts({
        authority: this.me, config: this.progConfig, asset: new PublicKey(heroAsset), collection: this.heroes,
        assetsProgram: this.assets.programId, assetsConfig: this.assetsConfig,
        upgradeAuthority: this.upgradeAuth, assetsUpdateAuthority: this.assetsAuth,
        mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
      }).rpc();
  }

  // ── claim idle combat rewards (server computes + mints) ──
  async claim(heroAsset: string) {
    const r = await fetch(`${this.backend}/battle/claim`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ player: this.me.toBase58(), heroAsset }),
    });
    return r.json();
  }

  // ── marketplace ──
  async list(item: InvItem, priceUi: number, decimals = 6) {
    const asset = new PublicKey(item.asset);
    return this.marketplace.methods.list(new BN(Math.round(priceUi * 10 ** decimals)))
      .accounts({
        market: this.market, seller: this.me, listing: this.listingPda(asset),
        asset, collection: new PublicKey(item.collection), escrowAuthority: this.escrow,
        mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
      }).rpc();
  }

  async delist(item: InvItem) {
    const asset = new PublicKey(item.asset);
    return this.marketplace.methods.delist()
      .accounts({
        market: this.market, seller: this.me, listing: this.listingPda(asset),
        asset, collection: new PublicKey(item.collection), escrowAuthority: this.escrow,
        mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
      }).rpc();
  }

  async buy(assetStr: string, collectionStr: string) {
    const asset = new PublicKey(assetStr);
    const listing = this.listingPda(asset);
    const l: any = await (this.marketplace.account as any).listing.fetch(listing);
    const seller = l.seller as PublicKey;
    return this.marketplace.methods.buy()
      .accounts({
        market: this.market, buyer: this.me, listing, seller,
        asset, collection: new PublicKey(collectionStr), escrowAuthority: this.escrow,
        paymentMint: this.jade, buyerTokenAccount: this.ata(this.me, this.jade),
        sellerTokenAccount: this.ata(seller, this.jade), treasuryTokenAccount: this.ata(this.fee, this.jade),
        royaltyTokenAccount: this.ata(this.fee, this.jade),
        coreProgram: this.core.programId, coreConfig: this.coreConfig,
        tokenProgram: TOKEN_2022_PROGRAM_ID, mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
      }).rpc();
  }
}
