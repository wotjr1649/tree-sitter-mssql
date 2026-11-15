# ADR-0002: Verification model and workflow toward complete T-SQL support

- Status: accepted (owner decisions, 2026-11-15)
- Date: 2026-11-15

## Context

The goal is complete T-SQL support with zero defects (ADR-0001: SQL Server's parser is the reference). Measurements on
2026-11-15:

- Upstream v1.0.0 agrees with SQL Server 2025 LocalDB 17.0.1000.7 (`SET PARSEONLY ON`) on 386 of 681 judged cases at
  compatibility level 170 (56.7 %). The 726 cases from tree-sitter-grammar-kit hold 708 distinct sources.
- grammar-kit carries a patch chain against the same base tree as this repository's commit `2e34d86e`. Applied there,
  it reaches 514 of 681 (75.5 %), but turns 6 engine-accepted cases into `ERROR`. It also fails 81 of 370 current
  corpus cases and 2 fixtures, removes 15 node kinds, and grows `src/parser.c` from 30.4 MB to 68.7 MB (states 15,192 →
  35,879). Merging it with this repository's sources conflicts in 15 hunks across 7 files.
- The tree-sitter CLI's parse summary (`successful parses`, exit code) can report success for trees that contain `ERROR`
  or only zero-width nodes, for example `SELECT * FROM dbo.t WITH (FORCESEEK(2(a)));`.
- All 773 grammar rules are reachable, and each of the 18 `conflicts` entries is required. Removing any one of them makes
  generation fail.

## Decision

1. **Definition of complete support.** A surface inventory lists every T-SQL statement, its sub-forms, the lexical rules
   and the environment settings. It is built from the Microsoft T-SQL reference (with a recorded access date) and
   cross-checked against the production rules of Microsoft's SqlScriptDOM grammar (MIT). Each cell needs:
   - a positive case and an adjacent negative case
   - the expected tree
   - an edit history where relevant
   - an engine verdict

   Complete support means every declared cell passes and none is unknown or unimplemented. A cell with only an
   issue attached is not done. The inventory includes `QUOTED_IDENTIFIER OFF` and the client syntax (`GO`, sqlcmd)
   as cells.
2. **Definition of zero defects.** Within the verified surface the following are all zero:
   - syntax false negatives (engine accepts, tree has `ERROR`/`MISSING`)
   - syntax false positives (engine rejects with a syntax error, tree is clean)
   - unintended changes to node kinds, fields, ranges or query results
   - new fresh-versus-incremental differences
   - crashes and hangs

   A known exception that remains is reported as "no new regressions, N known exceptions", not as zero.
   Context-dependent rejections (conflicting hints, boolean versus scalar, names, permissions) are not grammar
   defects. grammar-kit's `CheckTSQLContext` diagnoses them, and this repository classifies them as `contextual`.
3. **Verified engine profile.** SQL Server 2025 (17.x) at every compatibility level it supports: 100, 110, 120,
   130, 140, 150, 160 and 170 (owner decision, 2026-11-15; first measured the same day on LocalDB 17.0.1000.7: levels
   100 to 120 gave identical verdicts on the 726 cases, and 15 cases change verdict between levels).
   Real older engines are verified too (owner decision, 2026-11-16), each at the levels it supports, in Linux
   containers pinned by digest:

   | Engine | Levels |
   | --- | --- |
   | SQL Server 2017 CU31 (14.0.3570.2) | 100–140 |
   | SQL Server 2019 CU32 (15.0.4510.2) | 100–150 |
   | SQL Server 2022 CU28 (16.0.4325.4) | 100–160 |
   | SQL Server 2025 CU11 (17.0.5065.9) | 100–170 |

   - A case is positive if any verified engine and level accepts it. It is a syntax negative only if every one of
     them rejects it on syntax grounds.
   - Differences between engines are recorded, not overwritten. Linux rejects non-SQL-Server OPENROWSET providers
     with 7222, which is a platform difference and not syntax.
   - SQL Server 2008 to 2016 (Windows only) and Azure are marked unverified rather than claimed.
   - Verdicts record engine build, edition, platform, level, initial `SET` state, mode and case hash. Only
     `PARSEONLY` verdicts count; errors obtained by compiling (for example 4193, 4194, 4406 in grammar-kit's data)
     are context, not syntax.
4. **Oracle mechanics.**
   - CI replays committed verdicts on every pull request; verdicts depend on the case and the engine, not on the
     grammar.
   - A live engine (a SQL Server 2025 Linux container pinned by digest; LocalDB locally) runs only when cases, the
     runner, the profile or the classification change, and weekly.
   - The runner only parses: `PARSEONLY`, with no execution paths, a dedicated temporary database, and a control
     batch separate from the input batch. Infrastructure failures are reported as failures, never as rejections.
   - Tree errors are judged through the tree-sitter API (`has_error`, `is_missing`, full source range), not through
     the CLI summary.
5. **Regression gates.** No case that parsed cleanly may gain `ERROR`/`MISSING`, and no fixed syntax negative may be
   accepted again. Aggregate agreement, per-distinct-source results and per-class results are reported side by side.
6. **Cost budget.** Every pull request reports `src/parser.c` bytes, `STATE_COUNT`, `LARGE_STATE_COUNT`, `SYMBOL_COUNT`,
   `PRODUCTION_ID_COUNT` and the inherited field-map entries against the `main` baseline.
   - Growth must be explained by the cells it adds.
   - More than 5 % growth warns, and more than 10 % needs the budget raised in the same pull request with the owner's
     approval.
   - Parse time is measured in alternating runs, and a median slowdown above 10 % is investigated.
7. **grammar-kit patches.**
   - The patch chain is re-measured as an experiment and is not adopted wholesale.
   - Fixes land one defect class per pull request, re-implemented in this grammar with grammar-kit's reports and
     cases as evidence.
   - Wholesale adoption is considered only if every gate above passes with no node kind removed.
8. **Workflow.**
   - `main` is the only long-lived branch and stays green. Work happens on short topic branches from the latest
     `main`, merged by pull request with a merge commit (no squash or rebase: the imported upstream history stays
     intact).
   - Commit pins are the release mechanism. If tags are ever needed they use the prefix `fork-v`, because upstream
     may tag the same version numbers.
   - Dependabot targets `main`. The tree-sitter CLI is excluded from automatic version updates because consumers
     regenerate with the same pinned version.
9. **Upstream's deliberate error tests.** `test/corpus/errors.txt` pins `SELECT 1 WHERE 1=1;` and
   `SELECT 1 GROUP BY 1;` as errors, while SQL Server accepts both (measured at levels 110 and 170 on 2026-11-15). They are rewritten as
   positive cases in the change that fixes the grammar, and the divergence is recorded for upstream merges
   in [`docs/upstream-divergence.md`](../upstream-divergence.md).
10. **`docs/accepted-gaps.md`** stays as the ledger, rewritten against engine verdicts. Each row carries a case ID,
    direction, profile and issue, and rows not yet measured are marked `UNVERIFIED`.

## Consequences

- The oracle and its gates come before grammar fixes, and fixes become measurable per case.
- Complete support will grow the parser. The budget makes each increase deliberate instead of forbidding it.
- Supporting `QUOTED_IDENTIFIER OFF` requires a stateful external scanner with serialization. It is planned as its
  own change with incremental-parsing tests.
- Consumers move their pins deliberately. Behaviour changes arrive as separate, reviewed pin moves.
