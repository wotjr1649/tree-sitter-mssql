# SQL Server oracle

The reference for valid T-SQL is SQL Server's own parser under `SET PARSEONLY ON` ([ADR-0002](../docs/decisions/ADR-0002-verification-and-workflow.md)). This directory holds the cases, the engine's verdicts on them, the grammar's baseline results, and the runner that produced the verdicts.

| Path | Holds |
| --- | --- |
| `kit/cases.jsonl` | 726 cases from tree-sitter-grammar-kit: `id`, `sha256`, `source` ([NOTICE](kit/NOTICE)) |
| `kit/edits.jsonl` | 353 edit histories (939 edits) for comparing incremental and fresh parses |
| `verdicts/*.json` | Engine verdicts per case and compatibility level: the error numbers, `[]` for accepted. `meta` records the engine build, edition, platform, session `SET` state, levels, and the hashes of the cases and of the runner |
| `runner/ParseOnly/` | The runner that produces the verdicts (C#, .NET 10, Microsoft.Data.SqlClient pinned by `packages.lock.json`) |
| `expected.json` | The grammar's current result per case (`clean` or `error`) and the edit histories whose incremental parse differs from a fresh one |

## Reading the verdicts

- A case is **positive** when SQL Server accepts it at any verified level. A case is a **negative** only when every level rejects it.
- Rejections include context errors that a grammar cannot and should not catch (conflicting hints, boolean versus scalar, permissions). Agreement is therefore never expected to reach 100 %. Classifying the rejections as syntax or context is a later step.
- `verdicts/localdb-17.0.1000.7.json`: SQL Server 2025 LocalDB 17.0.1000.7 (RTM, Express, Windows), levels 100–170, observed 2026-11-15. The current C# runner and the PowerShell prototype it replaced gave identical verdicts on all 5,808 case-level pairs.

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

```sh
(cd oracle/runner/ParseOnly && dotnet restore --locked-mode && dotnet build -c Release --no-restore)
dotnet oracle/runner/ParseOnly/bin/Release/net10.0/ParseOnly.dll --self-test
dotnet oracle/runner/ParseOnly/bin/Release/net10.0/ParseOnly.dll --cases oracle/kit/cases.jsonl --out oracle/verdicts/<engine>.json
```

Without `--server` the runner uses SQL Server LocalDB with Windows authentication. With `--server HOST,PORT` it uses SQL authentication: the user comes from `MSSQL_USER` (default `sa`) and the password from `MSSQL_PASSWORD`. `--levels` restricts the compatibility levels.

What the runner does:

- It creates a temporary database per level and drops it at the end.
- Each case gets a new unpooled connection. `SET PARSEONLY ON` is sent as its own batch, then the case text unchanged.
- It records every error number the engine returns, and `[]` when the engine accepts the case.
- It never sends a case that mentions `PARSEONLY` (recorded as `UNSUPPORTED_CONTROL`).
- Connection failures, timeouts and fatal errors stop the run with exit 2. They are never recorded as rejections.
- Each verdict file records the hash of the runner's own source.

`--self-test` checks the runner against a live engine and exits 1 on any failure:

- a valid statement is accepted;
- a syntax error is rejected with 102;
- the batch-first rule for `CREATE PROCEDURE` holds;
- Unicode text is accepted;
- a parsed `CREATE TABLE` is not executed;
- guarded text is never sent.

The `Oracle engine` workflow runs the self-test and every case on real engines in Linux containers, each image pinned by digest:

| Engine | Levels |
| --- | --- |
| SQL Server 2017 CU31 | 100–140 |
| SQL Server 2019 CU32 | 100–150 |
| SQL Server 2022 CU28 | 100–160 |
| SQL Server 2025 CU11 | 100–170 |

It runs on pull requests that touch the oracle, and weekly. It compares each engine's verdicts with the committed LocalDB verdicts (`tools/verdict-diff.mjs`) and uploads them. Differences between engines are observations to classify; they do not fail the job. SQL Server 2008 to 2016 run only on Windows and are not verified.
