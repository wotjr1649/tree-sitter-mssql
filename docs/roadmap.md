# Roadmap

Phases in order. Each phase ends when its "done when" holds. The definitions, gates and budgets are in
[ADR-0002](decisions/ADR-0002-verification-and-workflow.md). The reasons for continuing from the original grammar are in
[ADR-0001](decisions/ADR-0001-continue-from-upstream-v1.md); the repository is independent since
[ADR-0007](decisions/ADR-0007-independent-repository.md). All work lands on `main` through short topic branches.

## M0.5: Foundation on `main`

The foundation landed on `main` in merge commit `c696e2c` (PR #1, 2026-11-15). It preserved the imported
history, removed the original registry-publishing and automatic-release workflows, and set `main` as
Verify's target branch. Completion required a full Verify pass on the merge commit.

## M1: Surface inventory and SQL Server oracle

- Surface inventory v0: statements and sub-forms from the Microsoft T-SQL reference, cross-checked against
  SqlScriptDOM's grammar productions.
- Oracle data: the 726 grammar-kit cases (source and engine errors only, MIT notice, original IDs and hashes) and its
  353 edit histories. grammar-kit's stored trees and its own `syntax_error` flags are not imported as expectations.
- A tree checker that uses the tree-sitter API (`has_error`, `is_missing`, full range), tested against inputs that the
  CLI summary misreports.
- A PARSEONLY runner uses LocalDB locally. The Oracle engine workflow runs pinned SQL Server 2017, 2019,
  2022 and 2025 Linux images and treats infrastructure failures as failures.
- The M1 design called for offline verdict replay on every pull request, per-case regression gates, and the cost report.

The original import recorded 726 cases (681 judged, 45 unknown) and 708 distinct sources (665 judged).
M1's completion criteria were to reproduce those counts, record verdicts at every compatibility level from
100 through 170, and demonstrate that CI fails on an injected regression.

The current Verify gate replays every committed case set and session. See [the oracle README](../oracle/README.md)
for coverage and the engine workflow.

## M2: Defect classes (completed)

M2 called for measuring grammar-kit's patch chain (per-case transitions, corpus and fixture breakage, node-type
diff and cost), then re-implementing each defect class here in a separate pull request:

1. `CREATE/ALTER QUEUE … WITH ACTIVATION (…, EXECUTE AS …)`.
2. Clauses without FROM: `WHERE`, `GROUP BY`, `HAVING`, rewriting the two error cases inherited from 1.0.0.
3. Unicode identifiers and variables (Unicode 3.2 BMP letters and digits, per engine verdicts).
4. Leading and empty `;`, and CTE boundaries.
5. Table-hint forms and their numeric bounds.
6. Trailing comma in `CREATE TABLE`.

Each class required a failing case before the change, engine evidence, adjacent negatives and a pass on every gate
within the cost budget. The M2 cost milestone was recorded at `e8a398e` (PR #25). See ADR-0002 for the subsequent
baseline changes.

## M2.5: Absorb grammar-kit's T-SQL work (ADR-0004)

- Port the tree-decidable checks of grammar-kit's `CheckTSQLContext` into `crates/tree-check`, one rule family per pull request, each with oracle evidence: hint conflicts and arguments, DML-target hints, column permissions, trigger name/owner/event scope, window frames, COUNT arity, RETURN in scalar functions, CTE without a table source, scalar versus predicate.
- Import grammar-kit's 640 route cases as an oracle probe set with fresh verdicts on every verified engine, and its T-SQL queries.
- Record the decisions in grammar-kit's reports where they apply.

Completion covers the checks and assets adopted here, with engine agreement on their oracle cases. The Go API
contracts listed below stay with go-treesitter under ADR-0004.

Status (2026-11-16):
- Ported: every `CheckTSQLContext` check (#25, #26 and the scalar-predicate rule). Each was re-measured on the engine.
- Imported: the 726 gap cases and 353 edit histories (M1), the 640 route cases and their 19 edit histories (#24),
  the six T-SQL queries with their capture cases (`test/queries/kit`), and the dynamic SQL fixture.
- Already the same as the oracle: `testdata/tsql-context` (the 726 gap cases).
- Used as evidence only:
  - the C2 patch chain (`src/dev/c2-patches/tsql.json`), re-implemented here;
  - the T-SQL issue reports, whose cases are in the oracle under their issue IDs.
- Left to go-treesitter (Go API contracts, ADR-0004 §1): the T-SQL fact-query mapping and the `tsql8` query
  classification.

## M3: Surface rounds and remaining coverage

The M3 rounds ended at `4ff4c5f` (PR #46). They expanded DDL, query, DML, module and expression coverage.
Session-aware QUOTED_IDENTIFIER ON/OFF checking subsequently landed under ADR-0006, and the committed
oracle covers both initial settings.

The grammar and tests cover GO and modeled sqlcmd syntax. The engine runner splits supported GO lines
and sends T-SQL batches to SQL Server. The full client-syntax suite remains subject to the ADR-0002
inventory criteria.

A declared inventory cell is verified when it has the required positive and adjacent negative cases,
expected tree, applicable edit history and engine verdict. An issue alone leaves the cell open. See
[Accepted gaps](accepted-gaps.md) and [the oracle](../oracle/README.md) for measured cases.

## M4: Consumers

go-treesitter pins a release of this repository and regenerates `src/parser.c` with its own pinned CLI. Pin moves
such as an update to `v2.2.0` are separate, reviewed changes. Each records the node-type diff, native tree diff,
cost and memory.

Done when each consumer move has its record.
