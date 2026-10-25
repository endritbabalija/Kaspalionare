# sil-debug: the M8 S5 debugger evidence, reproducible

The M8 S5 review ran SilverScript's `cli-debugger` on two real testnet-10 spends of the
campaign instance `e9f4f810…88d6`: round 8's `draw()`
`a08b156fb90063e62da8a760b3afbcf0a58bf3c16dee9d2268075dba1a8eed6c` (input 0, `drawer.sil`)
and the claim `43fb4cf3c4405e61ddb6e50c4af5e250f7c91348f5b5aa412f8b71f833f8b37f` (input 0,
`ticket.sil`). Results: `docs/tn10-mass-check.md` and `docs/specs/8-handoff.md` (S5). This
folder keeps the two scratch tools that produced them:

- `cli-debugger.patch`: a patch for `debugger/cli/src/main.rs` of SilverScript **v1.0.0**
  (`3ed973335b59269293564805cc2c58a14595ec03`). It changes only the transaction context:
  1. each spent UTXO gets its real creation DAA score (`SDB_INPUT_DAA=d0,d1,...`; the stock
     CLI uses 0, so `OpTxInputDaaScore` reads 0 and `draw()` stops at check 7);
  2. a KIP-21 accessor answers `OpChainblockSeqCommit` for the anchor X only
     (`SDB_SEQ_COMMIT=<X hash>=<commitment>`; the stock CLI has none, so opcode `0xd4` is
     invalid);
  3. it prints whether the redeem script compiled from the source and constructor
     arguments equals the on-chain one (`SDB_REDEEM=<hex>`);
  4. with `SDB_STACK_PEAK=1` it steps opcode by opcode, counts the stack peak (dstack and
     dstack + astack) with its source line, and prints the final stack (the stock `--run`
     reports PASS without the engine's final clean-stack check).
- `extractor/` (`silx`): reads the campaign's index store and, for each `label=txid[:sdb]`,
  writes `<label>.preflight.json` (the kascov preflight body), and with `:sdb` also
  `<label>.test.json` (a `cli-debugger` test file for input 0: constructor arguments from
  the decoded state, entry arguments from the real signature script, every other input and
  output with its real script and covenant binding) and `<label>.env` (the `SDB_*` values).
  It also writes the committed `ticket.sil` and `drawer.sil` the templates were compiled
  from, to compare with `covenants/`.

Limit: the debugger links rusty-kaspa **`a41a333b`** (SilverScript's own pin,
`refs/silverscript/Cargo.toml`), not our v2.1.0, and `crypto/txscript` differs between the
two. A debugger result is evidence from that engine revision only.

Nothing here reads keys or `.env`; the inputs are public testnet-10 bytes. Never modify
`refs/` (read-only): work on a copy.

## 1. A patched copy of SilverScript v1.0.0, outside `refs/`

Git Bash, from the repository root. Use a short target path: some SilverScript test file
names exceed the Windows path limit.

```bash
git -c core.longpaths=true clone --branch v1.0.0 refs/silverscript /c/sdbw
cd /c/sdbw
git config core.longpaths true
git log -1 --format=%H        # 3ed973335b59269293564805cc2c58a14595ec03
git apply --ignore-whitespace /c/Users/endri/Documents/Kaspalionare/tools/sil-debug/cli-debugger.patch
git diff --stat               # debugger/cli/src/main.rs | 65 +++-
```

(`--ignore-whitespace` because a Windows checkout has CRLF line ends; the patch is LF. On
an LF checkout plain `git apply` works.)

## 2. Build

```bash
cd /c/sdbw
cargo +1.94.0 build -j 1 --release --locked -p cli-debugger
```

About 8.5 minutes in S5. The binary is `target/release/cli-debugger.exe`.

## 3. Extract the two spends

The index store of the instance is on the owner's machine (git-ignored):
`tn10-data/e9f4f810c86467e7768aa1b42e6c62182b8a0d934bda922bdbb2804f223988d6/`
(`index-log.jsonl`, `bundle.json`). Write the output outside the repository.

```bash
cd /c/Users/endri/Documents/Kaspalionare/tools/sil-debug/extractor
cargo +1.94.0 run -j 1 --release --locked -- \
  ../../../tn10-data/e9f4f810c86467e7768aa1b42e6c62182b8a0d934bda922bdbb2804f223988d6 \
  /c/sdbw-out \
  draw=a08b156fb90063e62da8a760b3afbcf0a58bf3c16dee9d2268075dba1a8eed6c:sdb \
  claim=43fb4cf3c4405e61ddb6e50c4af5e250f7c91348f5b5aa412f8b71f833f8b37f:sdb
```

It prints one line per transaction and writes `draw.*`, `claim.*` and
`committed-{ticket,drawer}.sil` into `/c/sdbw-out`. Check that the committed sources equal
the repository's (`diff /c/sdbw-out/committed-drawer.sil ../../../covenants/drawer.sil`).

## 4. Run

From the repository root (so `covenants/` resolves):

```bash
D=/c/sdbw/target/release/cli-debugger
( set -a; . /c/sdbw-out/draw.env; set +a; SDB_STACK_PEAK=1 \
  $D covenants/drawer.sil --test-file /c/sdbw-out/draw.test.json --test-name draw --run )
( set -a; . /c/sdbw-out/claim.env; set +a; SDB_STACK_PEAK=1 \
  $D covenants/ticket.sil --test-file /c/sdbw-out/claim.test.json --test-name claim --run )
```

Without `SDB_STACK_PEAK` the stock `--run` path runs (with the DAA and KIP-21 changes).
Without the `.env` values the run is the stock CLI's: the claim passes, the draw stops at
`drawer.sil` check 7.

What S5 recorded:

| spend | redeem compiled = on chain | result | opcodes | stack peak (dstack + astack; limit 244) | final stack |
|---|---|---|---|---|---|
| claim | yes (25,452 B) | PASS | 22,453 | 88, at `ticket.sil:331` (check 15) | `[01]` |
| draw | yes (64,629 B) | PASS (checks 7-8 included) | 57,170 | 240, at `drawer.sil:994` (check 24) | `[01]` |
