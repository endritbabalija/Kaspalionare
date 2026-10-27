# anatomy-kit (vendored drawing kit for the docs figures)

The three modules `docs/figures/draw-cabinet/build.mjs` imports, copied unchanged from the `anatomy` skill
(github `wheresryan22/anatomy`, MIT, `LICENSE` here; installed 2026-10-12, `skills-lock.json` hash
`5e8699464cc059fbae60af030b707b314c6be5b9fc71c187711fc3abe48d9f46`), so the figure builds from a clean clone
with Node alone (D138, front end F6 part B):

| File | sha256 |
|---|---|
| `iso-kit.mjs` | `6d3ecc725d30c3b0346da309cf61dfa5a65017c23620bbd4093009d4fb104018` |
| `lathe.mjs` | `a4e7e974b1a37e603ad2b76363b6f9815cc101dfff0ad277a5d4e8f1dc56fccb` |
| `tube.mjs` | `bcb3cc360f6d83e6a5753f4c464b4587ab7965028b05a5d58e285abce58667c9` |

The skill's `scripts/inline-kit.mjs` (`kitScript()`: `iso-kit.mjs` with `export ` removed) is two lines inside
`build.mjs` instead. Nothing else of the skill is needed. Rebuilding the committed figure with these files gave
byte-identical HTML (checked 2026-10-27 before any figure change).

Build: `node docs/figures/draw-cabinet/build.mjs --all` (from the repository root; no npm install). To update the
kit, copy the three files from a newer skill, rebuild, and look at the figure before committing.
