"use client";
// frontend/app/page.tsx  (wiring example)
//
// Shows the pattern for turning GameUI's simulated handlers into live txs.
// Wrap once at the root, then drive actions through useGameClient().

import React, { useEffect, useState, useCallback } from "react";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";
import { GameWalletProvider, useGameClient } from "../lib/WalletProvider";
import type { InvItem } from "../lib/gameClient";
// import Game from "../components/GameUI"; // your UI, with props for items + handlers

export default function Page() {
  return (
    <GameWalletProvider>
      <Inner />
    </GameWalletProvider>
  );
}

function Inner() {
  const client = useGameClient();
  const [items, setItems] = useState<InvItem[]>([]);
  const [hero, setHero] = useState<string | null>(null); // hero asset pubkey
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    if (!client) return;
    const inv = await client.inventory();
    setItems(inv);
    setHero(inv.find((i) => i.collection === client.heroes.toBase58())?.asset ?? null);
  }, [client]);

  useEffect(() => { refresh(); }, [refresh]);

  // ── real handlers (replace GameUI's simulated ones) ──
  const onEnhance = async (item: InvItem) => {
    if (!client) return; setBusy(true);
    try { const r = await client.enhance(item); await refresh(); return r; }
    finally { setBusy(false); }
  };
  const onAwaken = async (item: InvItem) => {
    if (!client) return; setBusy(true);
    try { await client.awaken(item); await refresh(); } finally { setBusy(false); }
  };
  const onClaim = async () => {
    if (!client || !hero) return; setBusy(true);
    try { return await client.claim(hero); } finally { setBusy(false); refresh(); }
  };
  const onList = async (item: InvItem, price: number) => {
    if (!client) return; setBusy(true);
    try { await client.list(item, price); await refresh(); } finally { setBusy(false); }
  };

  if (!client) return <div style={{ padding: 24 }}><WalletMultiButton /><p>Connect a wallet to play.</p></div>;

  return (
    <div>
      <div style={{ position: "fixed", top: 12, right: 12, zIndex: 60 }}><WalletMultiButton /></div>
      {/* <Game items={items} onEnhance={onEnhance} onAwaken={onAwaken} onClaim={onClaim} onList={onList} busy={busy} /> */}
      {/* Wire these props into GameUI: replace doEnhance->onEnhance, doAwaken->onAwaken,
          claim->onClaim, the Trade "List" button->onList. The inspect card already
          reads plus/stars/quality/slot, which now come straight from on-chain attributes. */}
    </div>
  );
}
