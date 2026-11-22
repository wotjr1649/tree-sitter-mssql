# SQL Server oracle

The reference for valid T-SQL is SQL Server's own parser under `SET PARSEONLY ON` ([ADR-0002](../docs/decisions/ADR-0002-verification-and-workflow.md)). This directory holds the cases, the engine's verdicts on them, the grammar's baseline results, and the runner that produced the verdicts.

| Path | Holds |
| --- | --- |
| `data/kit/cases.jsonl` | 726 grammar-kit cases with `id`, `sha256` and `source`. See [NOTICE](data/kit/NOTICE). |
| `data/kit/edits.jsonl`, `data/kit/route-edits.jsonl` | 353 gap histories (939 edits) and 19 route histories (40 edits) for comparing incremental and fresh parses. |
| `data/kit/batch-rules-edits.jsonl` | The five-edit #87 history, with a recorded incremental difference at step 5. |
| `data/probes/*.jsonl` | Engine-checked defect and surface cases, with IDs prefixed by set. The replay discovers every probe file automatically. The 32 probe sets plus the kit contribute 10,841 sources (21,682 session/case pairs). |
| `data/verdicts/*.json` | Per-engine, per-session and per-level observations. An empty error array means acceptance. `UNSUPPORTED_CONTROL` marks source skipped by the PARSEONLY guard. Metadata records the engine, levels, session setting and case and runner hashes. LocalDB has committed ON and OFF verdicts for every set. The four Linux engines have committed ON verdicts for every set except batch-rules, column-constraints, if-nesting, korean, metamorphic-whitespace and quoted-identifier. The engine workflow measures both settings for every set. |
| `runners/ParseOnly/` | The .NET 10 runner, with Microsoft.Data.SqlClient pinned by `packages.lock.json`. |
| `data/expected.json` | The current tree-check verdict per session/case pair and edit histories whose incremental parse differs from a fresh parse. |
| `data/rejected-clean.json` | Clean session/case pairs with no accepted engine verdict. Each entry has a reason and a class: `gap` (a check the tree can decide), `context` (depends on declarations or the catalog), `version` (unavailable in the recorded builds), `environment` (edition, platform or configuration), or `runner` (a runner limitation). Replay fails when an entry is missing or stale. |

The `table-hints` probe set adds 165 cases for INDEX/FORCESEEK arguments, numeric and token-length
bounds, adjacent/space-separated hints, query TABLE HINT and UPDATE/DELETE targets. Its committed
LocalDB verdicts cover levels 100–170; the matching corpus includes the documented context gaps.

The `ddl-external` set is measured on LocalDB 17.0.1000.7, levels 100–170. Its five
rejected-but-clean cases are Full-Text component/endpoint edition limits (7609/7878);
option-set rejections are checked in tree-check.
The `query-forms` set adds 174 query, DML, cursor, EXEC and native-module neighbours measured on
LocalDB 17.0.1000.7, levels 100–170 (108 accepted, 66 rejected). Every case is pinned in
`test/corpus/query-forms.txt`. At R13 this set recorded engine-accepted forms deferred by the
parser-cost budget. The deferred cases now pass tree-check after R17 and R18. Matching rules live
under [constraints/](../crates/tree-check/src/constraints/), with evidence in the corresponding probes.

The `batch-rules` set has 174 cases, including trigger, certificate and function body
constraints, reserved names and standalone column options. LocalDB 17.0.1000.7 accepts 85
cases with QUOTED_IDENTIFIER ON and 84 with OFF at one or more of levels 100–170.
The set has ScriptDOM observations at the same levels in both sessions.

## Reading the verdicts

- A case is **positive** in a session when at least one recorded engine and level returns an
  empty error array for that session. A case with only rejections or an unsupported-control outcome
  has no accepted verdict. The ledger explains why some of those cases pass tree-check.
- A case is a **syntax negative** only when every recorded engine and level for that session
  rejects it on syntax grounds. Catalog, permission, edition and configuration errors can produce
  rejections without establishing a syntax defect.
- The replay binds every verdict file to its source set using `meta.cases_sha256` and keeps ON
  and OFF observations separate.

| Verdict file | Engine | Platform | Levels | Cases that differ from LocalDB at a shared level |
| --- | --- | --- | --- | ---: |
| `localdb-17.0.1000.7.json` | 2025 RTM, Express | Windows (LocalDB) | 100–170 | — |
| `linux-17.0.5065.9.json` | 2025 CU11, Enterprise Developer | Linux | 100–170 | 5 |
| `linux-16.0.4325.4.json` | 2022 CU28, Developer | Linux | 100–160 | 9 |
| `linux-15.0.4510.2.json` | 2019 CU32, Developer | Linux | 100–150 | 12 |
| `linux-14.0.3570.2.json` | 2017 CU31, Developer | Linux | 100–140 | 19 |

- The C# runner and the PowerShell prototype it replaced gave identical LocalDB verdicts on all 5,808 case-level pairs.
- The Linux files are the `Oracle engine` workflow's output.
- The 5 cases that differ on 2025 CU11 are OPENROWSET with a non-SQL-Server provider, which Linux rejects with 7222 (platform, not syntax).
- The older engines also reject features they predate, such as UNMASK permissions and `REGEXP_*`.

## Checking the grammar

```sh
export CARGO_TARGET_DIR=target
cargo build --locked --manifest-path crates/tree-check/Cargo.toml --bin tree-check
node scripts/oracle-replay.mjs --tree-check target/debug/tree-check
```

The replay sends `--quoted-identifier on|off` to `tree-check` for the session recorded in each verdict. Existing ON cases keep their IDs; OFF cases use `<id>@qi-off` in `expected.json` and `rejected-clean.json`. Missing session metadata means the historical ON default. ON and OFF acceptance are never merged. Every case set must have engine verdicts under both settings; missing ON or OFF coverage fails the replay. Edit histories compare the shared, session-independent grammar tree once.

The replay judges every case with `tree-check` (through the tree-sitter API) and prints the agreement table. It replays the edit histories and compares the results with `expected.json`. Exit codes:

- **0:** the results equal the baseline.
- **1:** The baseline differs, an engine-accepted case fails, or the rejected-clean ledger is
  out of sync. Resolve the listed failures first. Use `--update` to record a verified baseline
  change once the acceptance and classification checks pass.
- **2:** the inputs are inconsistent, for example a hash mismatch or verdicts made from different cases.

`--update` refuses to record any engine-accepted case with an error, including a newly added case. The replay also refuses a baseline with any accepted-and-error case.

## Producing verdicts

From the repository root, run these refresh commands against LocalDB 17.0.1000.7. For another
engine or build, write to separate verdict files named for the product version reported in
`meta.engine.product_version`.

```sh
runner=oracle/runners/ParseOnly/bin/Release/net10.0/ParseOnly.dll
(cd oracle/runners/ParseOnly && dotnet restore --locked-mode && dotnet build -c Release --no-restore) &&
  dotnet "$runner" --self-test &&
  dotnet "$runner" --cases oracle/data/kit/cases.jsonl \
    --out oracle/data/verdicts/localdb-17.0.1000.7.json --quoted-identifier on &&
  dotnet "$runner" --cases oracle/data/kit/cases.jsonl \
    --out oracle/data/verdicts/localdb-17.0.1000.7.qi-off.json --quoted-identifier off
```

Without `--server` the runner uses LocalDB with Windows authentication. With `--server HOST,PORT`
it reads SQL credentials from `MSSQL_USER` (default `sa`) and `MSSQL_PASSWORD`. Use `--levels` for
a subset of compatibility levels and `--quoted-identifier on|off` for the initial session setting.
The verdict records the numeric SESSIONPROPERTY value in `meta.session_set.quoted_identifier`.

- The runner creates a temporary database for each level and drops it at the end.
- Each case gets a new unpooled connection. The runner sends `SET QUOTED_IDENTIFIER` and
  `SET PARSEONLY ON` as separate control batches.
- The runner sends each nonblank part between recognized GO lines as one batch. Its line-based split also
  applies inside comments and strings. The runner consumes a repeat count as part of the separator
  without repeating the batch. A GO line ending in a semicolon is outside the splitter's pattern.
- It records the engine's error numbers across the submitted batches and `[]` for acceptance. Source containing
  PARSEONLY is recorded as `UNSUPPORTED_CONTROL` and skipped.
- Connection failures, timeouts and fatal errors abort the run with exit 2.
- Each verdict file records the SHA-256 of the embedded ParseOnly.csproj and Program.cs bytes, in that order.

`--self-test` checks the runner on a live engine at its default compatibility level. Test failures
return 1. Infrastructure or argument failures return 2. Run it with `--quoted-identifier on`
and then with `--quoted-identifier off` to verify both sessions. The self-test checks that:

- a valid statement is accepted
- a double-quoted table name is accepted under ON and rejected under OFF
- a syntax error is rejected with 102
- `CREATE PROCEDURE` must start its batch
- GO splits batches, and an empty or GO-only script is accepted
- Unicode text is accepted
- a parsed `CREATE TABLE` is not executed
- guarded text is never sent

The `Oracle engine` workflow runs the self-test and every case on real engines in Linux containers, each image pinned by digest:

| Engine | Levels |
| --- | --- |
| SQL Server 2017 CU31 GDR20 | 100–140 |
| SQL Server 2019 CU32 GDR13 | 100–150 |
| SQL Server 2022 CU28 | 100–160 |
| SQL Server 2025 CU11 | 100–170 |

The workflow runs weekly, on manual dispatch, and on pull requests that change
`oracle/runners/`, `oracle/data/kit/`, `oracle/data/probes/`, `oracle/data/verdicts/`,
`scripts/verdict-diff.mjs` or the workflow file. It compares each run with the matching
committed LocalDB session using [verdict-diff.mjs](../scripts/verdict-diff.mjs) and uploads
the verdicts. Engine differences are reported for classification. The verified matrix starts
at SQL Server 2017.


## QUOTED_IDENTIFIER sessions

The `metamorphic-whitespace` set adds 110 cases for 55 code points in two unambiguous token-gap positions, engine-checked under both settings at levels 100–170. SQL Server accepts the 53 separators represented by U+0000–0020, U+0085, U+00A0, U+1680, U+2000–200B, U+2028–2029, U+202F, U+205F and U+3000; U+180E and U+FEFF are rejected.

The 130-case `probes/quoted-identifier.jsonl` set covers quoted names, expression values, LIKE/IN,
EXEC arguments and dynamic SQL, DDL, defaults, XML methods, indexed views, computed-column and
filtered indexes, inline SET switches across GO, stored-procedure capture, dynamic-SQL isolation and
WRITETEXT BULK. LocalDB verdicts cover levels 100–170 under both settings. Session-specific files
use `<engine>[.<set>].qi-on.json` and `<engine>[.<set>].qi-off.json` (the kit omits `<set>`);
legacy files without a session suffix retain their recorded ON setting. The new quoted-identifier set
uses explicit suffixes for both sessions; other sets retain their legacy ON filename. For example:

```sh
dotnet oracle/runners/ParseOnly/bin/Release/net10.0/ParseOnly.dll --cases oracle/data/probes/quoted-identifier.jsonl --out oracle/data/verdicts/localdb-17.0.1000.7.quoted-identifier.qi-off.json --quoted-identifier off
```

The engine workflow measures every set under both settings and compares each with the matching
LocalDB session file. The other committed case sets also have LocalDB OFF verdicts at levels 100–170.
The replay judges every source under both settings using separate engine observations.

The ON requirements for indexed views, filtered indexes, computed-column indexes and XML methods
are checked when SQL Server compiles or executes the relevant operation. `SET PARSEONLY ON` accepts
these probes under OFF as well. These observations do not establish that the operations execute
under OFF, and the syntax replay must not reject them solely for that execution requirement.
