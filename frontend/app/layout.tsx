// frontend/app/layout.tsx — Next.js App Router root layout
import React from "react";
import "@solana/wallet-adapter-react-ui/styles.css";
import "./globals.css";

export const metadata = {
  title: "Soulforge",
  description: "DeFi NFT idle-ARPG on Solana",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
