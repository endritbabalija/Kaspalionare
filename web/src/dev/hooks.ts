// DEV ONLY (simnet dev run): test hooks for the e2e suite's shard-conflict rebuild (spec 8). Imported behind
// import.meta.env.DEV (screens/Play.tsx), so a production build has none of it; scripts/check-bundle.mjs
// fails the build on the marker. They let a test hold a signed purchase before it is sent and force the
// shard a purchase is built on, so a second buyer can take the same shard first: a real conflict on the
// node, deterministic.

/** Searched for by scripts/check-bundle.mjs. */
export const DEV_HOOKS_MARKER = 'KASPALIONARE_DEV_HOOKS';

export interface DevHooks {
  /** DEV_HOOKS_MARKER: kept on the object so the marker survives tree-shaking in a dev build (the control of check-bundle). */
  marker?: string;
  /** Build on this shard index when it is live. */
  forceShard?: number;
  /** The shard index the last build used. */
  lastShard?: number;
  /** Hold a signed, checked purchase before it is sent, until `release()`. */
  holdBeforeSubmit?: boolean;
  /** True while a purchase is held. */
  holding?: boolean;
  release?: () => void;
}

export function installDevHooks(): DevHooks {
  const w = window as unknown as { __klDev?: DevHooks };
  w.__klDev ??= {};
  w.__klDev.marker = DEV_HOOKS_MARKER;
  return w.__klDev;
}
