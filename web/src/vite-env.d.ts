/// <reference types="vite/client" />

// Build-time configuration (spec 6.5; none of it secret). See src/config.ts.
interface ImportMetaEnv {
  readonly VITE_NETWORK?: string;
  readonly VITE_NODE_URL?: string;
  readonly VITE_INDEXER_URL?: string;
  readonly VITE_COVENANT_ID?: string;
  readonly VITE_BUNDLE_HASH?: string;
  readonly VITE_REFERRAL_ADDRESS?: string;
  readonly VITE_EXPLORER_URL?: string;
  readonly VITE_FAUCET_URL?: string;
  readonly VITE_WASM?: string;
  /** TEST-ONLY: a build that must not contain the quick wallet (scripts/check-mainnet.mjs). */
  readonly VITE_MAINNET_CHECK?: string;
  /** The testnet-10 faucet the wallet sheet links to (spec S7). */
  readonly VITE_FAUCET_LINK?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
