# ADR-0005: Deliver the grammar and the constraint layer to CGO-free Go consumers

- Status: accepted (owner decision, 2026-11-16)
- Date: 2026-11-16
- Amends: ADR-0002 §6 (runtime milestone)

## Context

go-treesitter, the intended consumer, is a CGO-free Go library. It compiles each pinned grammar's C (`parser.c`,
`scanner.c`) with wasi-sdk to WebAssembly and translates the module to Go with wasm2go. Its `tsql` module is pinned to
the original upstream, `alfpricot/tree-sitter-mssql` at `2e34d86e`.

This repository's guarantee (ADR-0004) is the grammar plus the constraint layer in `tools/tree-check`, which is Rust.
On the oracle:
- the grammar alone parses every engine-accepted case cleanly (7,450 of 7,450 at #45);
- of the 2,850 cases every engine rejects, the grammar rejects 852;
- the constraint layer rejects 1,755 more;
- the remaining 243 are classified in `oracle/rejected-clean.json` (no `gap`).

The runtime bound of ADR-0002 §6 is 1.5x of the M2 end (`e8a398e`). After the M3 work the compiled library stands at
about 1.39x. Tree-preserving refactors now save 2–4 % per round, against 15 % in the first rounds.

## Decision

1. **Grammar first.**
   - go-treesitter moves its `tsql` pin to this repository once the pending pull requests are merged. The pin move is
     go-treesitter's change, under its own rules.
   - That gives "every engine-accepted T-SQL parses cleanly" at once.
2. **Then the constraint layer, compiled to WebAssembly.**
   - The Rust rules stay the single implementation.
   - This repository builds them, with the parser, into a wasm module that exports a small C ABI. The toolchain is
     pinned to the versions go-treesitter uses (wasi-sdk 34.0; binaryen where used).
   - A CI gate proves that the wasm module and the native binary give the same result on every oracle case.
   - go-treesitter translates the module with its existing wasm2go pipeline. The Go side is its decision.
   - No Go port: two implementations would drift.
3. **Runtime milestone.**
   - The milestone in `test/runtime-cost.json` moves to the end of M3, the merge of the pull request that completes
     the M3 backlog and its cost rounds (#46).
   - The 1.5x bound then applies to growth from there.
   - The M2 baseline stays recorded in this ADR: at M3 end, against M2, library 1.39x, parse time 0.93x, peak memory
     1.15x.

## Consequences

- Consumers that only parse (highlighting, navigation, analysis) can use the grammar as soon as go-treesitter's pin
  moves.
- Consumers that validate T-SQL need the wasm constraint module. Checking parses the input again inside the module.
  The cost is measured and recorded with the module.
- A rule change becomes visible to Go consumers only through a new module build. The parity gate keeps the module
  equal to the native tool at each commit.
