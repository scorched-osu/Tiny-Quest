# How to run the playable demo

The demo is a **client-side simulation** of the game — the full loop (hero,
gear, enhance/awaken, auto-battler, skills, marketplace) runs locally with no
wallet, no tokens, and nothing to deploy. It's for playing and evaluating the
game before the on-chain version goes online.

## 1. Prerequisites (one-time)

You need **Node.js 18 or newer**. Check:

```bash
node --version
```

If it's missing or older, install the LTS build from <https://nodejs.org>
(that also installs `npm`).

## 2. Get the code

First time:

```bash
git clone https://github.com/scorched-osu/Tiny-Quest.git
cd Tiny-Quest
git checkout claude/session-link-2r24ya
```

Already cloned:

```bash
cd Tiny-Quest
git checkout claude/session-link-2r24ya
git pull origin claude/session-link-2r24ya
```

## 3. Run it

```bash
cd frontend
npm install      # first time only — ~1–2 min
npm run dev
```

Then open the URL it prints — **http://localhost:3000**.

Stop the server with **Ctrl+C**.

## What to try

- **Bag** — tap items to inspect; equip/unequip (green dot = equipped) and
  watch the hero's Attack / HP / Defence change. Only equipped gear counts.
- **Item card** — Enhance (+N, guaranteed vs. chance by slot/quality) and
  Awaken (★ stars).
- **Battle** — the auto-battler clears scaling stages (boss every 5th); skills
  (🔥 Fireball / ✨ Mend / ☄️ Meteor) auto-cast on cooldown; drops accrue.
  Hit **Claim Rewards** to bank Exp/Soul/Jade and level up.
- **Trade** — buy NPC gear with Jade; list your own and see the live
  seller / royalty / fee / burn split.
- **Class chip** (on the hero card) — re-mint a different class.

## Troubleshooting

- **Install fails** — `rm -rf node_modules && npm install`.
- **Port 3000 in use** — `npm run dev -- -p 3001`, then open
  <http://localhost:3001>.
- **Blank / unstyled page** — hard refresh (Ctrl+Shift+R); web fonts load on
  first paint.

## Note

`http://localhost:3000/onchain` is the wallet + Anchor wiring reference. It
needs the programs deployed to a Solana cluster to actually transact, so leave
it for the online phase — use the root route (`/`) to play.
