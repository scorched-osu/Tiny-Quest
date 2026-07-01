// scripts/initialize.ts
//
// One-shot bootstrap for the whole game. Run AFTER `anchor deploy`.
// Order matters: progression's upgrade-auth PDA must be registered as the
// assets stat-writer, and mint authorities must move to the controlling PDAs.
//
//   npm i @coral-xyz/anchor @solana/web3.js @solana/spl-token
//   ANCHOR_WALLET=~/.config/solana/id.json ANCHOR_PROVIDER_URL=https://api.devnet.solana.com \
//     npx ts-node scripts/initialize.ts
//
// IDLs/types come from `anchor build` (../target/types/*).

import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import {
  createMint,
  getOrCreateAssociatedTokenAccount,
  TOKEN_2022_PROGRAM_ID,
} from "@solana/spl-token";

import { GameCore } from "../target/types/game_core";
import { GameResources } from "../target/types/game_resources";
import { GameAssets } from "../target/types/game_assets";
import { GameProgression } from "../target/types/game_progression";
import { GameMarketplace } from "../target/types/game_marketplace";

// ──────────────────────────────────────────────────────────────────────────
//  ⬇⬇⬇  PASTE YOUR FEE ADDRESS HERE  ⬇⬇⬇
//  Receives: marketplace protocol fees, primary mint fees, and Core royalties.
const FEE_ADDRESS = new PublicKey("REPLACE_WITH_YOUR_FEE_ADDRESS");
//  ⬆⬆⬆  PASTE YOUR FEE ADDRESS HERE  ⬆⬆⬆
// ──────────────────────────────────────────────────────────────────────────

// Metaplex Core program (same on devnet/mainnet)
const MPL_CORE = new PublicKey("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");

// Economic knobs (tune freely)
const DECIMALS = 6;
const u = (n: number) => BigInt(Math.round(n * 10 ** DECIMALS)); // human → base units
const FEE_BPS = 500;        // 5% marketplace fee
const ROYALTY_BPS = 500;    // 5% creator royalty
const BURN_SHARE_BPS = 3000; // burn 30% of the protocol fee
const REWARD_CAP_PER_EPOCH = u(1_000_000); // Jade emission ceiling / epoch
const SOUL_CAP_PER_EPOCH = u(10_000_000);  // Soul emission ceiling / epoch

async function main() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const admin = provider.wallet as anchor.Wallet;
  const conn = provider.connection;

  if (FEE_ADDRESS.toBase58() === "REPLACE_WITH_YOUR_FEE_ADDRESS") {
    throw new Error("Set FEE_ADDRESS at the top of this script first.");
  }

  const core = anchor.workspace.GameCore as Program<GameCore>;
  const resources = anchor.workspace.GameResources as Program<GameResources>;
  const assets = anchor.workspace.GameAssets as Program<GameAssets>;
  const progression = anchor.workspace.GameProgression as Program<GameProgression>;
  const marketplace = anchor.workspace.GameMarketplace as Program<GameMarketplace>;

  // Backend signer: authorized to mint rewards/XP/Soul and resolve rolls.
  // In production load this from a secured keypair; here we generate + log it.
  const backend = Keypair.generate();
  console.log("BACKEND AUTHORITY (save this):", backend.publicKey.toBase58());

  // ── 1. Mints: Jade (native) + Soul (resource) ───────────────────────────
  const jadeMint = await createMint(
    conn, admin.payer, admin.publicKey, null, DECIMALS, undefined, undefined, TOKEN_2022_PROGRAM_ID,
  );
  const soulMint = await createMint(
    conn, admin.payer, admin.publicKey, null, DECIMALS, undefined, undefined, TOKEN_2022_PROGRAM_ID,
  );
  console.log("Jade mint:", jadeMint.toBase58());
  console.log("Soul mint:", soulMint.toBase58());

  // ── 2. Derive PDAs ───────────────────────────────────────────────────────
  const pda = (seeds: (Buffer | Uint8Array)[], pid: PublicKey) =>
    PublicKey.findProgramAddressSync(seeds, pid)[0];

  const coreConfig = pda([Buffer.from("config")], core.programId);
  const assetsConfig = pda([Buffer.from("assets_config")], assets.programId);
  const assetsAuthority = pda([Buffer.from("authority")], assets.programId);
  const progConfig = pda([Buffer.from("prog_config")], progression.programId);
  const upgradeAuth = pda([Buffer.from("upgrade_auth")], progression.programId);
  const market = pda([Buffer.from("market")], marketplace.programId);
  const escrow = pda([Buffer.from("escrow")], marketplace.programId);
  const soulController = pda([Buffer.from("resource"), soulMint.toBuffer()], resources.programId);

  // Treasury ATA (fee address) for Jade — collects fees & royalties.
  const feeJadeAta = (await getOrCreateAssociatedTokenAccount(
    conn, admin.payer, jadeMint, FEE_ADDRESS, true, undefined, undefined, TOKEN_2022_PROGRAM_ID,
  )).address;

  // ── 3. core.initialize (moves Jade mint authority → coreConfig PDA) ──────
  await core.methods.initialize({
    authorizedMinter: backend.publicKey,
    marketplaceFeeBps: FEE_BPS,
    royaltyBps: ROYALTY_BPS,
    burnShareBps: BURN_SHARE_BPS,
    rewardCapPerEpoch: new anchor.BN(REWARD_CAP_PER_EPOCH.toString()),
  })
    .accounts({
      admin: admin.publicKey, config: coreConfig, tokenMint: jadeMint,
      treasury: FEE_ADDRESS, tokenProgram: TOKEN_2022_PROGRAM_ID, systemProgram: SystemProgram.programId,
    })
    .rpc();
  console.log("✓ core initialized");

  // ── 4. resources.init_resource for Soul (moves Soul authority → controller)
  await resources.methods.initResource({
    authorizedMinter: backend.publicKey,
    capPerEpoch: new anchor.BN(SOUL_CAP_PER_EPOCH.toString()),
  })
    .accounts({
      admin: admin.publicKey, mint: soulMint, controller: soulController,
      tokenProgram: TOKEN_2022_PROGRAM_ID, systemProgram: SystemProgram.programId,
    })
    .rpc();
  console.log("✓ Soul resource initialized");

  // ── 5. assets.initialize_assets ─────────────────────────────────────────
  //  upgrade_authority = progression's upgrade-auth PDA → only it can mutate stats.
  await assets.methods.initializeAssets({
    treasury: FEE_ADDRESS,
    upgradeAuthority: upgradeAuth,
    royaltyRecipient: FEE_ADDRESS,
    royaltyBps: ROYALTY_BPS,
    gearPrice: new anchor.BN(u(20).toString()), // heroes mint free
  })
    .accounts({
      admin: admin.publicKey, assetsConfig, updateAuthority: assetsAuthority,
      paymentMint: jadeMint, systemProgram: SystemProgram.programId,
    })
    .rpc();
  console.log("✓ assets initialized (upgrade authority = progression PDA)");

  // ── 6. Create Heroes + Gear collections (fresh keypairs sign their creation)
  const collections: Record<string, string> = {};
  for (const [kind, label] of [[{ heroes: {} }, "Heroes"], [{ gear: {} }, "Gear"]] as const) {
    const collection = Keypair.generate();
    await assets.methods.createCollection(kind as any, `${label} Collection`, `https://your.cdn/${label.toLowerCase()}.json`)
      .accounts({
        assetsConfig, admin: admin.publicKey, collection: collection.publicKey,
        updateAuthority: assetsAuthority, mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
      })
      .signers([collection])
      .rpc();
    collections[label.toLowerCase()] = collection.publicKey.toBase58();
    console.log(`✓ ${label} collection:`, collection.publicKey.toBase58());
  }

  // ── 7. progression.initialize ───────────────────────────────────────────
  await progression.methods.initialize({
    rollAuthority: backend.publicKey,
    xpAuthority: backend.publicKey,
    coreProgram: core.programId,
    assetsProgram: assets.programId,
    tokenMint: jadeMint,
    baseTokenCost: new anchor.BN(u(10).toString()),
    baseSuccessBps: 7500,
    successDecayBps: 750,
    minSuccessBps: 500,
    committedSeedHash: Array(32).fill(0), // rotate immediately via backend before first roll
    baseXp: 100, xpPerLevel: 50, xpQuad: 10, maxLevel: 60,
    growthStr: 2, growthDex: 2, growthInt: 2, growthVit: 2, pointsPerLevel: 3,
    soulMint, resourcesProgram: resources.programId,
    baseStarCost: new anchor.BN(u(100).toString()),
    starCostGrowth: new anchor.BN(u(50).toString()),
    maxStars: 10,
  })
    .accounts({
      admin: admin.publicKey, config: progConfig, upgradeAuthority: upgradeAuth,
      systemProgram: SystemProgram.programId,
    })
    .rpc();
  console.log("✓ progression initialized");

  // ── 8. marketplace.init_market ──────────────────────────────────────────
  await marketplace.methods.initMarket({
    treasury: FEE_ADDRESS, royaltyRecipient: FEE_ADDRESS, coreProgram: core.programId,
    feeBps: FEE_BPS, royaltyBps: ROYALTY_BPS, burnShareBps: BURN_SHARE_BPS,
  })
    .accounts({
      admin: admin.publicKey, market, escrowAuthority: escrow,
      paymentMint: jadeMint, systemProgram: SystemProgram.programId,
    })
    .rpc();
  console.log("✓ marketplace initialized");

  console.log("\n=== DEPLOY SUMMARY ===");
  console.log(JSON.stringify({
    feeAddress: FEE_ADDRESS.toBase58(),
    backendAuthority: backend.publicKey.toBase58(),
    jadeMint: jadeMint.toBase58(), soulMint: soulMint.toBase58(),
    feeJadeAta: feeJadeAta.toBase58(),
    collections,
    programs: {
      core: core.programId.toBase58(), resources: resources.programId.toBase58(),
      assets: assets.programId.toBase58(), progression: progression.programId.toBase58(),
      marketplace: marketplace.programId.toBase58(),
    },
    pdas: {
      coreConfig: coreConfig.toBase58(), assetsConfig: assetsConfig.toBase58(),
      assetsAuthority: assetsAuthority.toBase58(), progConfig: progConfig.toBase58(),
      upgradeAuth: upgradeAuth.toBase58(), soulController: soulController.toBase58(),
      market: market.toBase58(), escrow: escrow.toBase58(),
    },
  }, null, 2));
  console.log("\nNext: fund the backend authority with SOL, rotate the commit seed, and run the backend service.");
}

main().then(() => process.exit(0)).catch((e) => { console.error(e); process.exit(1); });
