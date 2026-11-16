# SQL Server oracle

The reference for valid T-SQL is SQL Server's own parser under `SET PARSEONLY ON` ([ADR-0002](../docs/decisions/ADR-0002-verification-and-workflow.md)). This directory holds the cases, the engine's verdicts on them, the grammar's baseline results, and the runner that produced the verdicts.

| Path | Holds |
| --- | --- |
| `kit/cases.jsonl` | 726 cases from tree-sitter-grammar-kit: `id`, `sha256`, `source` ([NOTICE](kit/NOTICE)) |
| `kit/edits.jsonl`, `kit/route-edits.jsonl` | Edit histories for comparing incremental and fresh parses: 353 gap histories (939 edits) and 19 route histories (40 edits) |
| `probes/*.jsonl` | Cases written and engine-checked while fixing defects. They gate those fixes like the kit cases do. IDs are prefixed with the set name. Current sets: `kit-dynamic-sql` (grammar-kit's dynamic SQL fixture), `backup-restore` (666 BACKUP and RESTORE forms: every WITH option name bare, with a number and with a string in both statements, devices, file lists, MIRROR TO, certificate and key backups), `query-forms` (174 query, DML, cursor, EXEC and native-module forms), `ddl-database` (259 database, availability group and server DDL forms), `ddl-external` (184 EXTERNAL/FULLTEXT/ENDPOINT neighbours), `ddl-table` (146 adjacent table/index DDL forms), `hint-rules` (596, the table-hint rules: every hint alone, every pair, every hint on each DML target), `table-hints` (165, hint syntax), `context` (797, the context rules ported from grammar-kit under ADR-0004: permissions, all 312 trigger event types on both scopes, built-in arity, scalar RETURN, CTE table source, window frame, MERGE terminator), `kit-routes` (640 realistic scripts from grammar-kit, ADR-0004; 44 contain `GO` lines, which the runner splits like a client), `queue-activation` (86, the QUEUE fix and constraint rules), `semicolon` (70), `exec-output` (18, EXEC OUTPUT rules), `identifiers` (34: Unicode names, bare `#`/`@` sigil names, and variables in column, table and MERGE alias positions, which the engine rejects) , `column-constraints` (28: table constraint forms inside a column definition, and foreign keys in table variables and types) and `batch-rules` (50: statements that must start or end a batch, and GO lines inside comments and strings) |
| `verdicts/*.json` | Engine verdicts per case and compatibility level: the error numbers, `[]` for accepted. `meta` records the engine build, edition, platform, session `SET` state, levels, and the hashes of the cases and of the runner. Every case set has verdicts from LocalDB (`localdb-17.0.1000.7[.set].json`) and from the four Linux engines of the CI matrix (`linux-<build>[.set].json`, downloaded from the `oracle-engine` workflow artifacts). LocalDB is an Express edition, so edition-limited forms (Msg 1712, 11905) are accepted only by the Linux Developer edition engines |
| `runner/ParseOnly/` | The runner that produces the verdicts (C#, .NET 10, Microsoft.Data.SqlClient pinned by `packages.lock.json`) |
| `expected.json` | The grammar's current result per case (`clean` or `error`) and the edit histories whose incremental parse differs from a fresh one |
| `rejected-clean.json` | Every case that every engine rejects but that parses cleanly, with its class: `gap` (decidable from the tree, not enforced yet), `context` (needs declarations or the catalog), `version` (a feature these engine builds lack), `environment` (edition, platform or server configuration), `runner` (not judgeable). Classes come from the newest engine's error. The replay fails on an unclassified case or a stale entry, so a change that leaves a rejected case clean must classify it |

The `table-hints` probe set adds 165 cases for INDEX/FORCESEEK arguments, numeric and token-length
bounds, adjacent/space-separated hints, query TABLE HINT and UPDATE/DELETE targets. Its committed
LocalDB verdicts cover levels 100–170; the matching corpus includes the documented context gaps.

The `ddl-external` set is measured on LocalDB 17.0.1000.7, levels 100–170. Its five
rejected-but-clean cases are Full-Text component/endpoint edition limits (7609/7878);
option-set rejections are checked in tree-check.
The query-forms set adds 174 query, DML, cursor, EXEC and native-module neighbours measured on
LocalDB 17.0.1000.7, levels 100–170 (108 accepted, 66 rejected). Every case is pinned in
test/corpus/query-forms.txt. The set deliberately retains engine-accepted budget-stopped and pre-existing
gaps; a corpus tree containing ERROR is not a claim that SQL Server rejects its source.
The matching constraints and their engine evidence live in tools/tree-check/src/constraints.rs.

## Reading the verdicts

- A case is **positive** when SQL Server accepts it at any verified level. A case is a **negative** only when every level rejects it.
- Rejections include context errors that a grammar cannot and should not catch (conflicting hints, boolean versus scalar, permissions). Agreement is therefore never expected to reach 100 %. Classifying the rejections as syntax or context is a later step.
- The replay merges every verdict file; each file is bound by its `meta.cases_sha256` to the one case set (kit or a probe set) it was made from. A case is positive when any engine at any level accepts it **under the same session setting**.

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
cargo build --locked --manifest-path tools/tree-check/Cargo.toml
node tools/oracle-replay.mjs --tree-check target/debug/tree-check   # with CARGO_TARGET_DIR=target
```

The replay sends `--quoted-identifier on|off` to `tree-check` for the session recorded in each verdict. Existing ON cases keep their IDs; OFF cases use `<id>@qi-off` in `expected.json` and `rejected-clean.json`. Missing session metadata means the historical ON default. ON and OFF acceptance are never merged. Every case set must have engine verdicts under both settings; missing ON or OFF coverage fails the replay. Edit histories compare the shared, session-independent grammar tree once.

The replay judges every case with `tree-check` (through the tree-sitter API) and prints the agreement table. It replays the edit histories and compares the results with `expected.json`. Exit codes:

- **0:** the results equal the baseline.
- **1:** the results differ from the baseline. Review the listed changes, then rerun with `--update` in the same pull request.
- **2:** the inputs are inconsistent, for example a hash mismatch or verdicts made from different cases.

`--update` refuses to record any engine-accepted case with an error, including a newly added case. The replay also refuses a baseline with any accepted-and-error case.

## Producing verdicts

```sh
(cd oracle/runner/ParseOnly && dotnet restore --locked-mode && dotnet build -c Release --no-restore)
dotnet oracle/runner/ParseOnly/bin/Release/net10.0/ParseOnly.dll --self-test
dotnet oracle/runner/ParseOnly/bin/Release/net10.0/ParseOnly.dll --cases oracle/kit/cases.jsonl --out oracle/verdicts/<engine>.json
```

Without `--server` the runner uses SQL Server LocalDB with Windows authentication. With `--server HOST,PORT` it uses SQL authentication: the user comes from `MSSQL_USER` (default `sa`) and the password from `MSSQL_PASSWORD`. `--levels` restricts the compatibility levels. `--quoted-identifier on|off` selects the initial session setting (default `on`); the runner sets it in its own control batch before `SET PARSEONLY ON` and before sending the case text unchanged. `meta.session_set.quoted_identifier` records the verified numeric `SESSIONPROPERTY` value (`1` for ON, `0` for OFF).

What the runner does:

- It creates a temporary database per level and drops it at the end.
- Each case gets a new unpooled connection. `SET QUOTED_IDENTIFIER ON|OFF` and `SET PARSEONLY ON` are sent as separate control batches, then the case text unchanged.
- It records every error number the engine returns, and `[]` when the engine accepts the case.
- It never sends a case that mentions `PARSEONLY` (recorded as `UNSUPPORTED_CONTROL`).
- Connection failures, timeouts and fatal errors stop the run with exit 2. They are never recorded as rejections.
- Each verdict file records the hash of the runner's own source.

`--self-test` checks the runner against a live engine and exits 1 on any failure:

- a valid statement is accepted;
- a double-quoted table name is accepted under ON and rejected under OFF;
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


## QUOTED_IDENTIFIER sessions

The `metamorphic-whitespace` set adds 110 cases for 55 code points in two unambiguous token-gap positions, engine-checked under both settings at levels 100–170. SQL Server accepts the 53 separators represented by U+0000–0020, U+0085, U+00A0, U+1680, U+2000–200B, U+2028–2029, U+202F, U+205F and U+3000; U+180E and U+FEFF are rejected.

The 122-case `probes/quoted-identifier.jsonl` set covers quoted names, expression values, LIKE/IN,
EXEC arguments and dynamic SQL, DDL, defaults, XML methods, indexed views, computed-column and
filtered indexes, inline SET switches across GO, stored-procedure capture and dynamic-SQL isolation. LocalDB verdicts cover levels 100–170 under both settings. Session-specific files
use `<engine>[.<set>].qi-on.json` and `<engine>[.<set>].qi-off.json` (the kit omits `<set>`);
legacy files without a session suffix retain their recorded ON setting. The new quoted-identifier set
uses explicit suffixes for both sessions; other sets retain their legacy ON filename. For example:

```sh
dotnet oracle/runner/ParseOnly/bin/Release/net10.0/ParseOnly.dll --cases oracle/probes/quoted-identifier.jsonl --out oracle/verdicts/localdb-17.0.1000.7.quoted-identifier.qi-off.json --quoted-identifier off
```

The engine workflow measures every set under both settings and compares each with the matching
LocalDB session file. The other committed case sets also have LocalDB OFF verdicts at levels 100–170.
The replay judges every source under both settings using separate engine observations.

The ON requirements for indexed views, filtered indexes, computed-column indexes and XML methods
are checked when SQL Server compiles or executes the relevant operation. `SET PARSEONLY ON` accepts
these probes under OFF as well. These observations do not establish that the operations execute
under OFF, and the syntax replay must not reject them solely for that execution requirement.
