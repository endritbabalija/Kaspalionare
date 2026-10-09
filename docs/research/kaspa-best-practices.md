# Hardening a well-aligned Kaspa lottery design

**Verdict: the lottery's design is already close to what Kaspa's core developers recommend.** Keeping state inside the script behind a P2SH address, using covenant IDs for lineage, splitting tickets across K shards, settling everything in one atomic close, using absolute DAA-score deadlines and holding a fixed reserve in every UTXO all match official guidance. They also match what the most mature UTXO ecosystem (Cardano) learned the hard way ([docs covenant-state](https://docs.kaspa.org/toccata/covenant-state); [Decision Guide](https://docs.kaspa.org/toccata/decision-guide); [Cardano resource exhaustion](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/resource-exhaustion/)). The risks are in the details. The lottery mixes several kinds of UTXO in one protocol, which SilverScript's convenience macros explicitly do not cover. Its "output 0 has our covenant ID" check is weaker than the binding the official examples use. And the M0 measurements line up exactly with Kaspa's **per-block** mass limits. If the node's **per-transaction** relay limit (100,000 grams in KIP-9) still applies, close() and small tickets could be refused by the public network. No official standard yet exists for lotteries, vaults, admin keys or wallet display. The community conventions (KCCs) are all unmerged drafts, and no covenant lottery exists on Kaspa yet. So the project can follow core guidance and must document the rest of its choices itself.

The five changes that matter most, in order:

1. **Verify the mass limits before anything else** (items G1–G3 below). M0's reserve, ticket-deposit and max-K numbers appear to be sized against block limits. If transaction-level standardness applies, reserves may need to be about 5× larger, or close() redesigned.
2. **Write the cross-role checks by hand, template by template** (A2–A4, B1–B4). Every entry point must pin exact covenant input and output counts. Every read of another role must use the template-checked builtins, and outputs must be selected through the authorized-output index.
3. **Close the double-satisfaction gap for plain payouts** (D1–D3). Winner payouts, referral and fee outputs carry no covenant binding, so they must be paired by fixed position or summed per payee.
4. **Bound everything a single transaction must gather** (E4, F2, I3). This means late buys after the deadline, pending vault requests per draw, and claim deadlines with a permissionless sweep. That way no step can be stalled, and no keeper bot is ever required.
5. **Pin and re-check the tooling** (N1–N4). The pragma must stay `^0.1.0` even on SilverScript 1.0.0. SilverScript is built against a different rusty-kaspa revision than our v2.1.0. Security fixes land only on SilverScript's master branch.

## A two-minute glossary

| Term | Plain meaning |
|---|---|
| UTXO | A "coin" on Kaspa: an unspent output holding KAS and locked by a script. Spending it destroys it and creates new outputs. |
| Covenant | A script that controls not just *who* can spend a UTXO but *what the spending transaction must look like*. |
| Covenant ID (KIP-20) | A consensus-enforced family tag. An output can carry ID X only if the input that authorizes it already carries X, or if X is freshly derived at genesis ([KIP-20](https://github.com/kaspanet/kips/blob/master/kip-0020.md)). It proves lineage, not correctness. |
| Authorizing input / authorized outputs | Each covenant output names the one input that authorizes it. Scripts list "my authorized outputs" with OpAuthOutputCount/OpAuthOutputIdx, and "all outputs of this family" with OpCovOutputCount/OpCovOutputIdx. |
| P2SH / redeem script | The chain stores only a hash of the script. The spender reveals the full script, including the embedded state. A new state means a new address, so the address is not a stable name and the covenant ID is ([docs covenant-state](https://docs.kaspa.org/toccata/covenant-state)). |
| Template / template hash | The fixed code around the state bytes. A template hash identifies "this kind of contract" independent of its current state. |
| Leader / delegate | The pattern for a transaction that spends many family members. One "leader" input checks the whole transition. The others ("delegates") only check that a genuine leader is present. |
| Storage mass | KIP-9's charge for creating UTXOs, roughly 10^12 ÷ output value (in sompi) per UTXO-size unit. It makes small outputs expensive ([KIP-9](https://github.com/kaspanet/kips/blob/master/kip-0009.md)). |
| Plurality | A large UTXO counts as several. Covenant-bound P2SH UTXOs count as **2** (130 bytes), so they cost 4× the storage mass of a plain output of the same value ([rusty-kaspa mass/mod.rs](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/consensus/core/src/mass/mod.rs)). |
| Transient mass | Transaction size × 4. It caps how many bytes (scripts, proofs) one block can carry ([KIP-13](https://github.com/kaspanet/kips/blob/master/kip-0013.md)). |
| Compute budget / script units (SU) | A per-input allowance of script work. 1 budget unit = 10,000 SU, plus 9,999 free SU per input ([docs script-pricing](https://docs.kaspa.org/toccata/script-pricing)). |
| Standard vs consensus | "Consensus" rules decide whether a block is valid. "Standard" rules are node policy that decides whether a transaction is relayed by the public mempool. |
| DAA score | Kaspa's block-count clock, used for absolute deadlines. |
| Seqcommit | KIP-21's per-chain-block commitment. Scripts can read it with OpChainblockSeqCommit for a limited time. |
| Double satisfaction | One output satisfies the checks of two different inputs, so an attacker pays once and collects twice ([Cardano double satisfaction](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/double-satisfaction/)). |
| KIP vs KCC | KIPs change consensus. KCCs ("Kaspa Calls for Conventions") are non-binding app conventions ([kaspanet/kccs](https://github.com/kaspanet/kccs)). |

## What exists: four tiers of authority, only one of them ratified

Kaspa's covenant guidance comes in four tiers, and they carry very different weight. **Only the consensus KIPs and the node code are binding.** docs.kaspa.org says so itself: "The sources of truth are the KIPs and the implementation. When the guide and code disagree, follow the code and fix the guide" ([docs references](https://docs.kaspa.org/toccata/references)). Everything else is guidance, a draft, or one project's habit.

### Official core: ratified or shipped

| Item | Label | Status and date | What it gives us |
|---|---|---|---|
| Toccata hard fork (KIP-16, 17, 20, 21) | Official | All four marked Active on kips master by PR #45, merged 2026-07-15 ([kips PR #45](https://github.com/kaspanet/kips/pull/45)). Mainnet activation at DAA score 474,165,565, about 2026-06-30 ([toccata-guide](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/docs/toccata-guide.md)). | The opcodes we use, covenant IDs, and the seqcommit accessor. |
| KIP-17 covenant opcodes | Official, Active (updated 2026-05-31) | Introspection, OpCat/OpSubstr, OpMul/Div/Mod, BLAKE2b/BLAKE3 (keyed variants), OpTxInputDaaScore. Limits raised to 1,000,000 bytes and ops ([KIP-17](https://github.com/kaspanet/kips/blob/master/kip-0017.md)). | Merkle-proof hashing, arithmetic, and time introspection. |
| KIP-20 covenant IDs | Official, Active (updated 2026-05-28) | Consensus lineage plus **non-normative** patterns: bounded and unrolled output checks, singleton, leader/delegate ([KIP-20](https://github.com/kaspanet/kips/blob/master/kip-0020.md)). | The backbone of our state authentication. |
| KIP-21 partitioned seqcommit | Official, Active | OpChainblockSeqCommit is readable only within finality depth (about 12 h) and only on the selected chain ([accessor spec](https://github.com/kaspanet/kips/blob/master/kip-0021/seqcommit-accessor.md)). The proving spec is a "Reserved" placeholder. | Our seed reference and its time window. |
| KIP-9 storage mass, KIP-13 transient mass, KIP-10 introspection | Official, Active (Crescendo era) | Still govern every covenant transaction ([KIP-9](https://github.com/kaspanet/kips/blob/master/kip-0009.md); [KIP-13](https://github.com/kaspanet/kips/blob/master/kip-0013.md)). | Reserve sizing and max K. |
| rusty-kaspa v2.1.0 | Official release | 2026-09-22. Post-activation cleanup: covenant verification is unconditional and the activation override was removed ([v2.1.0 release](https://github.com/kaspanet/rusty-kaspa/releases/tag/v2.1.0)). | Our pinned node. It is the latest official release. |

### Official core: drafts, guidance and pre-stable tooling

| Item | Label | Status and date | What it gives us |
|---|---|---|---|
| KIP-24 (transaction v1) | Official draft | Unmerged PR #41, header "Implemented", created 2026-01-27 ([kips PR #41](https://github.com/kaspanet/kips/pull/41)). docs.kaspa.org links it as if it were on master. | Per-input compute_budget and the v1 txid (BLAKE3). |
| KIP-25 (compute budget and pricing) | Official draft | Unmerged PR #42, "Draft", but implemented in code ([kips PR #42](https://github.com/kaspanet/kips/pull/42)). | SU prices: BLAKE2b 2 SU/byte, BLAKE3 1 SU/byte, 100,000 SU per signature. |
| KIP-12 wallet API (`kaspa:signPskt`) | Official draft, now migrating | kips PR #44 DRAFT, last commit 2026-08-10 ([kips PR #44](https://github.com/kaspanet/kips/pull/44)). Sutton moved the wallet API into the KCC process on 2026-07-15 ([forum](https://kas-smiths.org/t/wallet-integration-protocol/147)). | How wallets sign only their own inputs in a covenant transaction. |
| docs.kaspa.org/toccata (Dev Guide, Covenant State, Decision Guide, Agent Brief, Script Pricing, Transaction v1) | Official guidance (non-normative) | Source repo commit 2026-07-02. It describes itself as "a synthesis" ([docs toccata](https://docs.kaspa.org/toccata)). | The clearest design-pattern advice and pitfall lists. |
| SilverScript | Core tooling, pre-stable | Tag `v1.0.0` (commit 3ed9733, 2026-09-09) and `v1-rc1` (2026-08-30). No commits since. SECURITY.md still says "experimental… has not reached its first stable release". No audit ([SECURITY.md](https://github.com/kaspanet/silverscript/blob/v1.0.0/SECURITY.md)). | Our language. DECL.md, TUTORIAL.md, UNDEFINED-BEHAVIOUR.md, the KCC20 book and a 12-contract chess app are the de facto style guide. |
| Argent | Core-adjacent, experimental | Actor-style front end expected to become SilverScript's multi-contract layer; "not yet release-ready" ([docs toccata](https://docs.kaspa.org/toccata); [Kaspalytics #1](https://buttondown.com/kaspalytics/archive/issue-1-toccata-live-covenant-usage-a-block/)). | Not usable yet. Watch it for multi-template routing. |
| vprogs | Core prototype | `release-candidate` branch, HEAD 2026-10-08, "early development / prototype phase" ([vprogs README](https://github.com/kaspanet/vprogs/blob/release-candidate/README.md)). Uses hand-built Rust scripts, not SilverScript. | Good covenant hygiene to copy (positional pinning, Merkle domain tags). It is not an SDK. |

### Community drafts (Kas-Smiths forum and kaspanet/kccs)

Kas-Smiths (kas-smiths.org) is a Discourse forum that opened around 2026-07-03. It is the coordination venue for covenant builders, with core developers participating. It names no operator and runs no audit service, and some recent replies are machine-generated ([Welcome post](https://kas-smiths.org/t/welcome-to-kas-smiths/5); [Audit jury thread](https://kas-smiths.org/t/would-a-project-audit-jury-be-a-good-implementation-for-kas-smiths-at-this-time/83)). The kaspanet/kccs repo was announced 2026-07-15. Its README states that acceptance "does not make the convention protocol ground truth" ([kaspanet/kccs](https://github.com/kaspanet/kccs)). As of August, "the only merged pull request is the README" ([Kaspalytics #1](https://buttondown.com/kaspalytics/archive/issue-1-toccata-live-covenant-usage-a-block/)). **No KCC is accepted.**

| Draft | Status and date | Relevance to us |
|---|---|---|
| KCC-01: covenant concepts, byte layout, Program ABI | Draft, PR #3 ([kccs PR #3](https://github.com/kaspanet/kccs/pull/3)) | SilverScript already emits KCC-01 dispatch tags: `blake3("entry(types)")[0..4]` ([DECL.md](https://github.com/kaspanet/silverscript/blob/v1.0.0/docs/DECL.md#L271-L281)). |
| KCC-02: control-principal references (owner, admin, treasury) | Early draft, PR #5, 2026-07-20 ([forum](https://kas-smiths.org/t/kcc-02-control-principal-references-in-program-abis/95)) | How to declare our admin key and fee recipient so wallets can discover them. Covers discovery only, not authentication. |
| KCC20 fungible covenant token | Unversioned draft, open PR, deployed against anyway ([Kaspalytics #2](https://buttondown.com/kaspalytics/archive/issue-2-reading-a-99-drop-a-spec-authors-own-mint/); [forum](https://kas-smiths.org/t/fungible-token-covenant-specification-kcc20/8)) | Relevant only if backer shares become transferable tokens. |
| KCC-0021 token metadata | Draft, PR #6 ([kccs PR #6](https://github.com/kaspanet/kccs/pull/6)) | JSON in the genesis payload. Adopted by kascov and KaspaRocket. |
| KCC-23: metadata bound to covenant ID | Open PR #26, opened 2026-09-12, forum 2026-09-24 ([kccs PR #26](https://github.com/kaspanet/kccs/pull/26)) | A "preparation transaction" payload whose outpoint the genesis commits to. Open issues: JSON canonicalization, look-alike covenants from prep-tx fee outputs, dependency on KIP-24 ([forum](https://kas-smiths.org/t/kcc-23-convention-for-binding-covenant-metadata-to-a-covenant-id/150)). |
| KCC-12: browser wallet provider API | Draft, PR #24 ([kccs PR #24](https://github.com/kaspanet/kccs/pull/24)) | The web wallet flow. |
| `getUtxosByCovenantId` RPC | Forum proposal only, "no core-team commitment" ([forum](https://kas-smiths.org/t/has-there-been-any-discussion-of-adding-getutxosbycovenantid/74)) | Until it exists, we need our own indexer. |

### Single-project conventions and existing dApps

**No covenant lottery or raffle exists on Kaspa L1, Igra or Kasplex** in the sources reviewed, so ours would set the precedent. Web searches surfaced only exchange airdrop campaigns ([KuCoin example](https://www.kucoin.com/announcement/pt-kaspa-listing-campaign-950000-kas-to-be-won)). Igra's launch dApp list has no games ([CryptoPotato](https://cryptopotato.com/igra-network-launches-public-mainnet-as-decentralized-evm-layer-on-kaspas-proof-of-work-blockdag/)). The nearest forum idea is a voter lottery inside a prediction-market sketch ([forum](https://kas-smiths.org/t/permissionless-covenant-based-prediction-markets-high-level-sketch/57)). The closest working analogues are below.

- **The official chess app** inside SilverScript is the best multi-role game reference. It has a League, Player, Mux router, eight move workers and a Settle contract with stakes ([chess_settle.sil](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/tests/apps/chess/chess_settle.sil#L31-L194)).
- **KRON** (testnet launchpad and DEX) reports that one pool UTXO per token serializes trades under load. It uses "template versioning" to upgrade ([KCC20 thread](https://kas-smiths.org/t/fungible-token-covenant-specification-kcc20/8)).
- **kascov.io** is a covenant explorer with compile, preflight and spend-trace replay, plus the `kascov-decode` crate v0.1.0 (2026-09-25) ([kascov.io](https://kascov.io/)).
- **Others:** BitPhoque, kaspa-x402, the Covenant Game Kit, and vprog-tictactoe, a ZK-settled stake game ([vprog-tictactoe README](https://github.com/biryukovmaxim/vprog-tictactoe/blob/master/README.md)).

Mainnet covenant usage is still small: peaks near 9,837 covenant-output transactions a day on 2026-08-01, then 728–773 a day, with a covenant UTXO set around 2,730 ([Kaspalytics #2](https://buttondown.com/kaspalytics/archive/issue-2-reading-a-99-drop-a-spec-authors-own-mint/)).

### Lessons from other UTXO ecosystems

Four outside sources transfer directly:

- **Cardano Developer Portal vulnerability reference.** This is the most complete checklist: double satisfaction, missing UTXO authentication, time handling, unbounded inputs, cheap spam and hash grinding ([overview](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/overview/)).
- **ErgoRaffle.** It is the only well-documented UTXO raffle. It used one contended box and needed backend transaction chaining, which our shards avoid ([ErgoRaffle docs](https://github.com/ErgoRaffle/raffle-documentation)).
- **BIP-345 (OP_VAULT).** Its lesson is that "vaulted values cannot be repurposed to pay for fees" ([BIP-345](https://github.com/bitcoin/bips/blob/master/bip-0345.mediawiki)).
- **ERC-4626 inflation-attack literature.** The vault share-price maths carries over unchanged ([OpenZeppelin](https://www.openzeppelin.com/news/a-novel-defense-against-erc4626-inflation-attacks)).

One single-firm data point is worth weighing. Tweag's review of more than 20 Cardano audits found design and specification flaws the costliest category, and "unclear specification" the second-largest (70 findings) ([Essential Cardano](https://www.essentialcardano.io/article/five-years-of-cardano-smart-contract-audits)).

## Adopt, change, verify: the component-by-component checklist

How to read this table:

- **Adopt** means take on the practice. "(already)" marks practices the design has and must keep.
- **Change** means our design differs from the practice. Change it, or record in DECISIONS.md why not.
- **Verify** means sources conflict or a fact is unconfirmed. Measure it; do not guess.
- **Priority:** *must* = before any real value is at risk; *should* = before public launch; *nice* = later.

IDs are referenced in the hand-off list at the end.

### A. State and covenant IDs

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| A1 | Adopt (already) | Keep state in the redeem-script preimage behind P2SH. Use the covenant ID, never the changing P2SH hash, as identity. | [docs covenant-state](https://docs.kaspa.org/toccata/covenant-state); [KIP-20](https://github.com/kaspanet/kips/blob/master/kip-0020.md) | Matches. KIP-20 explicitly prefers this over keeping state in tx payloads. | must |
| A2 | Change | A mixed-role family (controller, shard, marker, pool, vault, ticket) **cannot use SilverScript's `#[covenant…]` declaration wrappers** for cross-role routes. It "must use handwritten validation with the templated state builtins". | [DECL.md#L519-L536](https://github.com/kaspanet/silverscript/blob/v1.0.0/docs/DECL.md#L519-L536) | Our hand-coded approach is the documented path. Declarations remain fine for same-template hops, such as a shard updating itself. Write this split into the spec. | must |
| A3 | Adopt | Read other roles only with `readInputStateWithTemplate`. Write them only with `validateOutputStateWithTemplate` or `…WithInputTemplate`. Plain `readInputState` "does not independently prove which redeem script… is being decoded". | [builtins.sil#L103-L175](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/std/builtins.sil#L103-L175) | Applies to close() reading shards, draw() reading the marker and vault, and claims reading pools and tickets. | must |
| A4 | Adopt | Template hashes must come from contract state or constants, "not from an untrusted caller". Compute them with `templateHash(prefix, suffix)`, which is length-prefixed and BLAKE3. `blake3(prefix‖suffix)` "is not a valid template hash". | [builtins.sil#L55-L57, #L177-L202](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/std/builtins.sil#L177-L202); [PR #143](https://github.com/kaspanet/silverscript/pull/143) | Store a route table of role template hashes in the controller, as ChessMux packs its eight worker hashes ([chess_mux.sil](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/tests/apps/chess/chess_mux.sil#L53-L299)). | must |
| A5 | Adopt | Give every state an explicit **role byte and round number**. Every consumer checks covenant ID + role + round. | [Cardano missing UTxO authentication](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/missing-utxo-authentication/); [ErgoRaffle](https://github.com/ErgoRaffle/raffle-documentation) | This prevents a ticket-shaped UTXO being presented where a shard is expected, and a last-round UTXO in this round. | must |
| A6 | Verify | The genesis covenant ID commits to **every initially authorized output's value and script**. Later members can only be spawned through an authorizing input. Genesis outputs are invisible to the OpAuth/OpCov iterators but visible to OpOutputCovenantId. | [KIP-20](https://github.com/kaspanet/kips/blob/master/kip-0020.md) | Decide and document two things. First, whether each round is a **new genesis** or a **continuation** of one protocol ID. Second, whether roles share one ID (the chess style) or have separate IDs linked by "spends a UTXO of that ID" (the KCC20/Minter style) ([KCC20 overview](https://github.com/kaspanet/silverscript/blob/master/docs/kcc20-book/src/kcc20-overview.md)). | must |
| A7 | Adopt | Derive unique IDs from the spent outpoint instead of a global counter. | [league.sil#L26-L98](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/tests/apps/chess/league.sil#L26-L98) | Gives ticket and request IDs without contention. | nice |

### B. Transaction shapes and extra outputs

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| B1 | Change | Every entry point pins **exact** `OpCovInputCount` for each family it spends. It also pins exact `OpAuthOutputCount`, plus `OpCovOutputCount == OpAuthOutputCount` (SilverScript's `groups = single`). KIP-20 allows "multiple isolated authorization groups with the same covenant_id" in one transaction. | [DECL.md#L241-L254](https://github.com/kaspanet/silverscript/blob/v1.0.0/docs/DECL.md#L241-L254); [KIP-20](https://github.com/kaspanet/kips/blob/master/kip-0020.md) | Without this, a second protocol UTXO can ride along unvalidated (the Cardano "missed input" class). | must |
| B2 | Change | Select continuation outputs via `OpAuthOutputIdx(this.activeInputIndex, k)`, or assert `OpCovOutputIdx(cov, 0) == 0`. Do not rely only on `OpOutputCovenantId(0) == cov`. | [kcc20-minter.sil](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/tests/examples/kcc20-minter.sil#L17-L85); [vprogs script.rs](https://github.com/kaspanet/vprogs/blob/release-candidate/zk/backend/risc0/covenant/src/script.rs) | Our output-0 check proves *membership*, not *which input authorized it*. The auth index is the exposed equivalent of the missing `OpOutputAuthorizingInput` ([stack_analysis.rs#L326](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/src/compiler/compile/stack_analysis.rs#L326)). Keep `OpOutputCovenantId` only for **genesis** outputs, such as opening the next round if that is a new genesis (A6). | must |
| B3 | Change or record why not | The docs say a script "should be able to find the subset it must validate and ignore the rest". They list "assuming whole-transaction ownership" as a pitfall. | [docs covenant-state](https://docs.kaspa.org/toccata/covenant-state) | Our global "no extra outputs" rule is stricter. It matches the Cardano "ban other scripts" strict form ([Cardano double satisfaction](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/double-satisfaction/)), but it forces fees to come out of covenant value. Pick one rule and write it in DECISIONS.md. **Option 1 (recommended):** exact covenant counts (B1), exact values (B4), plus at most one plain fee input and one change output that carry **no covenant ID**. **Option 2:** keep it strict, and record the reason (for example, that permissionless close() must not carry foreign outputs) and who pays fees. | should |
| B4 | Adopt | Pin the exact `value` of every covenant output. Enforce conservation: Σ covenant inputs = Σ covenant outputs + explicit payouts. Split remainders deterministically (`b = total − a`). | [chess_settle.sil](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/tests/apps/chess/chess_settle.sil#L31-L194); [BIP-345](https://github.com/bitcoin/bips/blob/master/bip-0345.mediawiki) | `validateOutputState` "does not on its own constrain amount or transaction shape" ([builtins.sil#L28-L30](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/std/builtins.sil#L7-L32)). Every prize, fee and referral split needs an explicit remainder rule. | must |
| B5 | Adopt | Use leader/delegate for close() and draw(). The controller is the leader at covenant input 0 and validates the whole group. Shards are delegates: they reject index 0, require `OpAuthOutputCount == 0` when they authorize nothing, and authenticate the leader's template. Every **non-delegate** shard entry requires `OpCovInputCount == 1`. | [DECL.md#L283-L340](https://github.com/kaspanet/silverscript/blob/v1.0.0/docs/DECL.md#L283-L340); [player.sil](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/tests/apps/chess/player.sil#L58-L232) | This stops a shard being "dragged into" a close whose leader runs a different route. The docs warn that leader/delegate "is easy to get subtly wrong by hand" ([docs silverscript](https://docs.kaspa.org/toccata/silverscript)). | must |
| B6 | Adopt | Authenticate every input, and validate the initial state of every newly created UTXO (tickets, requests) at creation. | [Kaspalytics #2 (KCC20 mint bug)](https://buttondown.com/kaspalytics/archive/issue-2-reading-a-99-drop-a-spec-authors-own-mint/); [Cardano vulnerability reference](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/overview/) | KCC20 shipped a byte-saving shortcut that let anyone mint without a signature. Garbage state in a ticket could brick later claims. | must |
| B7 | Adopt | Each entry-point mode checks its **full** precondition set, with no short-circuit gaps. Extra call arguments not committed to by the transaction need "a commitment story". | [DECL.md#L198](https://github.com/kaspanet/silverscript/blob/v1.0.0/docs/DECL.md#L198); [Cardano overview](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/overview/) | Applies to Merkle proofs and selector arguments in claim and draw. | must |

### C. Cross-family reads

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| C1 | Adopt | Kaspa has no read-only "reference inputs", so **reading a UTXO means spending it**. Snapshot what later steps need into the leader's state. | [Cardano resource exhaustion](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/resource-exhaustion/); [KCC20 thread](https://kas-smiths.org/t/fungible-token-covenant-specification-kcc20/8) | Verify that close() copies the K shard roots into the controller or marker, so draw() and claims prove against one UTXO instead of re-gathering shards. | should |
| C2 | Adopt | Cross-family ownership proofs work by "spends a UTXO of that covenant ID" (`OpCovInputCount(id) > 0`); there is no eval or call. | [KCC20 overview](https://github.com/kaspanet/silverscript/blob/master/docs/kcc20-book/src/kcc20-overview.md) | This is the pattern to use if roles get separate covenant IDs (A6). | should |

### D. Double satisfaction in claims

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| D1 | Adopt | Either one ticket per claim transaction, or the pool (as leader) sums all obligations per payee. | [Cardano double satisfaction](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/double-satisfaction/); [Aiken patterns](https://aiken-lang.org/fundamentals/common-design-patterns) | Otherwise, spending one pool plus two winning tickets can be "paid" by a single payout output. | must |
| D2 | Change | **Plain payout outputs have no covenant binding**, so the authorized-output iterators cannot pair them with an input. Pair them by **fixed position** in the enumerated shape, or tag them. | [KIP-20 binding struct](https://github.com/kaspanet/kips/blob/master/kip-0020.md); [Cardano double satisfaction](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/double-satisfaction/) | KIP-20 gives each *covenant* output exactly one `authorizing_input`, which answers the open question in the cross-ecosystem notes. But winner payouts, the 10% referral in buy() and the 5% fee payout in draw() are ordinary outputs. Each must be pinned to a declared output index with an exact value. | must |
| D3 | Adopt (already) | A claim spends the ticket UTXO, so it cannot be replayed. Griefing is then bounded by real ticket count. | [Cardano resource exhaustion](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/resource-exhaustion/); [ErgoRaffle](https://github.com/ErgoRaffle/raffle-documentation) | Keep this. Claims proving K paths must also check that each path belongs to the claimed shard index and round. | must |

### E. Vault share accounting

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| E1 | Adopt | Use internal accounting. `total_assets` and `total_shares` live in vault state, and UTXO value = `total_assets` + reserve R at every transition. "Donated tokens are not accounted for." | [OpenZeppelin](https://www.openzeppelin.com/news/a-novel-defense-against-erc4626-inflation-attacks) | Covenant IDs plus spend-to-modify already block outside "donations". This makes it explicit. | must |
| E2 | Adopt | Seed the vault at genesis with dead (unredeemable) shares, or use a virtual offset. | [OpenZeppelin](https://www.openzeppelin.com/news/a-novel-defense-against-erc4626-inflation-attacks) | This matters more for us because a jackpot hit can crash the share price. No UTXO-specific sizing guidance exists, so choose the offset and record it. | should |
| E3 | Adopt | Round shares down on deposit and assets down on withdrawal. Reject zero-share mints and payouts below the dust floor. Range-check before every multiply or divide. In SilverScript, overflow and divide-by-zero are **undefined behaviour that the compiler may delete**, not guaranteed failures. | [UNDEFINED-BEHAVIOUR.md](https://github.com/kaspanet/silverscript/blob/v1.0.0/UNDEFINED-BEHAVIOUR.md#L1-L88) | `int` is 64-bit signed. Share × price products need explicit upper bounds. | must |
| E4 | Change | A deposit or withdrawal request must commit to its target round. It is processed exactly once, is owner-refundable after a timeout, and **the number processed per draw is capped**. The alternative is lazy settlement: record the share price per draw and let each owner convert their own request. | [Cardano resource exhaustion](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/resource-exhaustion/); [IOG concurrency](https://www.iog.io/news/concurrency-and-all-that-cardano-smart-contracts-and-the-eutxo-model) | If draw() must consume every pending request, an attacker can stall it with many small requests. Lazy settlement mirrors our claim pattern. Also require that a request is locked in **before the seed for that draw is fixed** (our inference), so nobody deposits after seeing the outcome. | must |
| E5 | Adopt | Value that raises share price without minting shares (protocol inflows, unclaimed-prize sweeps) enters only at draw boundaries, after pending requests are priced. | [OpenZeppelin](https://www.openzeppelin.com/news/a-novel-defense-against-erc4626-inflation-attacks) (our application) | This prevents timing games around sweeps. | should |

### F. Contention

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| F1 | Adopt (already) | Partition mutable state across many UTXOs; "assume a single global mutable UTXO is a bottleneck". | [Decision Guide](https://docs.kaspa.org/toccata/decision-guide); [docs silverscript](https://docs.kaspa.org/toccata/silverscript); [IOG](https://www.iog.io/news/concurrency-and-all-that-cardano-smart-contracts-and-the-eutxo-model) | K shards is exactly this. Wallets should pick a shard at random, and a buyer who loses a race retries on another shard. | should |
| F2 | Verify, then change if needed | close() needs all K shards unspent at once. The Decision Guide flags "transaction construction needing to discover and coordinate many state fragments" as an alarm. | [Decision Guide](https://docs.kaspa.org/toccata/decision-guide) | Make sure post-deadline buys cannot starve close() indefinitely. One option (our inference) uses `OpTxInputDaaScore`, the creation DAA score of the spent UTXO ([KIP-17](https://github.com/kaspanet/kips/blob/master/kip-0017.md)): a shard *created* after the deadline accepts only close(). That allows at most one late buy per shard. Check whether the frozen marker already guarantees this. | must |
| F3 | Verify | It is unknown whether the mempool lets builders chain unconfirmed covenant spends (Ergo's throughput trick). | [ErgoRaffle](https://github.com/ErgoRaffle/raffle-documentation) | This affects buy throughput per shard. Measure it on TN10. | nice |

### G. Reserves, storage mass and dust

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| G1 | **Verify** | KIP-9 sets a **100,000-gram standard transaction limit**. Block limits are 500,000 for storage and 1,000,000 for transient mass. | [KIP-9](https://github.com/kaspanet/kips/blob/master/kip-0009.md); [params.rs](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/consensus/core/src/config/params.rs) | M0's reserve, ticket and max-K figures match the **block** limits exactly (see the next section). Confirm which per-transaction limit v2.1.0's mempool enforces for each mass type. | must |
| G2 | **Verify** | A covenant-bound P2SH UTXO has plurality 2: (63 + 35 + 32)/100, rounded up. A new covenant output therefore costs about **4×10^12 ÷ value** grams, versus 10^12 ÷ value for a plain output. | [mass/mod.rs](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/consensus/core/src/mass/mod.rs); [KIP-20](https://github.com/kaspanet/kips/blob/master/kip-0020.md) | At a 1 KAS reserve that is 40,000 grams per new covenant output; at 0.4 KAS, 100,000 grams. Close() spends roughly as many covenant UTXOs as it creates, so input credit may cancel much of this. Measure with `calc_storage_mass` on real shapes rather than relying on this arithmetic. | must |
| G3 | **Verify** | Dust floor. Aspectron docs: "not less than 0.019 KAS", "0.02 KAS… safe". Derived from KIP-9 against a 100,000-gram limit: about 0.1 KAS (plain) or 0.4 KAS (covenant). | [Aspectron storage mass](https://kaspa.aspectron.org/transactions/fees/storage-mass.html); [KIP-9](https://github.com/kaspanet/kips/blob/master/kip-0009.md) | The 0.019 figure equals about 10^12 ÷ 1.9 M sompi ≈ 526,000 grams, i.e. the *block* limit. The sources measure against different limits. Our 0.08 KAS minimum sits between them. | must |
| G4 | Change | Do not emit outputs below the floor. Accrue small amounts in state, or top up an existing UTXO (KCC20's "Borrowed Receive" idea). | [KCC20 thread](https://kas-smiths.org/t/fungible-token-covenant-specification-kcc20/8); [Aspectron](https://kaspa.aspectron.org/transactions/fees/storage-mass.html) | The **10% referral output in buy() is a standalone plain output**, which conflicts with our own "no standalone tiny outputs" rule on small purchases. Either set a minimum purchase so the referral clears the floor, or accrue referrals in state for later withdrawal. | should |
| G5 | Adopt (already) | Use a uniform reserve. Compute balances excluding R, so rounding never eats into it. The reserve never pays fees. | [BIP-345](https://github.com/bitcoin/bips/blob/master/bip-0345.mediawiki) | Matches the lend-and-refund reserve design. | should |
| G6 | Verify | Rule of thumb: "outputs should not exceed 10 times the number of inputs" unless outputs are large. | [Aspectron](https://kaspa.aspectron.org/transactions/fees/storage-mass.html) | This is a secondary-source heuristic. Confirm it against the code formula for buy(), close() and draw(). | should |

### H. Fees and compute budget

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| H1 | Adopt | Transaction builders size each v1 input's `compute_budget` with `ComputeBudget::checked_covering_script_units`. The budget is **not signed**, so a relayer can adjust it. Report budget failures as resource errors. | [docs script-pricing](https://docs.kaspa.org/toccata/script-pricing) | Our off-chain builder must estimate script units per shape. Over-budgeting raises mass and fees. | must |
| H2 | Adopt | Toccata raised the minimum fee rate from 1 to 100 sompi/gram, so derive fees from the node's estimator. Do not copy the hard-coded `minerFee = 1000` from the CashScript-style examples. | [toccata-guide](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/docs/toccata-guide.md); [covenant_escrow.sil](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/tests/examples/covenant_escrow.sil#L1-L20) | The median fee jumped from 2,036 to 232,500 sompi overnight at activation ([Kaspalytics #1](https://buttondown.com/kaspalytics/archive/issue-1-toccata-live-covenant-usage-a-block/)). | must |
| H3 | Change (consider) | BLAKE3 costs 1 SU per byte and BLAKE2b costs 2. A depth-23 path over 64-byte nodes costs about 2,944 SU with BLAKE2b and about 1,472 with BLAKE3 (derived arithmetic, not measured). | [KIP-25 PR #42](https://github.com/kaspanet/kips/pull/42); [docs script-pricing](https://docs.kaspa.org/toccata/script-pricing) | Claims prove K paths, so the saving scales with K. Switching requires the off-chain tree to change too. SilverScript itself moved its internal hashes to BLAKE3 (#163). | should |
| H4 | Adopt | Use domain-separated Merkle hashing: distinct tags for leaf, internal and empty nodes. | [vprogs node_tags.rs](https://github.com/kaspanet/vprogs/blob/release-candidate/core/merkle-tree/src/node_tags.rs); [hashed_node.rs](https://github.com/kaspanet/vprogs/blob/release-candidate/core/smt/src/hashed_node.rs) | Prevents leaf/branch second-preimage forgery. Use keyed BLAKE2b/BLAKE3 or a tag byte. | must |
| H5 | Adopt | Pass SMT siblings as **one flat byte string of 32 × depth** and check its length before slicing. The stack is capped at 244 items, and an unchecked cast "does not prove" the length. | [txscript lib.rs](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/crypto/txscript/src/lib.rs); [TUTORIAL.md#L904-L930](https://github.com/kaspanet/silverscript/blob/v1.0.0/docs/TUTORIAL.md#L904-L930) | K paths × 23 levels would overflow a one-item-per-hash stack. Struct arrays need per-leaf cardinality checks. | must |
| H6 | Adopt | P2SH inputs are standard only up to 15 signature operations. | [check_transaction_standard.rs](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/mining/src/mempool/check_transaction_standard.rs) | Fine for our shapes. Keep signature checks off the shard and close paths. | nice |

### I. Time and deadlines

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| I1 | Adopt (already) | Use absolute DAA deadlines via `require(tx.daa >= T)`, with 0 ≤ T < 500,000,000,000. `this.ageDaa` is relative. | [TUTORIAL.md#L517-L567](https://github.com/kaspanet/silverscript/blob/v1.0.0/docs/TUTORIAL.md#L517-L567) | Matches. | must |
| I2 | Change | Lock time proves only **"now is after T"**. "Before T" cannot be enforced by a lock. | [Cardano time handling](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/time-handling/) | The buy cut-off must be structural (close spends the shards) plus the F2 rule. Accept that buys can land between T and close(). | must |
| I3 | Adopt | Every state needs a permissionless exit: a claim deadline followed by an anyone-callable sweep to the vault, request refunds, and a refund path if a round cannot close or draw. | [Cardano resource exhaustion](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/resource-exhaustion/); [ErgoRaffle](https://github.com/ErgoRaffle/raffle-documentation) | Keeps the "no keeper bot" goal true even when nobody acts. | must |

### J. Randomness seed window

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| J1 | Adopt | OpChainblockSeqCommit succeeds only if the block is on the selected chain and within finality depth (about 12 h). It is **re-evaluated after a reorg** and fails with BlockAlreadyPruned, BlockNotSelected or BlockIsTooDeep. | [accessor spec](https://github.com/kaspanet/kips/blob/master/kip-0021/seqcommit-accessor.md); [opcodes/mod.rs](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/crypto/txscript/src/opcodes/mod.rs) | draw() must be mined within the window. Spec a fallback for a missed window, such as re-anchoring to a newer block or refunding, even while randomness is parked. | must |
| J2 | Verify | The value returned is the KIP-21 partitioned commitment (lanes SMT + context + miner payloads), not a plain transaction root. The proving spec is "Reserved". | [KIP-21](https://github.com/kaspanet/kips/blob/master/kip-0021.md) | No official randomness guidance exists. | should |
| J3 | Adopt | "Block randomness is grindable." Never ship a block-hash-only seed. | [Cardano randomness](https://developers.cardano.org/docs/developers/curriculum/dapps/oracles/randomness/); [ErgoRaffle](https://github.com/ErgoRaffle/raffle-documentation) | Make the test-only seed path impossible to compile into a mainnet build. | must |

### K. Admin key

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| K1 | Adopt (already) and extend | Fee and referral caps are hard constants in the template. Changes take effect from the next round, which works as a one-round timelock. | [BIP-345](https://github.com/bitcoin/bips/blob/master/bip-0345.mediawiki) | Matches. There is **no official or community admin-key standard** ([KCC-02 thread](https://kas-smiths.org/t/kcc-02-control-principal-references-in-program-abis/95)). | must |
| K2 | Adopt | Sign admin messages over (domain tag, covenant ID, target round, new params, nonce), and store a monotonic nonce. | [Cardano overview: signature domain separation](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/overview/) | Prevents replay onto another round or onto a look-alike family. | must |
| K3 | Adopt | The admin key has **no spend path** on value-holding UTXOs (pools, vault, tickets). A lost key only freezes fee changes. | [Cardano overview](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/overview/) | The protocol must never need the admin to act. | must |
| K4 | Adopt | Store `blake2b(pubkey)` and check `checkSig` at spend time. Declare the key as a KCC-02 `p2pkh-schnorr/v1` principal. | [TUTORIAL.md#L1524-L1528](https://github.com/kaspanet/silverscript/blob/v1.0.0/docs/TUTORIAL.md#L1524-L1528); [KCC-02 PR #5](https://github.com/kaspanet/kccs/pull/5/changes) | Wallet-discoverable, and saves script bytes. | should |
| K5 | Adopt | Upgrades happen by deploying a new covenant family; backers migrate through their own withdrawals. | [KRON, KCC20 thread](https://kas-smiths.org/t/fungible-token-covenant-specification-kcc20/8); [Cardano double satisfaction](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/double-satisfaction/) | No in-place upgrade. | nice |

### L. Interoperability standards

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| L1 | Adopt | KCC-01 dispatch tags are automatic in SilverScript. Renaming an entry point "changes the protocol-visible dispatch tag". | [DECL.md#L271-L281](https://github.com/kaspanet/silverscript/blob/v1.0.0/docs/DECL.md#L271-L281); [kccs PR #3](https://github.com/kaspanet/kccs/pull/3) | Freeze entry-point names and signatures before deployment. The tag scheme was revised in #223 and #164. | should |
| L2 | Adopt | Declare control principals (admin, fee recipient) per KCC-02. "Never present recognition as authentication." | [KCC-02 thread](https://kas-smiths.org/t/kcc-02-control-principal-references-in-program-abis/95) | Pin the PR revision you implement. | should |
| L3 | Adopt (optional) | Bind metadata (name, image) to the covenant ID with KCC-23's prep transaction, or with KCC-0021 genesis-payload JSON. | [kccs PR #26](https://github.com/kaspanet/kccs/pull/26); [kccs PR #6](https://github.com/kaspanet/kccs/pull/6) | Both are open drafts with unresolved issues. Avoid extra prep-tx outputs that could spawn look-alikes. | nice |
| L4 | Adopt (conditional) | Use the KCC20 state header (`ownerIdentifier`, `identifierType`, `amount`) if backer shares become tradable. | [KCC20 thread](https://kas-smiths.org/t/fungible-token-covenant-specification-kcc20/8) | Not needed while shares stay inside the vault. | nice |
| L5 | Adopt | Wallets sign only their own inputs (PSKT, `kaspa:signPskt`), and covenant inputs are script-authorized. Plan to ship your own wallet flow. | [kips PR #44](https://github.com/kaspanet/kips/pull/44); [kccs PR #24](https://github.com/kaspanet/kccs/pull/24); [Kaspalytics #2](https://buttondown.com/kaspalytics/archive/issue-2-reading-a-99-drop-a-spec-authors-own-mint/) | A testnet token team "must ship its own wallet". Expect the same. | should |

### M. Indexer and data availability

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| M1 | Adopt | Run our own indexer. It tracks the covenant IDs, every live UTXO, its creating transaction, redeem-script preimages, genesis data and template hashes. **No node RPC lists UTXOs by covenant ID.** | [docs covenant-state](https://docs.kaspa.org/toccata/covenant-state); [Kaspalytics #1](https://buttondown.com/kaspalytics/archive/issue-1-toccata-live-covenant-usage-a-block/) | Users cannot find their tickets or claim without it. | must |
| M2 | Adopt | Publish a "launch proof / audit bundle": genesis outpoint, authorized genesis outputs, derived covenant ID, template preimages, live UTXOs. Archive preimages, because the docs give no pruning guidance. | [docs covenant-state](https://docs.kaspa.org/toccata/covenant-state); [KCC-23 thread](https://kas-smiths.org/t/kcc-23-convention-for-binding-covenant-metadata-to-a-covenant-id/150) | Lets anyone independently verify that the lottery is the lottery. | must |
| M3 | Adopt | Handle the v2 RPC changes: `covenant` and `covenant_id` fields, `computeBudget`, and `mass` → `storage_mass`. | [toccata-guide](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/docs/toccata-guide.md) | Indexer and builder schemas. | must |
| M4 | Adopt | Register with kascov, or use `kascov-decode`, for public decoding. | [kascov.io](https://kascov.io/); [docs.rs kascov-decode](https://docs.rs/kascov-decode) | Free third-party visibility. | nice |

### N. Tooling versions and pragma

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| N1 | Adopt | Write **`pragma silverscript ^0.1.0;`**. The compiler's `COMPILER_VERSION` is "0.1.0" even in crate 1.0.0, and `^1.0.0` fails to compile. | [compiler mod.rs#L55](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/src/compiler/mod.rs#L55); [static_check.rs#L382-L411](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/src/compiler/static_check.rs#L382-L411) | Likely an oversight that may change in a later release. Re-check on every bump. | must |
| N2 | **Verify** | SilverScript 1.0.0 pins rusty-kaspa to git rev `a41a333…`, not a release ("v2.0.1 predates kaspa-txscript-zk-sdk"). | [Cargo.toml#L21-L31](https://github.com/kaspanet/silverscript/blob/v1.0.0/Cargo.toml#L21-L31) | Confirm that rev is an ancestor of v2.1.0, or that the txscript and covenant code is identical. Otherwise SilverScript's own tests and our engine can disagree. | must |
| N3 | Adopt | Security fixes ship "for the latest revision of the master branch only". | [SECURITY.md](https://github.com/kaspanet/silverscript/blob/v1.0.0/SECURITY.md) | Pin commit 3ed9733, watch master, and regenerate compiled artifacts on any bump. Breaking changes so far: #143 (template hash), #163 (BLAKE3), #223 (dispatch tags), #236 (variable-width state) ([git history](https://github.com/kaspanet/silverscript/commits/v1.0.0)). | must |
| N4 | Adopt | Follow the undefined-behaviour rules. Validate loop bounds explicitly ("may be removed in a later version"). The loop cap is 10,000 iterations per function. Check lengths before casts and slices. | [TUTORIAL.md#L569-L614](https://github.com/kaspanet/silverscript/blob/v1.0.0/docs/TUTORIAL.md#L569-L614); [static_check.rs#L6-L52](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/src/compiler/static_check.rs#L6-L52) | K × depth = 64 × 23 = 1,472 iterations fits under the cap. | must |
| N5 | Verify | Release status: tags `v1-rc1` and `v1.0.0` exist, but community sources say "no releases" and SECURITY.md still says pre-stable. | [README](https://github.com/kaspanet/silverscript/blob/v1.0.0/README.md); [kaspanet/silverscript](https://github.com/kaspanet/silverscript) | Treat 1.0.0 as a tag of experimental software, not an audited release. | should |
| N6 | Adopt | Rust toolchain 1.94.0, matching `rust-version`. | [Cargo.toml#L13-L18](https://github.com/kaspanet/silverscript/blob/v1.0.0/Cargo.toml#L13-L18) | Reproducible builds. | nice |

### O. Testing

| ID | Action | Practice | Source | What it means for us | Priority |
|---|---|---|---|---|---|
| O1 | Adopt | Write the spec first: per transaction shape, list inputs, outputs, values, state changes and signers. | [Essential Cardano/Tweag](https://www.essentialcardano.io/article/five-years-of-cardano-smart-contract-audits); [docs agent-brief](https://docs.kaspa.org/toccata/agent-brief) | The enumerated shapes are a head start. Make them the single source of truth that tests read. | must |
| O2 | Change | SilverScript's test harness runs with `sigop_script_units: 0`, which **does not meter compute**. | [tests/common.rs#L96-L140](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/tests/common.rs#L96-L140) | Add tests with production engine flags and real v1 compute budgets. | must |
| O3 | Adopt | Test maximum paths against every limit: "test the maximum paths" for hard limits. | [Plutus common weaknesses](https://plutus.cardano.intersectmbo.org/docs/delve-deeper/common-weaknesses) | Run close() and draw() at maximum K with worst-case proofs, against both block and standard limits (G1). | must |
| O4 | Adopt | End-to-end runs on Testnet-10, where Toccata is active and the `signPskt` flow was validated. | [params.rs v2.0.1](https://github.com/kaspanet/rusty-kaspa/blob/v2.0.1/consensus/core/src/config/params.rs); [kips PR #44](https://github.com/kaspanet/kips/pull/44) | TN12's current status is unconfirmed ([testnet12.md](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/docs/testnet12.md)). | must |
| O5 | Adopt | Use the `sil-debug` source debugger and kascov preflight / spend-trace replay. | [README](https://github.com/kaspanet/silverscript/blob/v1.0.0/README.md); [kascov.io](https://kascov.io/) | Practical review tools. | should |
| O6 | Adopt | Get an independent audit. There is no ecosystem audit programme, and SilverScript itself is unaudited. | [Audit jury thread](https://kas-smiths.org/t/would-a-project-audit-jury-be-a-good-implementation-for-kas-smiths-at-this-time/83); [docs silverscript](https://docs.kaspa.org/toccata/silverscript) | Budget for it before mainnet value. | should |

## Where sources disagree, and where M0 may be measuring the wrong limit

### M0 looks sized to block limits, not transaction limits

The most consequential finding is arithmetic, not a citation. KIP-9's charge for a new covenant-bound UTXO is about 4×10^12 ÷ value (in sompi) grams ([mass/mod.rs](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/consensus/core/src/mass/mod.rs)). At M0's **0.08 KAS minimum ticket deposit** (8 million sompi), that is exactly **500,000 grams**, which is the entire per-block storage-mass limit ([params.rs](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/consensus/core/src/config/params.rs)).

M0's reserves follow the same pattern. They equal **0.08 KAS × (K + 2)**:

| K | 0.08 × (K + 2) | M0 reserve R |
|---|---|---|
| 16 | 0.08 × 18 = 1.44 KAS | 1.44 KAS |
| 32 | 0.08 × 34 = 2.72 KAS | 2.72 KAS |
| 64 | 0.08 × 66 = 5.28 KAS | 5.28 KAS |

That is what you get if a close() creating K + 2 covenant outputs, with no credit for its inputs, is allowed to fill a block's storage budget.

The max-K figures follow the same block logic, this time for transient mass:

| Shard script size | Max K | K × script size | × 4 (transient mass) |
|---|---|---|---|
| 3,418 B | 62 | 212 KB | about 850,000 grams |
| 6,000 B | 37 | 222 KB | about 890,000 grams |
| 10,000 B | 23 | 230 KB | about 920,000 grams |

All three sit just under the **1,000,000-gram block transient limit** ([toccata-guide](https://github.com/kaspanet/rusty-kaspa/blob/v2.1.0/docs/toccata-guide.md)).

KIP-9, however, states a **standard (relay) transaction mass limit of 100,000 grams** ([KIP-9](https://github.com/kaspanet/kips/blob/master/kip-0009.md)). The research did not confirm whether v2.1.0's mempool applies that cap to each mass type. If it does, three things follow:

- A 0.08 KAS covenant ticket is non-standard. The floor rises to about 0.4 KAS for a covenant ticket, or about 0.1 KAS for a plain P2SH ticket.
- Reserves sized the M0 way would need to be about 5× larger: 7.2 / 13.6 / 26.4 KAS.
- A 212–230 KB close() would be roughly ten times over the standard size, so the public network would not relay it.

A non-standard transaction is still consensus-valid if a miner includes it. But needing direct miner submission is a keeper in disguise, which breaks the no-operator goal.

Two things could rescue the current numbers. KIP-9's input credit may offset most of close()'s charge, because close() spends roughly as many reserve-carrying covenant UTXOs as it creates. And the standard limit may no longer apply as stated. Both must be measured with `calc_storage_mass` and real mempool submission on TN10, not assumed. All of the arithmetic above is ours, derived from the cited formula and constants. It is not an official figure.

### Other source conflicts to record as Verify items

| Conflict | Side A | Side B | How to resolve |
|---|---|---|---|
| Dust floor | 0.019 KAS minimum, 0.02 KAS "safe" ([Aspectron](https://kaspa.aspectron.org/transactions/fees/storage-mass.html)) | About 0.1 KAS (plain) or 0.4 KAS (covenant) derived against a 100,000-gram limit ([KIP-9](https://github.com/kaspanet/kips/blob/master/kip-0009.md)); M0 measured 0.08 KAS | Same root cause as G1. Measure on v2.1.0. |
| SilverScript releases | "No releases"; "bytecode artifacts only on testnet-10 until first stable v1" ([kaspanet/silverscript](https://github.com/kaspanet/silverscript); [Kaspalytics #2](https://buttondown.com/kaspalytics/archive/issue-2-reading-a-99-drop-a-spec-authors-own-mint/), August) | Tags `v1-rc1` (2026-08-30) and `v1.0.0` (2026-09-09), with SECURITY.md still "pre-stable" ([SECURITY.md](https://github.com/kaspanet/silverscript/blob/v1.0.0/SECURITY.md)) | Check the GitHub Releases page by hand (it was blocked for the researchers). Treat 1.0.0 as unaudited either way. |
| rusty-kaspa revision | SilverScript pins `a41a333` ([Cargo.toml](https://github.com/kaspanet/silverscript/blob/v1.0.0/Cargo.toml#L21-L31)); vprog-tictactoe pins `eb0a856d` ([Cargo.toml](https://github.com/biryukovmaxim/vprog-tictactoe/blob/master/Cargo.toml#L15-L58)) | We pin release v2.1.0 ([release](https://github.com/kaspanet/rusty-kaspa/releases/tag/v2.1.0)) | Run `git merge-base --is-ancestor a41a333 v2.1.0` and diff `crypto/txscript`. |
| Script-size limit | 1,000,000 bytes for script, element and ops ([KIP-17](https://github.com/kaspanet/kips/blob/master/kip-0017.md)) | "~300 KB shared between code and state" for redeem scripts ([docs covenants](https://docs.kaspa.org/programmability/covenants)) | Irrelevant for 3–10 KB shards. Confirm before growing scripts. |
| KIP-24/25 status | Linked as master files by docs ([docs references](https://docs.kaspa.org/toccata/references)) | Unmerged PRs #41 and #42 ([kips PR #41](https://github.com/kaspanet/kips/pull/41); [kips PR #42](https://github.com/kaspanet/kips/pull/42)) | Treat code (`mass/units.rs`) as truth for SU prices. |
| `OpOutputAuthorizingInput` | Listed as a consensus opcode ([docs covenant-state](https://docs.kaspa.org/toccata/covenant-state)) | Not exposed by SilverScript ([builtin_types.rs](https://github.com/kaspanet/silverscript/blob/v1.0.0/silverscript-lang/src/compiler/builtin_types.rs#L66-L180)) | Use B2's substitute. Ask whether exposure is planned. |
| Whole-transaction checks | "Ignore the rest"; whole-tx ownership is a pitfall ([docs covenant-state](https://docs.kaspa.org/toccata/covenant-state)) | Ban other scripts or extras ([Cardano double satisfaction](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/double-satisfaction/); [Cardano resource exhaustion](https://developers.cardano.org/docs/developers/curriculum/smart-contracts/security/vulnerabilities/resource-exhaustion/)) | A design decision (B3), not a fact to look up. |
| Wallet API home | KIP-12 draft in kips PR #44, updated 2026-08-10 ([kips PR #44](https://github.com/kaspanet/kips/pull/44)) | Moved to KCC process 2026-07-15; KCC-12 PR #24 ([forum](https://kas-smiths.org/t/wallet-integration-protocol/147)) | Track both; pin the revision. |

## Community actions

The project has a real chance to set conventions rather than follow them, because no covenant lottery, vault standard or admin-key convention exists yet. Six steps are worth taking.

1. **Post the design on Kas-Smiths** under Product Use Cases. Include the transaction shapes, shard/close/claim flow, vault maths and admin caps. The forum is the normal route to review now that the audit-jury idea did not take off ([Audit jury thread](https://kas-smiths.org/t/would-a-project-audit-jury-be-a-good-implementation-for-kas-smiths-at-this-time/83)). Weight replies from known core contributors (Sutton, Newman, IzioDev, Manyfest) above anonymous or auto-generated ones ([Wallet integration thread](https://kas-smiths.org/t/wallet-integration-protocol/147)).
2. **Answer the open "High-BPS covenant state pressure" thread** (posted 2026-10-09, no replies). It asks exactly what M0 and TN10 runs measure: single-UTXO contention, storage mass under frequent updates, settlement frequency ([thread](https://kas-smiths.org/t/high-bps-covenant-state-pressure-utxo-contention-storage-mass-and-settlement-frequency-seeking-corrections/160)). Publishing shard-contention and mass numbers there builds credibility. It also invites a core answer on the per-transaction limit question (G1).
3. **Pin KCC drafts by commit hash** in DECISIONS.md. That covers KCC-01 PR #3, KCC-02 PR #5, KCC-23 PR #26 and KCC-12 PR #24, plus the KCC20 thread revision if used. Re-check them each milestone ([kaspanet/kccs](https://github.com/kaspanet/kccs)).
4. **Comment on KCC-02** with the "admin may change fees only within hard caps, from the next round" use case. That needs a principal plus a parameter scope, which the draft does not cover ([KCC-02 thread](https://kas-smiths.org/t/kcc-02-control-principal-references-in-program-abis/95)).
5. **Support the `getUtxosByCovenantId` RPC proposal**, since our indexer burden depends on it ([thread](https://kas-smiths.org/t/has-there-been-any-discussion-of-adding-getutxosbycovenantid/74)).
6. **Report any SilverScript bugs privately** through GitHub security advisories, never in public issues ([SECURITY.md](https://github.com/kaspanet/silverscript/blob/v1.0.0/SECURITY.md)). Register the launch bundle on kascov at deployment ([kascov.io](https://kascov.io/)).

The shift in understanding is this: the hard problems are no longer "can Kaspa do this?" but **which limit governs, and who is responsible for each check**. Consensus guarantees only who may enter a covenant family. Everything else (counts, values, pairing of plain payouts, deadlines, liveness) is the script's job. And the margin between block-level feasibility and network-level relay may decide whether the protocol is truly operatorless.

## Hand-off for Claude Code

1. **DECISIONS.md — mass limits (G1–G3).** Add an open decision: "Which per-transaction mass limit (storage, transient, compute) does the rusty-kaspa v2.1.0 mempool enforce?" Add a task to measure buy(), close() at K = 16/32/64 and draw() with `calc_storage_mass`, and to submit them via a TN10 node's mempool. Record whether M0's figures (0.08 KAS; R = 0.08 × (K + 2); max K 62/37/23) hold under standardness. If not, recompute R, the minimum ticket deposit and max K.
2. **DECISIONS.md — family structure (A6, C2).** Decide whether rounds are new genesis or continuations of one covenant ID. Decide whether roles share one covenant ID (chess style) or use separate IDs (KCC20/Minter style). Note that genesis commits to all initial output values and scripts, and that genesis outputs are invisible to the OpAuth/OpCov iterators.
3. **DECISIONS.md — transaction-shape policy (B3, H2).** Choose between "exact covenant counts + at most one plain fee input and one change output with no covenant ID" and "strict no extra outputs". Record who pays fees in each shape, and confirm that reserve R never pays fees.
4. **DECISIONS.md — pins (N1–N3, N5).** Record pragma `^0.1.0`, SilverScript commit 3ed9733 (tag v1.0.0), Rust 1.94.0 and rusty-kaspa v2.1.0. Add a task to check whether `a41a333` is an ancestor of v2.1.0 and to diff `crypto/txscript`. Add a standing task to watch SilverScript master for security fixes and regenerate artifacts on any bump.
5. **Covenant-reviewer checklist — counts.** Every entry point asserts exact `OpCovInputCount` per family touched, exact `OpAuthOutputCount`, and `OpCovOutputCount == OpAuthOutputCount` (B1).
6. **Covenant-reviewer checklist — output binding.** Continuation outputs are selected via `OpAuthOutputIdx(this.activeInputIndex, k)` or by asserting `OpCovOutputIdx(cov, 0) == 0`. `OpOutputCovenantId` is used only for genesis outputs (B2).
7. **Covenant-reviewer checklist — cross-role reads.** Every cross-role read uses `readInputStateWithTemplate`, and every cross-role write uses `validateOutputStateWith[Input]Template`. Template hashes come only from state or constants via `templateHash(prefix, suffix)` (A3, A4).
8. **Covenant-reviewer checklist — leader/delegate.** The controller is the leader at covenant input 0. Shard delegates reject index 0, assert `OpAuthOutputCount == 0`, and authenticate the leader template. Non-delegate shard entries assert `OpCovInputCount == 1` (B5).
9. **Covenant-reviewer checklist — values.** Every covenant output has an exact value. Conservation holds per shape. Remainders are split as `b = total − a` (B4).
10. **Covenant-reviewer checklist — plain outputs.** Winner payouts, referral (buy) and protocol fee (draw) each sit at a fixed declared index with an exact value. A claim spends exactly one ticket, or the pool sums obligations per payee (D1, D2).
11. **Covenant-reviewer checklist — role and round.** Every state carries a role byte and round number, and every consumer checks covenant ID + role + round (A5).
12. **Covenant-reviewer checklist — undefined behaviour.** Range-check all ints before + − × ÷ %. Check byte lengths before casts and slices. Bound loops explicitly. Never rely on undefined behaviour to reject (E3, N4).
13. **Milestone spec — Merkle.** Use domain-tagged leaf/internal/empty hashing. Pass siblings as one flat `32 × depth` byte string with a length check. Benchmark BLAKE3 against BLAKE2b script units for K-path claims before freezing the hash (H3–H5).
14. **Milestone spec — deadlines.** Specify the post-deadline buy bound, for example a shard created after the deadline (via `OpTxInputDaaScore`) accepts only close(), or confirm the frozen marker gives the same guarantee. State plainly that "before T" is structural, not lock-time enforced (F2, I2).
15. **Milestone spec — vault.** Use internal accounting (value = `total_assets` + R). Add dead shares or a virtual offset at genesis. Round down, and reject zero-share mints. Requests commit to a round, are locked before the seed is fixed, are refundable after a timeout, and are either capped per draw or lazily settled (E1–E5).
16. **Milestone spec — liveness.** Add a claim deadline plus an anyone-callable sweep to the vault, a refund path for rounds that cannot close or draw, and confirm that loss of the admin key halts nothing (I3, K3).
17. **Milestone spec — seed window.** draw() must be mined within finality depth (about 12 h) of the referenced chain block. Spec the missed-window fallback, and treat the returned value as the KIP-21 partitioned commitment. Make the test-only seed impossible to build for mainnet (J1–J3).
18. **Milestone spec — admin.** Caps are template constants. The signature covers (domain tag, covenant ID, target round, params, nonce), and the nonce in the controller is monotonic. The admin has no spend path on value UTXOs. Store the key as `blake2b(pubkey)`. Declare it as a KCC-02 `p2pkh-schnorr/v1` principal, pinned to its PR revision (K1–K4, L2).
19. **Milestone spec — referral dust.** Either enforce a minimum purchase so the 10% referral output clears the measured floor, or accrue referrals in state for later withdrawal (G4).
20. **Builder and indexer spec.** Size `compute_budget` per input with `ComputeBudget::checked_covering_script_units`. Take fees from the node estimator at ≥ 100 sompi/gram. Handle the `storage_mass`, `covenant` and `computeBudget` RPC fields. Build an indexer for covenant IDs and preimages. Publish a launch/audit bundle (genesis outpoint, auth outputs, covenant ID, template preimages, live UTXOs) and archive it against pruning (H1, H2, M1–M3).
21. **Test plan — compute metering.** Add tests with production engine flags; the SilverScript harness's `sigop_script_units: 0` does not meter compute (O2).
22. **Test plan — worst cases.** Run max-K close() and draw() with worst-case proofs against both block and standard limits (O3).
23. **Test plan — adversarial cases.** Cover: a parallel auth group with the same ID, an extra covenant output, a missed covenant input, a fake-family UTXO, a wrong-template read, wrong role or round, a double claim with one payout, a zero-share deposit, overflow boundaries, a post-deadline buy, a seqcommit that is too deep or reorged, and an under-funded compute budget (B1–B7, D1–D2, E3, J1).
24. **Test plan — networks and tools.** Run end-to-end on Testnet-10, and use `sil-debug` and kascov preflight for review (O4, O5).
25. **Release gate.** No mainnet value until G1 and N2 are resolved, the reviewer checklist passes on every shape, and an independent audit is scheduled (O6).
