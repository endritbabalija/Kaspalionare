# web/: the Kaspalionare page (front end, testnet-10 beta)

Spec: `docs/specs/frontend.md`. Build log and stage notes: `docs/specs/frontend-handoff.md`. Root rules: `../CLAUDE.md` (Safety, no keys, testnet-10 and simnet only).

## Rules the page keeps

- **No lottery logic in TypeScript** (spec 1). Every figure is decoded from chain state by the wasm (`crates/web`) or computed by it (`jackpotOdds`, builders' `BuiltTx`). `src/lib/format.ts` only prints decimal strings (KAS 2 decimals, exact sompi, DAA at 10/s as a clock).
- **No hand-written mirror of a Rust type** (spec 1.1). Types come from the generated `src/wasm/<dev|release>/web.d.ts` (`import type { … } from '@wasm/web.js'`). The worker's call list (`src/worker/protocol.ts` `CALLS`) holds export names only; argument and result types are `Parameters<>`/`ReturnType<>` of the generated functions.
- **Nothing from the indexer is trusted** (spec 6.3). Read a response with `fetchText` under its cap (`indexerLimits`), parse it with the worker's `parseIndexer*` (never `JSON.parse`; ESLint refuses it in `src/`), read every entry from the node by outpoint (`nodeEntries`, `nodeCoins`; never the indexer's), then `checkIndexer*`. X's header comes from the node (`nodeAnchorBlock`), never the indexer.
- **Errors are typed codes, one message each** (spec 1.1 AX): the Rust table (`lottery::web::Code`), loaded at boot (`errorTable`). The page raises codes of the same table (`indexer_unavailable`, `worker_stopped`, `bytes_length`) through `PageError` (`src/lib/errors.ts`). Show `message`; `detail` is for logs.
- **Copy rule** (spec 5 rule 6, D137): no taglines, helper text or secondary labels. Figures and their names, status, errors, the network badge and control labels only. Explanations live on How it works (S6).
- **AX**: every interactive element has a stable `data-testid` and an accessible name (the e2e `namedAndTagged` check); each component in its own small file under `src/components/` with every state of spec 4; touch targets at least 44 px; meaning never by colour alone.
- **Tokens in one file**: `src/theme/tokens.css` (both themes, spec 5's table exactly; `tokens.test.ts` pins the values and checks WCAG AA contrast of every text pair). Tailwind utilities map to them in `src/index.css` (`bg-ground`, `text-ink`, `border-rule`, …). Never a raw colour in a component.
- **Dev only**: the component catalogue (`#/dev/catalogue`, `src/dev/Catalogue.tsx`) and the dev faucet client (`src/dev/faucet.ts`) are imported behind `import.meta.env.DEV`; `scripts/check-bundle.mjs` fails the production build if any trace of them, or of the simnet configuration, is in `dist/`.
- **npm safety**: run every npm/npx command with `web/` as the working directory (it has its own `package.json`; `C:\Users\endri\package.json` exists, so npm run elsewhere lands in the home folder). Never install globally. Playwright's browsers go to `web/.playwright-browsers`.
- Low-memory PC: never run a cargo build and a Vite build at once.

## Layout

- `src/main.tsx` entry; `src/app/` shell (`Shell.tsx`: header, network badge, menu, bottom nav), hash routes (`routes.ts`), session (`ctx.ts`).
- `src/worker/`: `lottery.worker.ts` (the wasm in a Web Worker), `protocol.ts` (typed messages), `client.ts` (`LotteryClient`: size cap, restart on a crash, replays `boot`), `session.ts` (boot: `errorTable`, `start(network)`, `nodeConnect(url)`).
- `src/net/`: `fetchText.ts` (capped reads), `indexer.ts` (API client), `round.ts` (a round checked against the node; the round card's figures).
- `src/components/`: Number ball, Bonus tile, Round status pill, Jackpot and countdown, Amount, Time, Hash or address, Transaction progress, Cost panel; `ui/` the shadcn/ui primitives (Button, DropdownMenu over Radix) restyled with the tokens.
- `src/screens/`: Play (S1; part A: the round card) and placeholders for the other screens.
- `src/theme/`: tokens, fonts (Geist, Geist Mono: vendored woff2 in `src/fonts/`, OFL 1.1), the theme switch (dark default, device setting, remembered: `kl.theme`; `public/theme-boot.js` sets it before first paint).
- `scripts/`: `wasm.mjs` (build `crates/web` and run wasm-bindgen), `dev-simnet.mjs` (the one-command dev run), `check-bundle.mjs`, `ensure-wasm.mjs`, `playwright-install.mjs`.
- `e2e/`: Playwright against the simnet dev run.

## Commands (from `web/`)

- First time: `npm ci`, then `npm run e2e:install` (Chromium into `web/.playwright-browsers`). Rust side as `../CLAUDE.md` (the wasm32 target, LLVM clang/ar, `wasm-bindgen-cli` 0.2.100; simnet needs LIBCLANG_PATH and protoc).
- **Dev run against the private simnet (one command):** `npm run dev:simnet`. Builds the dev wasm (`crates/web` features `simnet,node`) and `simnet` (release; about 10 minutes the first time), starts `simnet dev` (a fresh private node, round 1 drawn, round 2 closing, round 3 open, the indexer API on 127.0.0.1:8790, the node's Borsh wRPC on 127.0.0.1:18610, the dev faucet on 127.0.0.1:8791), waits for its `crates/target/simnet-dev/current.json`, then serves the page on http://127.0.0.1:5173/. `--skip-build` reuses the built binaries. Ctrl-C stops everything.
- Tests: `npm test` = typecheck (`tsc -b`, strict) + ESLint + Prettier check + Vitest (builds `src/wasm/dev` first if it is missing). Single parts: `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm run test:unit`.
- End to end: `npm run e2e` (reuses a running `npm run dev:simnet`, else starts one). Screenshots in `test-results/shots/`.
- wasm packages: `npm run wasm:dev` (simnet,node → `src/wasm/dev`), `npm run wasm:release` (node → `src/wasm/release`, then `wasm-opt -Oz`; the release artifact is the documented `crates/web` wasm build plus `--features node`, so W4 checks it).
- Production build: `npm run build` (release wasm, typecheck, `vite build`, `check-bundle.mjs`). Testnet-10 only: it refuses `VITE_NETWORK` other than testnet-10 and any `VITE_FAUCET_URL`. Configuration (spec 6.5, `src/config.ts`): `VITE_INDEXER_URL`, `VITE_COVENANT_ID`, optional `VITE_NODE_URL` (else the public resolver), `VITE_BUNDLE_HASH`, `VITE_REFERRAL_ADDRESS` (kaspatest:), `VITE_EXPLORER_URL`.
