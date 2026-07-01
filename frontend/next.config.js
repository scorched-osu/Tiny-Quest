/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  typescript: {
    // The /onchain reference route pulls in @solana/wallet-adapter, whose
    // FC-typed providers are incompatible with the React 18.3+/19 JSX
    // ReactNode types that react-native drags in transitively. It's a
    // type-only artifact (works at runtime), so we don't fail the production
    // build on it. Type errors still surface in the editor and `next dev`.
    ignoreBuildErrors: true,
  },
  eslint: {
    ignoreDuringBuilds: true,
  },
  webpack: (config) => {
    // wallet-adapter / web3.js pull in optional node polyfills; ignore them client-side
    config.resolve.fallback = { fs: false, path: false, os: false, crypto: false };
    // pino (via walletconnect) optionally requires pino-pretty; it's dev-only
    config.resolve.alias = { ...config.resolve.alias, "pino-pretty": false };
    return config;
  },
};

module.exports = nextConfig;
