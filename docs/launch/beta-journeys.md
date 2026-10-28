# Testnet-10 journeys J1–J7: checklist and log (spec frontend 8 "Testnet-10", "Done when")

Run these on the live beta site (docs/launch/site-deploy.md) against the beta instance (docs/launch/beta-plan.md),
once with **KasWare** (testnet-10 account, network `kaspa_testnet_10`) and once with the **quick wallet**. Add
Kaspire as a third pass if it is installed.

For every step, write down:
- the date and time;
- the wallet;
- the **transaction ID**: the page shows it under Transaction progress or in the ticket card; copy it from
  there;
- what the screen showed: ok, or the error's code (each error shows a code, for example `node_unavailable`).

A step passes when its transaction is accepted. Open the explorer link next to the ID, or check the ticket or row
on the page.

**The pace (profile `beta8h`, docs/launch/beta-plan.md):** a round every 8 hours. Sales of a round close at its
deadline; you close and draw it (J4) within 4 hours to keep the clock, so its winning numbers come a couple of
minutes after your Close. Plan the journeys around three deadlines a day: buy (J1, J7) before a deadline, run J4
right after it, then claim (J2) and verify (J6). A losing ticket's deposit (J2.4) comes back 14 days after its
round's deadline.

Before starting:
- [ ] The site opens on `https://…` with the testnet-10 badge.
- [ ] `/health` of **each** API listed in `VITE_INDEXER_URL` shows `"ok":true`.
- [ ] KasWare is on its testnet-10 network and holds about 20 test-KAS from the faucet.

## J1: new player buys a ticket

| # | Do this | Expect | KasWare: txid / result | Quick wallet: txid / result |
|---|---|---|---|---|
| 1.1 | Open the site through a referral link: `https://<site>/?ref=<a kaspatest: address not your own>` | Play opens. The cost panel later shows a Referral row to that address. | | |
| 1.2 | **Connect** → **KasWare**, approve in KasWare. (Quick wallet: menu → Wallet → **Create**, then the faucet link, and fund it with 10 test-KAS.) | Connected address in the header. | | |
| 1.3 | Pick 5 numbers and a bonus, **Quick pick** for a second combination, **Add** up to 4 combinations | Ticket shows 1–4 combinations. | | |
| 1.4 | Read the cost panel: Price + Deposit + Network fee = Total; Referral and Back date are parts of it | The figures add up. | | |
| 1.5 | **Buy**, approve in KasWare (it shows one input to sign) | Progress: built → signed → sent → accepted. Note the txid. | | |
| 1.6 | Repeat with b = 1, without a referral link (open the site plainly) | Referral to the owner's default address only if it is at least 0.1 KAS; below that, no Referral row. | | |
| 1.7 | **My tickets** | The ticket card shows the round and the combinations. | | |

## J8: indexer fallback (several URLs, F6 part E)

Only when `VITE_INDEXER_URL` lists two or more APIs. Use a fresh private window for each row.

| # | Do this | Expect | Result |
|---|---|---|---|
| 8.1 | Stop the first API (`docker compose stop` or `systemctl stop indexer-api` on its server), open the site | The round card loads (after at most 15 s); Rounds lists the rounds. | |
| 8.2 | Reload the page | It loads at once: this browser now asks the second API first. | |
| 8.3 | Start the first API again; stop the second; reload | Loads from the first again. | |
| 8.4 | Both stopped | The page shows the indexer as unavailable (`indexer_unavailable`); My tickets still claims from a local copy. | |

## J4: caller runs the round (after the sales deadline)

| # | Do this | Expect | txid / result |
|---|---|---|---|
| 4.1 | **Rounds** → the closed-for-sales round → the panel shows **Close** due, the reward and the fee | Figures shown before signing. | |
| 4.2 | **Close**, approve, within 4 hours of the deadline | Accepted. The next round opens; its deadline is 8 hours after the last one. | |
| 4.3 | About 1–2 minutes later the panel shows **Draw** due | "Anchor proof" is available (the hosted API builds it). | |
| 4.4 | **Draw**, approve | Accepted. Winning numbers final; pools listed. | |
| 4.5 | **Old M8 instance (absorbing draw)**: see "Old M8 instance checks" below | | |

## J6: sceptic verifies

| # | Do this | Expect | Result |
|---|---|---|---|
| 6.1 | Rounds → the drawn round → **Verify in this browser** | Every item "match": the winning numbers from the anchor hash, the roots from the trees, the pools. | |
| 6.2 | The anchor block row | Checked against a public node's block. | |

## J2: returning player claims

| # | Do this | Expect | txid / result |
|---|---|---|---|
| 2.1 | **My tickets** after the draw | The winning numbers, with matches highlighted. Each winning combination has a claim row with its fee. | |
| 2.2 | **Claim** on a winning combination (if any; buy many combinations to raise the chance) | Accepted, payout in the wallet. The row shows Claimed. | |
| 2.3 | The last claim of a ticket: the row says the deposit comes back with it | Deposit back in the same transaction. | |
| 2.4 | A losing ticket: **Return deposit** appears only after its date (14 days after the sales deadline) | Accepted, deposit back. Log it when the date comes. | |

## J3: new device

| # | Do this | Expect | Result |
|---|---|---|---|
| 3.1 | Menu → Backup → **Download** | A file `kaspalionare-backup…`. | |
| 3.2 | In a private window (or another browser): open the site, Backup → **Import backup file** | "Tickets kept" N, the same as before. | |
| 3.3 | Instead of the file: connect the same wallet | My tickets finds the tickets through the API, by payout address. | |
| 3.4 | A claim or return from the imported copy (if one is due) | Accepted. | |

## J5: backer

| # | Do this | Expect | txid / result |
|---|---|---|---|
| 5.1 | **Vault**: price, liability, the position (none yet) | Figures shown. | |
| 5.2 | **Request deposit** of 10 KAS, approve | Accepted. The request is listed. | |
| 5.3 | After the next draw: **Settle** | Accepted. The position shows shares. | |
| 5.4 | **Request withdrawal** (Part, between Least and Most), then after the next draw **Settle** | Accepted. KAS back. | |

## J7: referrer

| # | Do this | Expect | Result |
|---|---|---|---|
| 7.1 | Menu → the referral link item → copy | `https://<site>/?ref=kaspatest:<your address>`. | |
| 7.2 | Open it in a private window and buy (as J1.5) | The cost panel shows Referral to your address; the referral output arrives. | |

## Old M8 instance checks (owner decisions 2026-10-26 and the F5 review)

These run against the **M8 instance** (covenant ID `e9f4f810…223988d6`), not the beta. Its rounds 1–8 are long
past their claim deadlines and round 9 is open. Build the page with the M8 values in a local preview (in `web/`):
- `VITE_COVENANT_ID=e9f4f810c86467e7768aa1b42e6c62182b8a0d934bda922bdbb2804f223988d6`;
- `VITE_INDEXER_URL`: an indexer API following a node on a **copy** of the M8 store, run as
  `indexer-api --store <copy of tn10-data/e9f4…/> --cov … --node resolver`. The M8 store's resume block (the last
  soak sitting, 2026-10-25) may already be pruned by the node; if `/health` reports "retention root", first run
  `tn10 campaign reanchor` on the M8 instance (it is `tn10-data/current`), then copy the store again;
- then `npm run build` and `npx vite preview`.

Never put this build on Vercel.

| # | Do this | Expect | txid / result |
|---|---|---|---|
| M.1 | **`return_deposit()` on an expired M8 ticket.** My tickets must hold an M8 ticket of rounds 1–8 that won nothing (see the note below), then press **Return deposit** on it. | Accepted. The deposit (0.13 KAS) goes to the ticket's payout address, whoever submits. | |
| M.2 | **An absorbing draw.** Round 9 of M8: after its deadline, **Close** (J4.2), then **Draw**. The panel's "Expired pools" row lists the expired pools of rounds 1–8, oldest first, at most 7. | Accepted. On the explorer, the draw spends those pools, and their reserves (1.69 KAS each) go back to their funders. | |

**The M8 store is re-anchored** (F6 part E, 2026-10-28): `tn10 campaign reanchor --check` proved on a public node
that the instance's 383 live UTXOs were unchanged, then `tn10 campaign reanchor` moved the store's resume point
from the pruned block (DAA 605,601,500) to block `05e00563…05f9c748` (DAA 608,152,477) and caught up. Only local
files changed (`tn10-data/e9f4…/`; a copy of the store from before is in that session's scratchpad). The node
prunes after about 30–34 hours, so start the M8 indexer API (`--node resolver`) on a copy of the store within a
day, or re-anchor again first.

Note for M.1 (**owner choice open**): every M8 ticket pays the M8 campaign's key, and **that key is the main
test key** (`TN10_PRIVATE_KEY`, `kaspatest:qqdasdj3…85tms5s`): the campaign was funded and run with it, and it
also funds the beta's genesis. So "move the M8 key's balance to the main key" has nothing to move, and importing
that key into a browser would put the main test key (4,789 test-KAS, the beta's funding) in it. Nothing was moved
or imported. My tickets finds tickets by the connected wallet's payout address (J3), so today the page sees them
only when connected as that key. `return_deposit()` itself pays the ticket's payout whoever submits, so the key is
not needed to sign it. Ways forward (docs/specs/frontend-handoff.md "F6 part E"):
- **A. A new main key** (recommended): `tn10` gets a fresh `TN10_PRIVATE_KEY`, the old key is kept as the M8 key,
  and about 4,739 test-KAS move from the old to the new one (a second testnet transfer, to approve). Then the old
  key, holding about 50 test-KAS, is imported into the throwaway profile as planned. About an hour of work.
- **B. A backup file of the M8 tickets**, written by a new `tn10` command from the M8 store and imported into any
  browser (Backup → Import); no key leaves `.env`, and the page checks every record against the chain. A ticket
  paying another address shows its Payout row; whether Return deposit is offered for it must be checked. About
  half a day of work.
- **C. Import the main key into the throwaway profile anyway:** no work; the risk is a browser holding the key
  that funds the beta (testnet coins only, no real money).

## Sign-off

| Journey | KasWare | Quick wallet | Kaspire |
|---|---|---|---|
| J1 | | | |
| J2 | | | |
| J3 | | | |
| J4 | | | |
| J5 | | | |
| J6 | | | |
| J7 | | | |
| J8 indexer fallback | | | |
| M.1 return_deposit (M8) | | | |
| M.2 absorbing draw (M8) | | | |
