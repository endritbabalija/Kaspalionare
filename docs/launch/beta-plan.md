# The beta instance: plan and options (testnet-10, nothing launched yet)

This is the plan for the lottery instance the public beta runs on. It was rehearsed with a dry run on
2026-10-27: local ledger and engine, nothing sent to any node. **The owner chooses the options below.** The
launch itself (one genesis transaction on testnet-10) waits for the owner's go-ahead.

## How it is launched (and how not)

- **Only through `tn10 beta genesis`.** It runs the M8 campaign's C0 (balance and fee checks) and C1 (genesis
  plus the launch bundle) with the beta's settings. It stops there: no attack checks, no soak.
- **A separate, fresh admin key.**
  - `tn10 admin-keygen` writes `TN10_ADMIN_KEY` into the git-ignored `.env`, and prints only its admin hash.
    Done on 2026-10-27: admin hash `d8b95fa1…684ad529`.
  - `beta genesis` refuses a missing key, the public default key `ADMIN_SK` (a key that is in the repository),
    and a key equal to the funding key.
  - The admin key can only change s_F, s_R and the fee address (`set_params()`, design 1.2). It cannot move
    money.
  - Keep a copy of `.env` somewhere safe (not in git).
- **Never through `tn10 probe` or `wallet-probe`** (`tn10/src/cases.rs`, `wallet_probe.rs`). Their instances use
  the public default admin key, so anyone could call `set_params()`. Two guards:
  - `tn10 bundle` and `tn10 bundle-verify` now refuse any bundle with that key's admin hash;
  - `bundle-verify` prints the admin hash it found (the M8 bundle: `e913e8dc…6a0eba67`, not the default).
- Commands (from `crates/`, after `cargo build -j 4 --release -p tn10`):
  - rehearse: `./target/release/tn10 beta genesis --profile steady --dry-run`;
  - launch (owner's go-ahead only):
    `./target/release/tn10 beta genesis --profile steady --fee-address kaspatest:… --submit`.
  - It writes `tn10-data/beta/<covenant ID>/` (the store the indexer server needs) and prints
    `VITE_COVENANT_ID` and `VITE_BUNDLE_HASH` for `web/.env.production`.
  - Then `tn10 bundle-verify tn10-data/beta/<cov>/bundle.json` before announcing anything.

## What the dry run showed (both profiles)

| | |
|---|---|
| Checks | C0 balance and fee rate ok. C1: covenant ID recomputed; the bundle's admin hash is the new key's, not the default's; `bundle-verify` passes (21 live UTXOs, no mismatch). |
| Genesis locks | **137.49 test-KAS**: V0 100 KAS vault, plus 21 protocol outputs × R = 1.69 KAS, plus 2 KAS fee accumulator. The fee is 0.46 KAS. |
| Funding wallet | 4,789 test-KAS on 2026-10-27; the largest coin is 3,728 KAS. Genesis spends one coin, so this is enough. |
| Larger jackpots | Also validated: J(1) 50 KAS with V0 300 KAS (locks 337.49 KAS), and J(1) 100 KAS with V0 600 KAS (637.49 KAS). Caller shares s_C = s_D = 300 (3 %) pass too. |

Unit tests (`cargo test -j 1 --release -p tn10`):
- `beta_profiles_pass_genesis_validation`: both profiles pass every genesis floor. A claim window one DAA below
  7 days fails, and so does a draw window below 24 h.
- `admin_guard_refuses_the_public_default_key`.

## The settings: what is fixed and what the owner picks

At 10 blocks a second: 36,000 DAA is 1 hour; 864,000 DAA is 1 day.

| Setting | Beta value | Floor or rule (cited) | What it means |
|---|---|---|---|
| Claim window | **7 days** (6,048,000) | ≥ CLAIM_WINDOW, `genesis.rs` validate | Winners have this long after the draw window to claim. |
| Draw window | **24 h** (864,000) | ≥ VOID_MIN (D97) | If nobody draws within 24 h of D, anyone can void the round (no winners). |
| Seed offset | **1 min** (600) | ≥ 1; 600 is the design value (D93) | The anchor block is the first chain block 1 minute after the close. |
| Windows sum | 600 + 864,000 + 6,048,000 = 6,912,600 | ≤ D_RET = 12,096,000 | Fixed in the ticket template: any ticket's deposit can be returned 14 days after the sales deadline. |
| Margin | 1.5 (15,000) | ≥ MARGIN_MIN | The floor; jackpot sizing (design 4.6). |
| m (bonus range) | starts at 3, may move within 2–50 | m_max ≤ M_DRAW = 50 | The draw picks the next round's m from sales. |
| Price, deposit | 1 KAS, 0.13 KAS | deposit ≥ D_FLOOR | As M8. |
| s_C, s_D (caller rewards) | 0.5 % each of the round's sales (owner may raise) | room check passes up to at least 3 % | See "Who runs the rounds". |
| Referral default | the owner's `kaspatest:` address (**owner to supply**) | page setting `VITE_REFERRAL_ADDRESS`, not genesis | Links without `?ref=` pay the owner's address the referral share. |
| Fee address | the owner's `kaspatest:` address (`--fee-address`; default: the funding key's) | Schnorr address | Receives the protocol fee s_F. |
| J(1), V0 | 10 KAS jackpot, 100 KAS vault (owner may raise) | L(1) ≤ 20 % of V0 | The first jackpot, and the house vault that backs it. |
| **Round length** | **owner's choice**: `fast` 1 h or `steady` 24 h | none at genesis | See below. |
| d_min | half a round | T' = max(T + round_len, close + d_min) | A close within half a round keeps the next deadline on the same clock time. |

**One rule the owner must keep: close every round within 6 days of its sales deadline.** A later close pushes the
claim deadline past the day when anyone may return a ticket's deposit. From then on a winning ticket that is still
unclaimed could be returned by someone else, and its prize lost. M8 found this (U6, D102); the arithmetic is
14 days − (1 min + 24 h + 7 days) ≈ 6 days.

## Option A, "steady": one round a day (recommended)

- **For players:** sales run 24 hours. The draw comes about 1–2 minutes after the close; the page shows the
  numbers as "Provisional" until the draw is on chain. Winners then claim within about 8 days. Every ticket's
  deposit comes back after 14 days, or with the last claim.
- **Owner's work:** once a day, open Rounds after the deadline and press **Close**. About 2 minutes later, press
  **Draw**. Both use KasWare or the quick wallet. Pressing late is fine (within 6 days, see above); the round just
  stays closed for sales until then.
- **Test-KAS, about 0.65 KAS a day in fees.** M8 measured a close at 0.432 KAS and a draw at 0.222 KAS, and the
  rewards at test sales are smaller than that (DECISIONS 2026-10-27).
- **Plus reserves that come back.**
  - The close posts R = 1.69 KAS for the marker; it comes back at the draw.
  - The draw posts 1.69 KAS for each prize pool it creates (only tiers someone holds; at most 7). That comes
    back when the pool is absorbed by a later draw after its claim deadline (about 8 days later), or with
    its last claim.
  - So at most about 7 × 1.69 ≈ 12 KAS a day, about 100 KAS in flight.
- **Budget:**
  - genesis 138 KAS;
  - about 100 KAS of reserves in flight;
  - about 20 KAS of fees a month;
  - test buys for the journeys (each about 1.2 KAS).
  - **About 300 test-KAS is plenty.** The wallet has 4,789.

## Option B, "fast": one round an hour

- **For players:** a draw every hour, so testers see results the same day. The claim and deposit windows are the
  same as in A: 7 days, and 14 days for deposits.
- **Owner's work:** a Close and a Draw press **every hour**, 48 presses a day. Nobody else will press: the reward
  is smaller than the fee at test sales. Rounds nobody closes simply wait, with sales stopped.
- **Test-KAS:** about 0.65 KAS an hour, about 15.6 KAS a day. Reserves of up to 12 KAS an hour come back after
  about 8 days, so up to about 2,500 KAS could be in flight after a week of busy rounds.
- **Use it for a launch day or a test session**, not for weeks.

## Option C, "steady" with bigger numbers

As A, with a 50 KAS jackpot (V0 300 KAS, about 340 KAS at genesis) or a 100 KAS jackpot (V0 600 KAS, about
640 KAS). The vault money is the owner's position: it can be withdrawn later through the Vault page. Raising
s_C and s_D from 0.5 % to 3 % makes each reward six times larger. At the test sales that paid 0.085 KAS for a
close (DECISIONS 2026-10-27), a close would pay about 0.51 KAS against its 0.43 KAS fee. With fewer sales it
still loses a little. Both are fixed at genesis.

## What the owner needs to give

1. **Profile:** A (daily, recommended), B (hourly) or C.
2. **The referral and fee address:** one or two `kaspatest:` addresses of the owner's (KasWare's testnet account
   is fine).
3. **Jackpot and vault size** if not 10 / 100 KAS, and the caller shares if not 0.5 %.
4. **The go-ahead** for the genesis transaction. It is one transaction on testnet-10 and spends about 138
   test-KAS.
