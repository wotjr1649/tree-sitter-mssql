# ADR-0005: Deliver the grammar and the constraint layer to CGO-free Go consumers

- Status: accepted (owner decision, 2026-11-16)
- Date: 2026-11-16
- Amends: ADR-0002 §6 (runtime milestone)

## Context

go-treesitter, the intended consumer, is a CGO-free Go library. It compiles each pinned grammar's C (`parser.c`,
`scanner.c`) with wasi-sdk to WebAssembly and translates the module to Go with wasm2go. Its `tsql` module is pinned to
the original grammar, `alfpricot/tree-sitter-mssql` at `2e34d86e`.

This repository's guarantee (ADR-0004) is the grammar plus the constraint layer in `crates/tree-check`, which is Rust.
On the 2.0.0 oracle (10,300 inputs; acceptance at any recorded engine/level counts):
- the grammar alone parses all 7,504 engine-accepted cases cleanly;
- of the 2,795 engine-rejected inputs, the grammar rejects 852 (30.48 %);
- the constraint layer rejects 1,755 more;
- the remaining 188, plus one input the runner excludes because it sets PARSEONLY itself, are classified in
  `oracle/data/rejected-clean.json` (189 total: context 49, environment 123, version 15, runner 2; no `gap`).

Before this decision, the runtime bound of ADR-0002 §6 was 1.5x of the M2 end (`e8a398e`). After the M3 work
the compiled library stands at about 1.39x in the release audit below.

## Decision

1. **Grammar first.**
   - go-treesitter can move its `tsql` pin to this repository. The pin move is go-treesitter's change, under its own rules.
   - That gives clean parses for every engine-accepted case in this oracle at once.
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
   - The 2.0.0 release audit on Windows x64, gcc 16.2.0 `-O2`, using `scripts/runtime-cost.mjs` with five
     alternating runs, measured M2 library bytes 10,628,283 → 14,730,473 (1.386x) and median parse time
     1,849.3 → 1,755.0 ms (0.949x). Against the new milestone `4ff4c5f`, library size is 1.000x and parse
     time was 0.952x (1,423.4 → 1,354.8 ms). Timing varies by host and run. Peak memory was not measured
     on Windows; its gate runs only where GNU time is available.

## Consequences

- Consumers that only parse (highlighting, navigation, analysis) can use the grammar as soon as go-treesitter's pin
  moves.
- Consumers that validate T-SQL need the wasm constraint module. Checking parses the input again inside the module.
  The cost is measured and recorded with the module.
- A rule change becomes visible to Go consumers only through a new module build. The parity gate keeps the module
  equal to the native tool at each commit.
