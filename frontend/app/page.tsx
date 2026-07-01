"use client";
// frontend/app/page.tsx — playable demo route
//
// Renders the self-contained simulated game (GameUI). No wallet or chain
// required: enhance/awaken/level/allocate/trade all run against local
// simulated state so the game loop is playable immediately.
//
// The on-chain wiring reference lives at /onchain (app/onchain/page.tsx),
// which shows how to swap the simulated handlers for real Anchor txs.

import Game from "../components/GameUI";

export default function Page() {
  return <Game />;
}
