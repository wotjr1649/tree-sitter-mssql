# SQL Server oracle

The reference for valid T-SQL is SQL Server's own parser under `SET PARSEONLY ON` ([ADR-0002](../docs/decisions/ADR-0002-verification-and-workflow.md)). This directory holds the cases, the engine's verdicts on them, the grammar's baseline results, and the runner that produced the verdicts.

| Path | Holds |
| --- | --- |
| `kit/cases.jsonl` | 726 cases from tree-sitter-grammar-kit: `id`, `sha256`, `source` ([NOTICE](kit/NOTICE)) |
| `kit/edits.jsonl` | 353 edit histories (939 edits) for comparing incremental and fresh parses |
| `verdicts/*.json` | Engine verdicts per case and compatibility level: the error numbers, `[]` for accepted. `meta` records the engine build, edition, platform, session `SET` state, levels, and the hashes of the cases and of the runner |
| `runner/parseonly-sweep.ps1` | The runner that produced the verdicts (prototype in PowerShell 7). A C# runner replaces it |
| `expected.json` | The grammar's current result per case (`clean` or `error`) and the edit histories whose incremental parse differs from a fresh one |

## Reading the verdicts

- A case is **positive** when SQL Server accepts it at any verified level. A case is a **negative** only when every level rejects it.
- Rejections include context errors that a grammar cannot and should not catch (conflicting hints, boolean versus scalar, permissions). Agreement is therefore never expected to reach 100 %. Classifying the rejections as syntax or context is a later step.
- `verdicts/localdb-17.0.1000.7.json`: SQL Server 2025 LocalDB 17.0.1000.7 (RTM, Express, Windows), levels 100–170, observed 2026-11-15. Running the runner again on the same engine reproduced every one of the 5,808 verdicts.

## Checking the grammar

```sh
cargo build --locked --manifest-path tools/tree-check/Cargo.toml
node tools/oracle-replay.mjs --tree-check target/debug/tree-check   # with CARGO_TARGET_DIR=target
```

The replay judges every case with `tree-check` (through the tree-sitter API) and prints the agreement table. It replays the edit histories and compares the results with `expected.json`. Exit codes:

- **0:** the results equal the baseline.
- **1:** the results differ from the baseline. Review the listed changes, then rerun with `--update` in the same pull request.
- **2:** the inputs are inconsistent, for example a hash mismatch or verdicts made from different cases.

`--update` refuses to record an engine-accepted case that turned from `clean` into `error`. Such a regression needs `--allow-regression=ID,...` and a reason in the pull request.

## Producing verdicts

On Windows with SQL Server LocalDB:

```sh
pwsh -File oracle/runner/parseonly-sweep.ps1 -Cases oracle/kit/cases.jsonl -Out oracle/verdicts/<engine>.json
```

- The runner creates a temporary database per level and drops it at the end.
- Each case gets a new connection. `SET PARSEONLY ON` is sent as its own batch, before the case's text.
- A case that mentions `PARSEONLY` is never sent.
- A connection or control failure stops the run; it is never recorded as a rejection.
