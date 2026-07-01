"use client";
// frontend/lib/WalletProvider.tsx
//
// Wraps the app in the Solana wallet-adapter stack and exposes useGameClient(),
// which returns a GameClient bound to the connected wallet (or null until ready).
//
//   npm i @solana/wallet-adapter-react @solana/wallet-adapter-react-ui \
//         @solana/wallet-adapter-wallets @solana/web3.js @coral-xyz/anchor
//   import "@solana/wallet-adapter-react-ui/styles.css";  // once, in your root

import React, { useMemo } from "react";
import { ConnectionProvider, WalletProvider, useConnection, useAnchorWallet } from "@solana/wallet-adapter-react";
import { WalletModalProvider } from "@solana/wallet-adapter-react-ui";
import { PhantomWalletAdapter, SolflareWalletAdapter } from "@solana/wallet-adapter-wallets";
import { AnchorProvider } from "@coral-xyz/anchor";
import { GameClient, Deploy } from "./gameClient";

import deploy from "./deploy.json"; // the summary printed by initialize.ts (+ feeAddress)

const RPC = process.env.NEXT_PUBLIC_RPC_URL ?? "https://api.devnet.solana.com";
const BACKEND = process.env.NEXT_PUBLIC_BACKEND_URL ?? "http://localhost:8787";

export function GameWalletProvider({ children }: { children: React.ReactNode }) {
  const wallets = useMemo(() => [new PhantomWalletAdapter(), new SolflareWalletAdapter()], []);
  return (
    <ConnectionProvider endpoint={RPC}>
      <WalletProvider wallets={wallets} autoConnect>
        <WalletModalProvider>{children}</WalletModalProvider>
      </WalletProvider>
    </ConnectionProvider>
  );
}

export function useGameClient(): GameClient | null {
  const { connection } = useConnection();
  const wallet = useAnchorWallet();
  return useMemo(() => {
    if (!wallet) return null;
    const provider = new AnchorProvider(connection, wallet, { commitment: "confirmed" });
    return new GameClient(provider, deploy as Deploy, BACKEND);
  }, [connection, wallet]);
}
