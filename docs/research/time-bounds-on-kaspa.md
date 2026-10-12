# Upper time bounds on Kaspa: can a spend be invalidated by the clock?

Date: 2026-10-12
Scope: rusty-kaspa v2.1.0 (`refs/rusty-kaspa-v2.1.0`), KIPs in `refs/kips`, SilverScript v1.0.0. Labels: VERIFIED (cited) or UNVERIFIED/INFERRED.

## Findings

| # | Mechanism | What it does | Upper bound? | Status |
|---|-----------|--------------|--------------|--------|
| 1 | Consensus `lock_time` | A tx passes if `lock_time < ctx DAA score` (or median time). Otherwise it passes only if every input has `sequence == u64::MAX`. Nothing rejects a tx when the current score is far above `lock_time`. `lock_time` is chosen by the spender. | No | VERIFIED `consensus/src/processes/transaction_validator/tx_validation_in_header_context.rs:56-76` (`check_tx_is_finalized`); threshold 500_000_000_000 splits DAA score from ms time: `consensus/core/src/constants.rs:17` |
| 2 | `OpTxLockTime` (0xb5) | Pushes the tx's own `lock_time` field. | No. It only proves "now > lock_time", because a tx with a larger `lock_time` is not yet valid. A spender can understate the time, never overstate it. | VERIFIED `crypto/txscript/src/opcodes/mod.rs:995-1001`; the "now > lock_time" consequence is INFERRED from row 1 |
| 3 | `OpCheckLockTimeVerify` (0xb0) | Fails if the stack value is greater than the tx `lock_time`, if the two are of different types, or if the input is finalized (sequence MAX). | No (it only enforces `lock_time >= X`) | VERIFIED `opcodes/mod.rs:874-924` |
| 4 | `sequence` and `OpCheckSequenceVerify` (0xb1) (CSV equivalent) | Relative lock. Consensus rejects an input with bit 63 clear when `entry.block_daa_score + (seq & 0xffffffff) - 1 >= pov_daa_score`. CSV requires the stack value to be `<=` the input's masked sequence. Bit 63 set disables the lock. | No, lower bound only: "at least N DAA after the UTXO was created". | VERIFIED `consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs:136-160`; masks `consensus/core/src/constants.rs:42,47`; CSV `opcodes/mod.rs:926-958` |
| 5 | `OpTxInputSeq` (0xbd) | Pushes input `idx`'s sequence as 8 raw LE bytes. | No | VERIFIED `opcodes/mod.rs:1085-1096`; `refs/kips/kip-0017.md:35` |
| 6 | `OpTxInputDaaScore` (0xc0), KIP-17 | Pushes `block_daa_score` of the block that created the spent UTXO (`utxo.block_daa_score`). | Indirect. It bounds "when the UTXO was created", which is a past fact. It gives no bound on when the spend is mined. | VERIFIED `opcodes/mod.rs:1124-1135` |
| 7 | `OpChainblockSeqCommit` (0xd4), KIP-21 | Takes a 32-byte block hash and returns that chain block's header `accepted_id_merkle_root`. The call fails if the block is not known (pruned), is not on the selected-parent chain of the point of view, or is outside the access window. The window rule is `X.blue_score + F > P.blue_score`, where P is the selected parent of the block being built or validated. F is the finality depth, "about 12 hours". | See the discussion below | VERIFIED `opcodes/mod.rs:1389-1402`; `consensus/src/model/services/seq_commit_accessor.rs:8-10,31-44`; `refs/kips/kip-0021/seqcommit-accessor.md` ("Opcode Mechanics", "Access Threshold", "Reorg Behavior") |
| 8 | Mempool expiry | Low-priority txs are dropped when `virtual_daa_score > added_at_daa_score + transaction_expire_interval_daa_score`. The default is 24 h of DAA score (`DEFAULT_TRANSACTION_EXPIRE_INTERVAL_SECONDS = 24*60*60`). High-priority txs are never expired. | Node policy only, not consensus | VERIFIED `mining/src/mempool/model/transactions_pool.rs:343-368`; `mining/src/mempool/config.rs:7,108` |
| 9 | Tx v1 fields (`TX_VERSION_TOCCATA = 1`) | The listed Toccata changes are covenant output bindings, input compute-budget mass, user-lane subnetworks and gas commitments. No expiry field is listed. The `Transaction` struct has `version, inputs, outputs, lock_time, subnetwork_id, gas, payload` and no expiry field. | No | VERIFIED `consensus/core/src/constants.rs:10-15`; `consensus/core/src/tx.rs:254-261` |
| 10 | Newer KIPs and PRs | The kaspanet/kips master listing has no KIP about expiry, max lock time or valid-until (files: kip-0001..0006, 0009, 0010, 0013-0017, 0020, 0021). KIP-24 (tx v1 fields and hashing) is open as kips PR #41. Its text could not be fetched, so I cannot rule out a time field in it. A search of rusty-kaspa PRs for "expir" found nothing relevant (only mempool tweaks and test deadlines). | None found | UNVERIFIED (negative search, web-sourced): <https://github.com/kaspanet/kips>, <https://github.com/kaspanet/kips/pull/41>, <https://docs.kaspa.org/toccata/references>, <https://github.com/kaspanet/rusty-kaspa/pulls?q=expir> |

## Point-by-point notes

**Mempool (question 3).** Expiry is a mempool-local policy applied to low-priority txs. Consensus has no counterpart, since the validator code in `transaction_validator/` contains no expiry rule. An adversary is not bound by it. They can rebroadcast, submit to a node configured with another interval, or mine the tx in their own block. A block containing an old but otherwise valid tx is valid. Mempool expiry therefore gives no covenant guarantee (INFERRED from the two VERIFIED points above).

**OpChainblockSeqCommit as a bound (question 4).**
- Reading the seqcommit of a chain block X proves that X exists on the selected-parent chain of the block that includes your tx. That is a lower bound on the inclusion time: the spend is included after X.
- The only upper-bound-like effect is the access window. Once the tx's inclusion point has moved more than F blue-score units past X, the opcode fails and the tx is invalid in that position. So a script that reads the seqcommit of a chosen block X makes the spend impossible after roughly X + F, and F is "about 12 hours" per the KIP text.
- This is INFERRED from the accessor rules and has not been run in the engine. Several things weaken it:
  - It is measured in blue score, not DAA score.
  - The window is a deployment parameter that we cannot choose. The KIP only says "about 12 hours of target block time".
  - A reorg re-evaluates the opcode from the new point of view (`kip-0021/seqcommit-accessor.md`, "Reorg Behavior"). A tx can therefore become valid or invalid across reorgs, and the cutoff is only deterministic relative to the selected-parent chain that is actually being validated.
  - The script needs a valid block hash X as an input. Whoever builds the tx supplies X, so the script must separately tie X to the round, for example by comparing against a committed value, and X must be a chain block.
  - The relevant cutoff would be fixed at about 12 h from X. It is not a free per-round deadline.
- We could not verify the exact value of F in v2.1.0 params. The check `grep seqcommit` in `consensus/core/src/config/params.rs` found nothing, so its source was not located.

**Sequence and CSV (question 2).** Row 4. Relative lock is lower-bound only. Setting bit 63 turns it off, and CSV insists that the input did not set bit 63.

**CLTV bypass.** CLTV refuses an input whose sequence is MAX, so the "finalized" bypass in row 1 does not defeat CLTV (`opcodes/mod.rs:874-924`). That only protects the lower bound.

## Conclusion (question 6)

- **Frozen-by-consumption is enforceable on Kaspa today.** A covenant can require that a UTXO be spent (so it ceases to exist) and bind the successor's state in the same transaction. All of it rests on VERIFIED opcodes (`OpTxInputDaaScore`, covenant opcodes). The existing structural rule "the spent UTXO must have been created before T, so at most one spend per UTXO after T" is within this category.
- **Frozen-by-clock is not enforceable in the general case.** No consensus field, opcode or script construct states "this spend is invalid after DAA score or time X". `lock_time`, `OpTxLockTime`, CLTV, `sequence` and CSV are all lower bounds. Mempool expiry is node policy and can be bypassed by anyone who can get a block mined.
- **Narrow exception (UNVERIFIED in practice).** `OpChainblockSeqCommit` makes a spend invalid after the access window F (about 12 h) has passed beyond a chosen chain block. That is a coarse, non-configurable cutoff with reorg and blue-score caveats. It would need engine tests before anything depends on it, and no result here shows it works as a "close ticket sales" gate.
- **Open item.** KIP-24 (open, unread) could add tx fields. Read `kip-0024.md` from PR #41 before treating this question as closed for post-v2.1.0 releases. No other post-v2.1.0 proposal for expiry was found.
