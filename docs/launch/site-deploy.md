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
| `VITE_INDEXER_URL` | `https://api.<your domain>/v1/`, or several comma-separated | the indexer servers (docs/launch/indexer-server.md); see "Several indexers" below |
| `VITE_COVENANT_ID` | 64 hex | printed by `tn10 beta genesis` |
| `VITE_BUNDLE_HASH` | 64 hex | printed by `tn10 beta genesis`: blake2b-256 of the `bundle.json` the indexer serves |
| `VITE_REFERRAL_ADDRESS` | your `kaspatest:` address | the default referrer for every link without `?ref=` |
| `VITE_EXPLORER_URL` | a testnet-10 explorer, https | your choice; open one transaction link by hand |
| `VITE_FAUCET_LINK` | `https://faucet-tn10.kaspanet.io` | owner decision 2026-10-26; check it by hand |
| `VITE_NODE_URL` | empty (public nodes through the resolver) | optional: one fixed `wss://` node |

## Several indexers (F6 part E, owner 2026-10-28)

`VITE_INDEXER_URL` takes one URL or a comma-separated list (at most 8), for example
`https://api.one.org/v1/,https://api.two.org/v1/`.

- The page asks them **in order**, and each browser remembers the one that last answered and asks it first next
  time.
- It moves on to the next one when an indexer cannot be reached, does not answer within 15 seconds, answers
  with an error (including "busy"), or serves answers the page's own checks refuse ("does not match the chain").
  Errors that are not the indexer's (the node, the wallet) are never retried on another indexer.
- One page step (the round card, a round's detail, a purchase's build, a claim's round copy, the vault) reads
  one indexer from start to end, so the checks never mix two indexers' answers (`src/net/indexers.ts`).
- **Trust is unchanged:** every answer is still checked against a public node. A second indexer only keeps a
  dead or lying one from stopping the page. Only after every listed indexer failed does the page show the error.
- `npm run csp:write` puts every listed origin into `connect-src`; `check:deploy` checks each entry (https, not a
  placeholder, no duplicates).

## The indexer through the site's own address (Vercel proxy; checked on paper, not deployed)

The owner has no domain for the indexer server yet. Vercel can forward `/api/*` on the site's own address
(`https://<project>.vercel.app/api/v1/...`) to the server, so the server needs no domain and no certificate.

**What Vercel's documentation says** (read 2026-10-28):
- **Supported for static, prebuilt sites:** "Rewrites to external origins work universally with all frameworks"
  ([rewrites](https://vercel.com/docs/routing/rewrites)); a prebuilt deploy's `config.json` routes use "the
  same syntax as the `routes` property of the `vercel.json` file", with `dest` "A destination pathname or full
  URL" ([Build Output API](https://vercel.com/docs/build-output-api/configuration)).
- **An IP address or plain `http://` as the destination: not documented either way.** The only stated limit: a
  private IP or an IPv6 address is refused
  ([DNS_HOSTNAME_RESOLVED_PRIVATE](https://vercel.com/docs/errors/DNS_HOSTNAME_RESOLVED_PRIVATE)), so the server
  needs a public IPv4 address. One unofficial project rewrites to an `http://<IPv4>:8080` origin. So: probably
  works, to be confirmed by one throwaway deploy (with the owner).
- **Timeout: 120 seconds on every plan** ("Proxied Request Timeout (Seconds) | 120 | 120 | 120",
  [limits](https://vercel.com/docs/limits)); a timeout is `ROUTER_EXTERNAL_TARGET_ERROR` (502). The anchor proof
  (up to about 25 s to build) fits; the server's own 10 s deadline is the tighter one (a longer build answers
  "being built, ask again", and the round's next load gets the proof).
- **Size:** no documented limit on a proxied response body (10 MB is only the cache limit,
  [CDN cache](https://vercel.com/docs/caching/cdn-cache)); request headers at most 16 KB each, 32 KB in all
  ([REQUEST_HEADER_TOO_LARGE](https://vercel.com/docs/errors/REQUEST_HEADER_TOO_LARGE)). The page's largest
  answers are a few MB.
- **Caching:** Vercel honours the server's cache headers on external rewrites, and a response with `no-cache` is
  not cached ([CDN cache](https://vercel.com/docs/caching/cdn-cache)). The indexer API sends
  `Cache-Control: no-cache` on every answer (`crates/indexer-api/src/routes.rs`), so nothing is cached.
- **Hobby plan:** 100 GB of Fast Data Transfer and 1,000,000 CDN requests a month; past a limit the feature stops
  until 30 days have passed ([Hobby](https://vercel.com/docs/plans/hobby)). Proxied answers count toward both.
  Hobby is "restricted to non-commercial personal use only", and commercial use includes "Any method of
  requesting or processing payment from visitors of the site"
  ([fair use](https://vercel.com/docs/limits/fair-use-guidelines)). **A lottery page that sells tickets, even for
  valueless test-KAS, may fall under that wording: ask Vercel support, or use Pro ($20 a month).** This applies
  with or without the proxy.
- **The visitor's address:** Vercel overwrites `X-Forwarded-For` with the visitor's address ("do not forward
  external IPs ... to prevent IP spoofing") and sends the same in `x-real-ip`
  ([request headers](https://vercel.com/docs/headers/request-headers)). Vercel's own outgoing addresses are not
  published (fixed ones need Pro or Enterprise, [static IPs](https://vercel.com/docs/connectivity/static-ips)),
  so the server cannot recognise Vercel by address. Vercel documents a shared-secret header instead
  ("Restricting your origin to Vercel traffic", [rewrites](https://vercel.com/docs/routing/rewrites)): a
  `routes` transform sets it on every rewritten request, and "a visitor can't supply their own" one.

**Verdict: feasible**, with these trade-offs:
- **Same origin:** the page reaches the indexer at `/api/v1/`, which the CSP's `'self'` already allows; no extra
  CSP host, and no domain or certificate for the server.
- **Per-client limits need the secret header** (docs/launch/indexer-server.md "Behind the Vercel proxy").
  Without it the server sees every visitor as one of Vercel's addresses, and must either throttle them together
  or turn its limits off.
- **With an IP-only server the Vercel-to-server leg is plain http.** Someone on that network path could read the
  secret (and then dodge the per-client limits) or change answers, which the page refuses like any lying
  indexer. No money is at risk either way. A server domain with https removes this.
- **The site and the indexer then share one point of failure (Vercel).** List a second, direct indexer URL as
  well when one exists.
- **Not yet verified:** a bare-IP or http destination; that `vercel build` carries the rewrite (and a transform's
  secret) into `.vercel/output/config.json` (`check:deploy -- --vercel` now checks the rewrite route); that
  query strings (`?payout=`) pass through.

**Prepared, not deployed:** set `INDEXER_PROXY_TARGET=http://<server IPv4>` (or `https://<server domain>`) in
`web/.env.production` (it is not a page variable) and list `/api/v1/` in `VITE_INDEXER_URL`. `npm run csp:write`
then writes `"rewrites": [{"source": "/api/v1/:path*", "destination": "<target>/v1/:path*"}]` into
`vercel.json`; `node scripts/csp.mjs --check` and `check:deploy` refuse a `vercel.json` whose rewrites differ, a
`/api/v1/` entry without a target, and a target that is a placeholder or this PC. The secret-header transform is
not written yet: it needs the throwaway deploy above to confirm that `routes` work beside `headers` and
`rewrites` in a prebuilt deploy.

## The CSP: which hosts the page may talk to

Until now the page could connect to any `https:` or `wss:` host. The CSP's `connect-src` now lists only:
- `'self'`;
- the indexer's origin (each listed origin when `VITE_INDEXER_URL` lists several; none for `/api/v1/`, which
  `'self'` covers);
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
