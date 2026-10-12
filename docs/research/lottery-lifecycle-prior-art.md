# Lottery lifecycle prior art: freeze, randomness, settlement

Date: 2026-10-12. Scope: how existing lotteries, raffles and round-based games order "sales freeze", "randomness known" and "settlement", and whether any sound design merges freeze and settlement into one transaction. This is a survey. It proposes no design for our lottery.

Labels: VERIFIED = I fetched the page and read the stated content. UNVERIFIED = search snippet only, or a page that did not state the point.

Our lifecycle for reference (docs/design.md 3.1, 7.2, 7.3): `buy()` -> `close()` (spends all shards, which freezes counts, and opens the next round in the same transaction; stores an absolute seed reference) -> `draw()` (separate transaction, after the seed block exists) -> `claim()`.

## Comparison table

Columns: (a) freeze is a transaction, a clock or implicit; (b) randomness source and when it becomes known; (c) distinct public calls per round; (d) entries between deadline and freeze; (e) documented exploit or griefing.

| Design | (a) Freeze | (b) Randomness | (c) Public calls per round | (d) Late entries | (e) Documented issues |
|:--|:--|:--|:--|:--|:--|
| lley154 Cardano lottery (Plutus) [1] VERIFIED | Transaction: StopBuy (admin) moves the buy token away from the buy validator; then Close records the close tx hash | Hash of the Close transaction. The README says it is unknowable during the buy phase because the close tx does not exist yet | 12 states per cycle (Init, Open, StartBuy, Buy, StopBuy, Close, Draw, Redeem, CalcPayout, Payout, Collect, End); most are admin-only | Rejected: buying works only while the lotto is Open and the buy token sits at the buy validator. No late-entry rule stated | README states the admin can predict the number (deterministic chain, and the admin chooses the close tx) and recommends an oracle. Repo archived 2024-09-05, beta |
| Ergo ticket-serial lottery (ErgoDocs concept) [2] VERIFIED | Not defined. After sales end, the first unsold serial n is stored in R4 | "Randomness from the previous block header", for example; winner uniform in 0..n-1 | Not specified | Not covered; no tickets after close by design | Docs say it is "not deterministic" as it depends on the last block header, and that the current winner must be online. Presented as a concept, no code |
| Substrate `pallet_lottery` [3] VERIFIED | Clock: `block < start + length` else `AlreadyEnded` | `T::Randomness::random` at the payout block `start + length + delay`; source depends on the runtime | 1 buy call. Payout runs in `on_initialize`, so no settlement call | Rejected, but silently: the wrapped call still executes and no ticket is issued ("may fail silently") | Code comment `TODO: deal with randomness freshness`; modulo bias is "best effort only"; zero-ticket payout may panic (my reading) |
| MegaPot V2 (Base) [4][5] VERIFIED (contract code), docs page thin | Transaction: `runJackpot()` sets `jackpotLock` after `drawingTime` | Pyth entropy; callback `scaledEntropyCallback` accepted only while locked | 2 (`runJackpot`, entropy callback), plus buys | Rejected while locked: `buyTickets` reverts `JackpotLocked`. Between `drawingTime` and `runJackpot`, buys still land in the current drawing | Owner `unlockJackpot()` clears the lock and allows a new `runJackpot`. Emergency mode refunds only current-drawing tickets and the callback has no `noEmergencyMode` guard, so a late callback can still settle (my reading of the code). Code4rena contest "Known Issues": emergency mode not recoverable |
| Chainlink VRF consumer pattern [6] VERIFIED | Transaction: the request; guidance says stop accepting inputs after it. Raffle examples use a CALCULATING state [7] UNVERIFIED (snippets) | VRF fulfillment, written after N confirmations | 2 (`performUpkeep`/request, fulfillment) | Rejected by a state guard (examples [7]) | Docs: re-request or cancellation of randomness is incorrect use; chain rewrites can swap the request into another block, so the confirmation count must be chosen for the value at risk |
| PoolTogether v5 [8][9] VERIFIED (PrizePool.sol), docs partly | Clock: draws close on a schedule (`drawClosesAt`); the pool is never frozen | RNG requested and relayed by auctioned bots, then `awardDraw(winningRandomNumber)`, which reverts before the draw has closed | 2 timed auctions per draw (start RNG, finish RNG), then per-winner `claimPrize` | Counted for the next draw: `contributePrizeTokens` assigns to the currently open draw. Balances are time-weighted (TWAB), aligned to draw periods | Winner is decided at claim by `isWinner` against the stored winning number (body not read). TWAB page notes a user could manipulate an average balance if a draw ended before its period [9]. 2-month draw timeout proposal for Optimism [10] UNVERIFIED |
| Polkadot / Kusama candle auction [11][12] UNVERIFIED mechanism; no-winner event VERIFIED on the digest page | Clock plus retroactive: an ending period is fixed, but the real end block is chosen afterwards | VRF run after the ending period picks the end block | Auction settles itself | Voided at settlement: bids after the retroactive end block do not count | The August 2023 Polkadot Digest says the last Kusama parachain auction had no winner because all bids came after the retroactively determined end [12] |
| Classic Ethereum future-blockhash lotteries [13][14] UNVERIFIED (snippets) | Commit to a block N blocks ahead | Hash of that future block, readable only 256 blocks | Commit tx plus a reveal tx inside the lookback window | Entries after the commit block are not allowed | SmartBillions: a result resolved after 256 blocks returned `000000`, which won the jackpot [15] UNVERIFIED. Proposer bias and RANDAO omission are described in [16] VERIFIED |
| Multiparty Bitcoin lotteries, Bartoletti-Zunino [17] UNVERIFIED (abstract only) | Participants join in a commit phase | Joint, from the players' secrets | Interactive; not read | Not read | Abstract only: earlier protocols need deposits growing quadratically; theirs is constant per player. Timeout handling not read |
| Nervos CKB, Bitcoin CTV/CAT, Kaspa SilverScript examples | -- | -- | -- | -- | Nothing found. See "Not found" |

## Not found

- Nervos CKB: no lottery or raffle design found in search results (UNVERIFIED, absence of results).
- Bitcoin covenants (CTV, CAT, CSFS): no lottery or raffle design found. Search results only cover general covenant background [18] UNVERIFIED.
- Kaspa: the Toccata SilverScript page shows only Vault and Token examples [19] VERIFIED. The `silverscript-lang/tests/examples` listing I could read (truncated, 86 of 103 names shown) contains escrow, last will, mecenas, kcc20, hodl vault and others, and no lottery [20] VERIFIED for the shown names. The kaspaexplained build page has no lottery or randomness mention [21] VERIFIED. kascov's canonical covenants are Mecenas, Escrow and LastWill [22] UNVERIFIED. I found no Kaspa lottery repository.
- I found no design that merges "freeze" and "settlement" into a single transaction while the entry window is still enforced by anything other than a clock or a state flag.

## Findings relevant to the two-functions question

1. **No surveyed design settles in the same step that freezes.** Every design that has an unpredictable random source separates the freeze event from the point where randomness exists. MegaPot (lock then callback) [5], Chainlink (request then fulfillment) [6], Cardano (StopBuy, Close, then Draw) [1], PoolTogether (close, RNG, award) [8], Substrate (end, then payout after a delay) [3]. In all of them the delay exists so randomness is not known while inputs are still accepted. VERIFIED for these five. This matches our `close()` / `draw()` split.

2. **Where freeze and randomness coincide, the freeze event itself is the seed.** The Cardano lottery uses the hash of the Close transaction as the seed [1], so freeze and randomness become known in the same transaction. The cost is documented in the same README: the admin who builds the close transaction can predict or grind the number. VERIFIED. In a permissionless setting the transaction builder would be the player-adversary instead of an admin.

3. **The freeze in surveyed systems is a flag or a clock, never a consumed set of objects.** Cardano uses the buy token, MegaPot a boolean, Substrate a block-number comparison, PoolTogether a time schedule. None has our property that the freeze transaction consumes every shard. Our `close()` is a stronger freeze than any surveyed system, and no source documents a one-transaction variant of it. This is a gap in the literature I found, not a proof.

4. **"Void or defer late entries" exists in two forms.**
   - Counted for the next round (PoolTogether): late deposits go to the open draw, and nothing is voided [8]. VERIFIED.
   - Silently ignored after the end (Substrate): no ticket issued, payment-side effects still run, so the buyer may lose the wrapped call's effects without a ticket [3]. VERIFIED. This is a documented user-harm risk of rejecting late entries without a refund path.

5. **Voiding entries at settlement (not prevention) is documented only in candle auctions.** Polkadot/Kusama bids after a retroactively chosen end block do not count [11][12]. Cost reported: the final Kusama auction had no winner because all bids came after the ending [12]. The mechanism needs the random end to be chosen after the window, so it relies on a post-window randomness source. The mechanism is UNVERIFIED beyond snippets; the no-winner event is on a page I read. I found no lottery that voids late entries at settlement.

6. **Sound lifecycle rule stated by Chainlink: stop accepting inputs once the randomness request exists, and do not allow re-request or discard.** [6] VERIFIED. This is the closest published statement of our requirement "counts frozen before the seed reference" and "no re-roll on a missed draw".

7. **Missed-settlement handling is a recurring weak point.** MegaPot: a stuck lock is cleared only by an owner call or emergency mode, and the late callback can still settle during emergency mode [5]. PoolTogether: a draw-timeout proposal that splits remaining prizes by TWAB [10] UNVERIFIED. Ethereum blockhash lotteries: a reveal outside the 256-block window gives a degenerate value, exploited in SmartBillions [15] UNVERIFIED, [16] VERIFIED for the 256-block limit. This corresponds to our parked "nobody calls draw() in time" item (design section 10, KIP-21 12 h window).

8. **Deferred winner determination is a settled pattern.** PoolTogether's `awardDraw` stores only the winning number and `claimPrize` checks `isWinner` at claim time [8]. VERIFIED (partly; `isWinner` body not read). Our `draw()`/`claim()` split follows the same shape.

9. **Commit-reveal lotteries need a last-revealer defence.** The single-transaction reveal does not work with a future block hash, because the hash does not exist until a later block [14][16]. UNVERIFIED for [14], VERIFIED for [16]. A "reveal and settle in one call" design therefore needs participants' own secrets, which brings the last-revealer abort problem.

10. **Kaspa-specific prior art is absent.** No Kaspa lottery, raffle or randomness example was found in docs, SilverScript examples or community pages [19][20][21]. Our design cannot lean on a published covenant precedent for any of these three events.

## Sources

1. https://github.com/lley154/cardano-lottery (and README: https://github.com/lley154/cardano-lottery/blob/main/README.md)
2. https://docs.ergoplatform.com/uses/lottery/
3. https://crates.parity.io/src/pallet_lottery/lib.rs.html
4. https://github.com/code-423n4/2025-11-megapot (README; DOCUMENTATION.md gave no lifecycle detail)
5. https://raw.githubusercontent.com/code-423n4/2025-11-megapot/main/contracts/Jackpot.sol
6. https://docs.chain.link/vrf/v2/security
7. https://codehawks.cyfrin.io/c/2024-03-snek-raffle/s/98 and https://codehawks.cyfrin.io/c/ai-puppy-raffle-cmd1osxjd0000jo04kon8udfk/s/5 (search snippets only)
8. https://github.com/GenerationSoftware/pt-v5-prize-pool/blob/main/src/PrizePool.sol and https://dev.pooltogether.com/protocol/design/draw-auction
9. https://dev.pooltogether.com/protocol/design/twab-controller
10. https://gov.pooltogether.com/t/pooltogether-v5-relaunch-optimism-param-recommendations/3232 (snippet only)
11. https://wiki.polkadot.network/docs/learn/learn-auction (redirects to wiki.polkadot.com; the fetched page no longer describes candle auctions; mechanism taken from search snippets of the support.polkadot.network article and polkadot.network blog "making-history-again-polkadot-auctions-1-5")
12. https://forum.polkadot.network/t/polkadot-digest-11-aug-2023/3693
13. https://www.cachemania.com/randomness-sourcing-methods-applied-in-ethereum-lottery-games/ (snippet only)
14. https://rareskills.io/post/generate-a-random-number-with-solidity-on-the-blockchain (snippet only)
15. https://hackenproof.com/vulnerabilities/5bd16ff175fa742a48d954c6 (snippet only; dating inconsistent across results)
16. https://www.paradigm.xyz/2023/01/eth-rng
17. https://eprint.iacr.org/2016/955
18. https://hashrateindex.com/blog/op-ctv-bip-119-guide/ and related covenant background pages (snippets only)
19. https://docs.kaspa.org/toccata/silverscript
20. https://github.com/kaspanet/silverscript/tree/master/silverscript-lang/tests/examples
21. https://kaspaexplained.com/build-on-kaspa
22. https://kascov.io/guide (snippet only)
