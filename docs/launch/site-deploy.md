# Deploying the site to Vercel (testnet-10 beta)

## The decision: build here, deploy the built files

The page contains the lottery code compiled to WebAssembly from Rust. That build needs:
- the pinned Rust toolchain with the wasm32 target;
- LLVM clang;
- wasm-bindgen 0.2.100;
- about 10 minutes.

Vercel's build machines have none of these. Building there would also mean the files Vercel serves were never
checked by our tests.

So the site is **built and checked on this PC**, and Vercel only receives the finished files:
`vercel build` copies the checked `web/dist`, and `vercel deploy --prebuilt` uploads it.

- `vercel.json` sets `installCommand` and `buildCommand` to `scripts/vercel-prebuilt.mjs`, which installs and
  builds nothing. If no checked `web/dist` is present (for example on Vercel's own builders, if a git repository
  were ever connected), it fails, so an unchecked build can never be deployed.
- Every check runs on the exact files that ship:
  - `check-bundle`: no dev code; the served `.wasm` is the release package;
  - `check:mainnet`;
  - `check:csp`: the built page under `vercel.json`'s headers, in Chromium;
  - `check:deploy`: W4 on the served `.wasm` after `wasm-opt`, the configuration, and the CSP's hosts.
- `npm run check:deploy -- --vercel` then compares `.vercel/output` (what is uploaded) with `web/dist` file for
  file, and checks that its `config.json` carries both header rules of `vercel.json`. The Vercel docs do not say
  whether `vercel build` copies `vercel.json`'s headers into `config.json`, so this check settles it before
  anything is uploaded.

**Cost:** Vercel Hobby is free for non-commercial personal use. A free testnet beta with no payments and no ads
fits, as we read the terms; if in doubt, Vercel asks you to contact support. Pro is $20 a month.
Sources: [fair use](https://vercel.com/docs/limits/fair-use-guidelines), [pricing](https://vercel.com/pricing).

## The configuration (`web/.env.production`)

Copy `web/.env.production.example` to `web/.env.production` and fill it in. Nothing in it is secret: every value
ends up in the public page, so the file is committed with `vercel.json` (git ignores only Vite's `.env.*.local`
overrides, so nothing local changes what is built).

| Variable | Value | Where it comes from |
|---|---|---|
| `VITE_NETWORK` | `testnet-10` | fixed |
| `VITE_INDEXER_URL` | `https://api.<your domain>/v1/` | the indexer server (docs/launch/indexer-server.md) |
| `VITE_COVENANT_ID` | 64 hex | printed by `tn10 beta genesis` |
| `VITE_BUNDLE_HASH` | 64 hex | printed by `tn10 beta genesis`: blake2b-256 of the `bundle.json` the indexer serves |
| `VITE_REFERRAL_ADDRESS` | your `kaspatest:` address | the default referrer for every link without `?ref=` |
| `VITE_EXPLORER_URL` | a testnet-10 explorer, https | your choice; open one transaction link by hand |
| `VITE_FAUCET_LINK` | `https://faucet-tn10.kaspanet.io` | owner decision 2026-10-26; check it by hand |
| `VITE_NODE_URL` | empty (public nodes through the resolver) | optional: one fixed `wss://` node |

## The CSP: which hosts the page may talk to

Until now the page could connect to any `https:` or `wss:` host. The CSP's `connect-src` now lists only:
- `'self'`;
- the indexer's origin;
- the public resolver's four domains: `https://*.kaspa.stream`, `*.kaspa.red`, `*.kaspa.green`, `*.kaspa.blue`,
  and the same with `wss://`;
- or, with `VITE_NODE_URL`, only that node's origin instead of the resolver's domains.

`npm run csp:write` writes this list into `vercel.json` from `web/.env.production`. `check:deploy` refuses a
`vercel.json` that does not match.

**Why these hosts:**
- The resolver list is compiled into the rusty-kaspa v2.1.0 client: `rpc/wrpc/client/Resolvers.toml` and
  `src/resolver.rs:126-143` in the pinned source.
- The page asks `https://<name>.<domain>/v2/kaspa/testnet-10/tls/wrpc/borsh`. On 2026-10-27, 32 such requests
  (two to each of the 16 resolver names) all answered with one of three nodes, each on one of the same domains:
  `wss://muon-10.kaspa.blue`, `wss://vector-10.kaspa.green`, `wss://quark-10.kaspa.red`. Some resolver names did
  not answer. The answers carry `access-control-allow-origin: *`, so the page can read them.

**The trade-off (an owner choice):**
- **Narrow, as built.** If an attacker ever got script into the page, it could send data (for example the quick
  wallet's key) only to those hosts, not anywhere on the internet. The risk is that the resolver could one day
  hand out a node on a new domain. The page would then show the node as unavailable until `vercel.json` is
  updated; reloading usually gets another node. No money is at risk.
- **Wide (`https: wss:`, as before).** It never breaks on a new node domain, but gives no protection of that
  kind.
- **One fixed node (`VITE_NODE_URL`).** The narrowest list. The page then depends on that one node.

## Steps

Run every `npm` command in `web/`. Never run a cargo build and a Vite build at the same time.

1. Fill in `web/.env.production`, then run `npm run csp:write` and commit `vercel.json` and `.env.production`.
2. `npm run build`: the release wasm, `wasm-opt -Oz`, typecheck, `vite build`, `check-bundle`, the control.
3. `npm run check:mainnet`, `npm run check:csp`, `npm run check:deploy`. All must end with no FAIL.
4. One time only, link the folder to a Vercel project (needs the owner's Vercel login): in `web/`,
   `npx vercel link`. This writes `web/.vercel/project.json`, which git ignores.
5. `npx vercel pull --yes --environment=production`, then `npx vercel build --prod`. The build step only runs
   `vercel-prebuilt.mjs` and copies `dist` into `.vercel/output/static`.
6. `npm run check:deploy -- --vercel`: `.vercel/output` must equal `dist` and carry both header rules.
7. `npx vercel deploy --prebuilt --prod`.

## After deploying, check

1. Open the site:
   - the network badge says testnet-10;
   - the round card loads;
   - Rounds lists round 1;
   - Verify in this browser shows matches.
2. In the browser's developer tools (Network tab, the page document and `lottery.worker-*.js`):
   - `Content-Security-Policy` equals `vercel.json`'s site rule;
   - `X-Frame-Options: DENY`;
   - `Permissions-Policy` equals `vercel.json`'s (camera, microphone, geolocation and the like denied).
3. `/figures/draw-cabinet-tour.html` has the figure rule's CSP (ending in `sandbox allow-scripts`: opened on its
   own it runs in no site's origin), `X-Frame-Options: SAMEORIGIN` and the same `Permissions-Policy`, and How it
   works shows the figure.
4. The console shows no CSP report while the page is used.
5. The faucet link and one explorer link open.
6. KasWare connects on `kaspa_testnet_10`, and the quick wallet sheet opens.
7. Then run the journeys in docs/launch/beta-journeys.md.
