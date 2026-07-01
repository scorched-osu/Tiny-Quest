// backend/server.ts
//
// The authoritative game server. Holds the backend authority keypair
// (authorized_minter / xp_authority / roll_authority) and is the ONLY thing
// allowed to mint Jade/Soul, grant XP, and resolve chance upgrades. This is the
// anti-cheat boundary: drops are computed here from elapsed time, never trusted
// from the client.
//
//   npm i express @coral-xyz/anchor @solana/web3.js @solana/spl-token cross-fetch
//   ts-node backend/server.ts
//
// Env: RPC_URL, HELIUS_API_KEY, BACKEND_KEYPAIR (path), DEPLOY (path to the
// JSON summary printed by initialize.ts).

import express from "express";
import fs from "fs";
import crypto from "crypto";
import fetch from "cross-fetch";
import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { getOrCreateAssociatedTokenAccount, TOKEN_2022_PROGRAM_ID } from "@solana/spl-token";

const MPL_CORE = new PublicKey("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");
const D = JSON.parse(fs.readFileSync(process.env.DEPLOY ?? "./deploy.json", "utf8"));
const backend = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(fs.readFileSync(process.env.BACKEND_KEYPAIR!, "utf8"))),
);
const conn = new Connection(process.env.RPC_URL!, "confirmed");
const provider = new anchor.AnchorProvider(conn, new anchor.Wallet(backend), { commitment: "confirmed" });
anchor.setProvider(provider);

// Program clients (IDLs from anchor build, copied next to this file)
const core = new Program(require("./idl/game_core.json"), provider) as Program<any>;
const resources = new Program(require("./idl/game_resources.json"), provider) as Program<any>;
const assets = new Program(require("./idl/game_assets.json"), provider) as Program<any>;
const progression = new Program(require("./idl/game_progression.json"), provider) as Program<any>;

const P = (s: string) => new PublicKey(s);
const JADE = P(D.jadeMint), SOUL = P(D.soulMint);
const CORE_CONFIG = P(D.pdas.coreConfig), SOUL_CTRL = P(D.pdas.soulController);
const ASSETS_CONFIG = P(D.pdas.assetsConfig), ASSETS_AUTH = P(D.pdas.assetsAuthority);
const PROG_CONFIG = P(D.pdas.progConfig), UPGRADE_AUTH = P(D.pdas.upgradeAuth);
const HEROES_COLL = P(D.collections.heroes), GEAR_COLL = P(D.collections.gear);

// ─────────────────────────── economy tuning ───────────────────────────
const EXP_PER_SEC = 12;     // idle accrual rates
const SOUL_PER_SEC = 4;
const JADE_PER_SEC = 0.5;
const MAX_OFFLINE_SEC = 8 * 3600; // cap offline accrual at 8h

// ─────────────────────── authoritative accrual ────────────────────────
// In production back this with a DB. lastClaim[hero] gates how much can drop.
const lastClaim = new Map<string, number>();

async function ata(owner: PublicKey, mint: PublicKey) {
  return (await getOrCreateAssociatedTokenAccount(
    conn, backend, mint, owner, true, undefined, undefined, TOKEN_2022_PROGRAM_ID,
  )).address;
}

// ─────────────────────── commit-reveal seed store ─────────────────────
// Per round we commit keccak(seed) on-chain, then reveal it in resolve.
// Persist this map in production; losing it strands pending upgrades.
let currentSeed = crypto.randomBytes(32);
const keccak = (b: Buffer) => Buffer.from(require("js-sha3").keccak256.array(b));
const seedHash = (s: Buffer) => Array.from(keccak(s));

const app = express();
app.use(express.json());

// Claim idle rewards for a hero. Drops are computed from elapsed time here.
app.post("/battle/claim", async (req, res) => {
  try {
    const { player, heroAsset } = req.body;
    const owner = P(player), hero = P(heroAsset);
    const now = Date.now() / 1000;
    const last = lastClaim.get(heroAsset) ?? now - 60;
    const elapsed = Math.min(now - last, MAX_OFFLINE_SEC);
    lastClaim.set(heroAsset, now);

    const exp = Math.floor(elapsed * EXP_PER_SEC);
    const soul = BigInt(Math.floor(elapsed * SOUL_PER_SEC * 1e6));
    const jade = BigInt(Math.floor(elapsed * JADE_PER_SEC * 1e6));

    const heroMutate = {
      authority: backend.publicKey, config: PROG_CONFIG, asset: hero, collection: HEROES_COLL,
      assetsProgram: assets.programId, assetsConfig: ASSETS_CONFIG, upgradeAuthority: UPGRADE_AUTH,
      assetsUpdateAuthority: ASSETS_AUTH, mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
    };

    const ixs = [
      await progression.methods.grantXp(new BN(exp.toString()))
        .accounts(heroMutate).instruction(),
      await resources.methods.mintResource(new BN(soul.toString()))
        .accounts({ minter: backend.publicKey, controller: SOUL_CTRL, mint: SOUL,
          recipientTokenAccount: await ata(owner, SOUL), tokenProgram: TOKEN_2022_PROGRAM_ID }).instruction(),
      await core.methods.mintReward(new BN(jade.toString()))
        .accounts({ minter: backend.publicKey, config: CORE_CONFIG, tokenMint: JADE,
          recipientTokenAccount: await ata(owner, JADE), tokenProgram: TOKEN_2022_PROGRAM_ID }).instruction(),
    ];
    const tx = new anchor.web3.Transaction().add(...ixs);
    const sig = await provider.sendAndConfirm(tx, []);
    res.json({ sig, exp, soul: soul.toString(), jade: jade.toString(), elapsed });
  } catch (e: any) { res.status(500).json({ error: e.message }); }
});

// Resolve a pending chance upgrade (after the client called attempt_chance).
// Reveals the committed seed and rotates to a fresh one.
app.post("/upgrade/resolve", async (req, res) => {
  try {
    const { asset, collection } = req.body;
    const assetPk = P(asset), coll = P(collection);
    const [pending] = PublicKey.findProgramAddressSync(
      [Buffer.from("pending"), assetPk.toBuffer()], progression.programId);
    const pend = await (progression.account as any).pendingUpgrade.fetch(pending);

    const reveal = currentSeed;              // matches the on-chain committed hash
    const next = crypto.randomBytes(32);     // commitment for the next round
    const sig = await progression.methods
      .resolveChance(Array.from(reveal), seedHash(next))
      .accounts({
        rollAuthority: backend.publicKey, config: PROG_CONFIG, pending, owner: pend.owner,
        asset: assetPk, collection: coll, assetsProgram: assets.programId, assetsConfig: ASSETS_CONFIG,
        upgradeAuthority: UPGRADE_AUTH, assetsUpdateAuthority: ASSETS_AUTH,
        mplCoreProgram: MPL_CORE, systemProgram: SystemProgram.programId,
      }).rpc();
    currentSeed = next;
    res.json({ sig });
  } catch (e: any) { res.status(500).json({ error: e.message }); }
});

// Inventory: NFTs owned by `owner`, filtered to our collections, via Helius DAS.
app.get("/inventory/:owner", async (req, res) => {
  try {
    const r = await fetch(`https://devnet.helius-rpc.com/?api-key=${process.env.HELIUS_API_KEY}`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0", id: "1", method: "getAssetsByOwner",
        params: { ownerAddress: req.params.owner, page: 1, limit: 1000 },
      }),
    });
    const { result } = await r.json();
    const ours = new Set([HEROES_COLL.toBase58(), GEAR_COLL.toBase58()]);
    const items = (result?.items ?? []).filter((a: any) =>
      (a.grouping ?? []).some((g: any) => g.group_key === "collection" && ours.has(g.group_value)));
    res.json({ items });
  } catch (e: any) { res.status(500).json({ error: e.message }); }
});

// Keeper: roll the per-epoch emission counters (run on a cron).
app.post("/admin/advance-epoch", async (_req, res) => {
  try {
    const a = await core.methods.advanceEpoch()
      .accounts({ admin: backend.publicKey, config: CORE_CONFIG }).rpc().catch((e: any) => e.message);
    res.json({ core: a });
  } catch (e: any) { res.status(500).json({ error: e.message }); }
});

app.listen(8787, () => console.log("game backend on :8787  authority:", backend.publicKey.toBase58()));
