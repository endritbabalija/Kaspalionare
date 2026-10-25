// Build-time configuration (spec 6.5), none of it secret. The simnet dev run (scripts/dev-simnet.mjs)
// fills it from crates/target/simnet-dev/current.json; a testnet-10 build sets it in .env files or the
// environment. A production build refuses any network but testnet-10 and any faucet (vite.config.ts).

export type Network = 'testnet-10' | 'simnet';

export interface Config {
  /** The network the lottery code is started on (spec 6.6). */
  network: Network;
  /** The node's wRPC URL; undefined means a public testnet-10 node from the resolver. */
  nodeUrl: string | undefined;
  /** The indexer API base, ending in /v1/ (spec 6.3). */
  indexerUrl: string;
  /** The instance this page is built for (64 hex digits). */
  covenantId: string;
  /** The launch bundle's blake2b-256 (spec 6.5), if pinned. */
  bundleHash: string | undefined;
  /** The site owner's referral address (a kaspatest: address; spec 7; kaspasim: on the simnet dev run). */
  referralAddress: string | undefined;
  /** The testnet-10 faucet the wallet sheet links to (spec S7; https only). */
  faucetLink: string | undefined;
  /** The testnet block explorer (transactions at <url>/txs/<id>, addresses at <url>/addresses/<a>). */
  explorerUrl: string | undefined;
  /** The dev faucet (simnet dev run only; never in a production build). */
  faucetUrl: string | undefined;
}

const HEX64 = /^[0-9a-f]{64}$/;

function nonEmpty(v: string | undefined): string | undefined {
  return v && v.trim().length ? v.trim() : undefined;
}

/** The build-time variables the configuration reads. */
export type ConfigEnv = Partial<
  Pick<
    ImportMetaEnv,
    | 'VITE_NETWORK'
    | 'VITE_NODE_URL'
    | 'VITE_INDEXER_URL'
    | 'VITE_COVENANT_ID'
    | 'VITE_BUNDLE_HASH'
    | 'VITE_REFERRAL_ADDRESS'
    | 'VITE_EXPLORER_URL'
    | 'VITE_FAUCET_URL'
    | 'VITE_FAUCET_LINK'
  >
>;

/** The configuration from `env`; the list of problems when it cannot be used. */
export function readConfig(env: ConfigEnv, dev: boolean): { config: Config } | { problems: string[] } {
  const problems: string[] = [];
  const network = (nonEmpty(env.VITE_NETWORK) ?? 'testnet-10') as Network;
  if (network !== 'testnet-10' && !(dev && network === 'simnet')) problems.push(`VITE_NETWORK=${network}`);
  const indexerUrl = nonEmpty(env.VITE_INDEXER_URL);
  if (!indexerUrl) problems.push('VITE_INDEXER_URL is not set');
  const covenantId = nonEmpty(env.VITE_COVENANT_ID)?.toLowerCase();
  if (!covenantId || !HEX64.test(covenantId)) problems.push('VITE_COVENANT_ID is not 64 hex digits');
  const bundleHash = nonEmpty(env.VITE_BUNDLE_HASH)?.toLowerCase();
  if (bundleHash && !HEX64.test(bundleHash)) problems.push('VITE_BUNDLE_HASH is not 64 hex digits');
  const referralAddress = nonEmpty(env.VITE_REFERRAL_ADDRESS);
  const refPrefix = network === 'simnet' ? 'kaspasim:' : 'kaspatest:';
  if (referralAddress && !referralAddress.startsWith(refPrefix))
    problems.push(`VITE_REFERRAL_ADDRESS is not ${refPrefix}`);
  const faucetLink = nonEmpty(env.VITE_FAUCET_LINK);
  if (faucetLink && !faucetLink.startsWith('https://')) problems.push('VITE_FAUCET_LINK is not https://');
  const faucetUrl = dev && network === 'simnet' ? nonEmpty(env.VITE_FAUCET_URL) : undefined;
  if (problems.length || !indexerUrl || !covenantId) return { problems };
  return {
    config: {
      network,
      nodeUrl: nonEmpty(env.VITE_NODE_URL),
      indexerUrl: indexerUrl.endsWith('/') ? indexerUrl : `${indexerUrl}/`,
      covenantId,
      bundleHash,
      referralAddress,
      explorerUrl: nonEmpty(env.VITE_EXPLORER_URL),
      faucetUrl,
      faucetLink,
    },
  };
}

export const CONFIG = readConfig(import.meta.env, import.meta.env.DEV);
