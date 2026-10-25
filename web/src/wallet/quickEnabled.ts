// Whether this page build has the quick testnet wallet (spec 6.6: absent from a mainnet build). The page has
// no mainnet build (vite.config.ts refuses any network but testnet-10); `npm run check:mainnet` makes a
// TEST-ONLY build with VITE_MAINNET_CHECK=1 and checks that the quick wallet's page code is gone from it
// (scripts/check-mainnet.mjs). Vite replaces the variable at build time, so the dead branch and the dynamic
// import behind it are dropped.
export const QUICK_WALLET: boolean = import.meta.env.VITE_MAINNET_CHECK !== '1';
