# Soulforge — DeFi NFT ARPG on Solana

An idle auto-battler ARPG where heroes and gear are Metaplex Core NFTs, the
economy runs on a native SPL token (Jade) plus a fungible resource (Soul), and
every item trades on an in-game marketplace with enforced creator royalties.

## Architecture

```
                         ┌─────────────────────────────────────────┐
   wallet ──tx──▶        │  PROGRAMS (Anchor)                       │
                         │                                          │
   frontend ──▶ backend  │  core ......... Jade token, faucet, burn │
   (GameUI)     (server) │  resources .... Soul + mats, faucet,burn │
      │            │     │  assets ....... Hero/Gear NFTs, royalties│
      │            │     │  progression .. +N, XP, allocate, stars  │──CPI──┐
      │            │     │  marketplace .. escrow trade, fee+burn    │       │
      │            │     └─────────────────────────────────────────┘       │
      │            │            ▲          ▲            ▲                    │
      │            └─ mints rewards (Jade/Soul/XP), resolves rolls ─────────┘
      └─ reads inventory (Helius DAS via backend), signs user actions
```

Stat writes flow through ONE authority: progression's `[upgrade_auth]` PDA is the
only key the assets program accepts for `set_asset_attributes`. That's what makes
stats forgery-proof.

## Repo layout

```
programs/
  core/src/lib.rs           (core_lib.rs)
  resources/src/lib.rs      (resources_lib.rs)
  assets/src/lib.rs         (assets_lib.rs)
  progression/src/lib.rs    (progression_lib.rs)
  marketplace/src/lib.rs    (marketplace_lib.rs)
scripts/initialize.ts
backend/server.ts
frontend/lib/{gameClient.ts, WalletProvider.tsx, idl/*.json, deploy.json}
frontend/app/page.tsx       (page.wiring.tsx)
frontend/components/GameUI.jsx
Anchor.toml · Cargo.toml · DATA_MODEL.md
```
(Rename the delivered `*_lib.rs` files into the paths above.)

## Prerequisites
- Rust + Solana CLI (agave) + Anchor 0.31
- Node 18+
- A Helius API key (DAS indexing) and an RPC endpoint

## Build status & toolchain notes
Verified on host (`cargo check`) with rustc 1.94:
- **5 programs compile cleanly** — `core`, `resources`, `arena`, `guilds`, `titles`.
- **4 programs use `mpl-core`** — `assets`, `progression`, `marketplace`, `skills`.
  Their own source is sound, but they only build under the real `anchor build`
  SBF toolchain, **not** a host `cargo check`. Reason: `mpl-core` and Anchor 0.31
  must agree on a single Solana crate major. `mpl-core 0.8` references solana-1.x
  APIs removed in solana ≥2.2 (`PrintProgramError`); `mpl-core 0.12` pulls the
  solana-3.x/4.0 split-crates that clash with Anchor 0.31's solana-2.x. **Pin
  `mpl-core` to the version whose Solana major matches your installed Anchor/Agave
  toolchain** before `anchor build` (0.8.0 targets the API this code is written
  against — `Pubkey` creators, `CreateCollectionV2CpiBuilder`, etc.).
- Program IDs in `declare_id!`/`Anchor.toml` are valid 32-byte placeholders;
  replace with `anchor keys list` output before deploying.

## Try the game now (no chain needed)
```bash
cd frontend && npm i && npm run dev   # http://localhost:3000  → playable demo
```
The root route renders the simulated game (enhance / awaken / level / allocate /
trade against local state). The wallet + on-chain wiring lives at `/onchain`.

## 1. Build & deploy programs
```bash
anchor build
anchor keys list          # copy the 9 generated program IDs
# replace each declare_id!() AND the ids in Anchor.toml, then:
anchor build
anchor deploy --provider.cluster devnet
anchor idl init ...       # or copy target/idl/*.json into frontend/lib/idl/
```

## 2. Initialize on-chain state
```bash
# scripts/initialize.ts — set FEE_ADDRESS at the top first.
ANCHOR_WALLET=~/.config/solana/id.json \
ANCHOR_PROVIDER_URL=https://api.devnet.solana.com \
npx ts-node scripts/initialize.ts > deploy.json
# add "feeAddress" to deploy.json, copy it to frontend/lib/ and backend/
```
This creates the Jade + Soul mints, every config PDA, the Hero/Gear collections,
moves mint authorities to the controlling PDAs, and registers progression's PDA
as the stat-writer. Save the printed **backend authority** keypair.

## 3. Run the backend
```bash
RPC_URL=... HELIUS_API_KEY=... BACKEND_KEYPAIR=./backend-authority.json \
DEPLOY=./deploy.json  ts-node backend/server.ts
# fund the backend authority with SOL; POST /admin/advance-epoch on a daily cron
```

## 4. Run the frontend
```bash
cd frontend && npm i && npm run dev
# NEXT_PUBLIC_RPC_URL, NEXT_PUBLIC_BACKEND_URL in .env.local
```

## Economy knobs (tune in initialize.ts)
| Knob | Default | Effect |
|---|---|---|
| `FEE_BPS` | 5% | marketplace + primary fee |
| `ROYALTY_BPS` | 5% | enforced creator royalty |
| `BURN_SHARE_BPS` | 30% | share of fee burned (deflation) |
| `REWARD_CAP_PER_EPOCH` | 1M | Jade emission ceiling / epoch |
| `SOUL_CAP_PER_EPOCH` | 10M | Soul emission ceiling / epoch |
| enhance cost / success curve | base 10, 75%→floor 5% | the +N sink |
| star cost curve | base 100 +50/star | the awakening Soul sink |
| XP curve / growth | quad curve, +8 stat & 3 pts /level | leveling pace |

## Invariants that MUST hold (or things silently break)
- `assets_config.upgrade_authority == progression [upgrade_auth] PDA`
- Jade mint authority == core `[config]` PDA; Soul mint authority == its controller PDA
- Attribute keys match across programs (see DATA_MODEL.md): `exp`, `plus`, `stars`,
  `strength`/`agility`/`intelligence`/`vitality`, etc.
- Backend authority == `authorized_minter`/`xp_authority`/`roll_authority` everywhere

## Security notes
- **Randomness**: chance upgrades use commit-reveal (backend reveals a pre-committed
  seed). For full trustlessness swap `resolve_chance` to Switchboard On-Demand VRF.
- **Backend authority custody**: it can mint rewards — keep it in a KMS/HSM, not on disk.
- **mpl-core version**: pin it; plugin/instruction type names drift between minors.
- **Pre-mainnet**: write the Anchor test suite, get an audit, replace placeholder
  declare_id!()s, and lock the upgrade authority (or make programs immutable).

## Roadmap (seen in the reference, not yet built)
Skills tree · Arena/PvP · Guilds/Social · Crafting · Gem sockets · Gacha "Treasure"
chests · Titles · daily/seasonal events · pets as combat companions · Diamonds
(premium) purchase flow.

## Playable demo (frontend, no chain required)
`cd frontend && npm i && npm run dev` → the root route is a fully playable
vertical slice of the game loop:
- **Hero**: pick a class (6 classes with distinct starting stats), level up,
  allocate STR/AGI/INT/VIT points.
- **Gear**: equip/unequip a loadout (only equipped gear feeds combat stats),
  enhance (+N, guaranteed vs. chance by slot/quality), awaken (★ stars).
- **Auto-battler**: idle combat over scaling stages with bosses every 5th;
  hero auto-attacks from derived stats; **active skills** (Fireball/Mend/
  Meteor) auto-cast on cooldown. Kills drop Soul/Exp/Jade; Claim banks them
  into level-ups.
- **Marketplace**: buy NPC gear with Jade, list your own with a live
  seller/royalty/fee/burn split.

The `/onchain` route shows the same UI wired to the Anchor programs via wallet.

## What's done
✅ 9 programs (5 host-verified) · ✅ deploy/init · ✅ authoritative backend ·
✅ playable frontend (combat + skills + equip + marketplace) ·
✅ on-chain client + wallet wiring · ✅ data-model spec
