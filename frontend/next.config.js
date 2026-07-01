/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  webpack: (config) => {
    // wallet-adapter / web3.js pull in optional node polyfills; ignore them client-side
    config.resolve.fallback = { fs: false, path: false, os: false, crypto: false };
    return config;
  },
};

module.exports = nextConfig;
