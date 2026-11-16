# Roadmap

Phases in order. Each phase ends when its "done when" holds. The definitions, gates and budgets are in
[ADR-0002](decisions/ADR-0002-verification-and-workflow.md). The reasons for continuing from upstream are in
[ADR-0001](decisions/ADR-0001-continue-from-upstream-v1.md). All work lands on `main` through short topic branches.

## M0.5: Foundation on `main`

Remove upstream's release automation, point every policy at `main`, merge the foundation with upstream history
preserved.

Done when the foundation is merged into `main` by merge commit and the full `Verify` run passes on it.

## M1: Surface inventory and SQL Server oracle

- Surface inventory v0: statements and sub-forms from the Microsoft T-SQL reference, cross-checked against
  SqlScriptDOM's grammar productions.
- Oracle data: the 726 grammar-kit cases (source and engine errors only, MIT notice, original IDs and hashes) and its
  353 edit histories. grammar-kit's stored trees and its own `syntax_error` flags are not imported as expectations.
- A tree checker that uses the tree-sitter API (`has_error`, `is_missing`, full range), tested against inputs that the
  CLI summary misreports.
- A parse-only runner (LocalDB locally, a SQL Server 2025 container pinned by digest in CI), with infrastructure
  failures reported as failures.
- Gates: offline verdict replay on every pull request, per-case regression gates, and the cost report.

Done when the counts 726 / 681 judged / 45 unknown and 708 / 665 distinct sources are reproduced, every level from 100
to 170 is recorded, and an injected regression makes CI fail.

## M2: Defect classes

First, re-run grammar-kit's patch chain as a measured experiment (per-case transitions, corpus and fixture breakage,
node-type diff, cost). Then fix one class per pull request, re-implemented here:

1. `CREATE/ALTER QUEUE … WITH ACTIVATION (…, EXECUTE AS …)`.
2. Clauses without FROM: `WHERE`, `GROUP BY`, `HAVING`, rewriting the two upstream error cases.
3. Unicode identifiers and variables (Unicode 3.2 BMP letters and digits, per engine verdicts).
4. Leading and empty `;`, and CTE boundaries.
5. Table-hint forms and their numeric bounds.
6. Trailing comma in `CREATE TABLE`.

Done for each class when it has a failing case before the change, engine evidence, adjacent negatives, and passes
every gate within the cost budget.

## M2.5: Absorb grammar-kit's T-SQL work (ADR-0004)

- Port the tree-decidable checks of grammar-kit's `CheckTSQLContext` into `tools/tree-check`, one rule family per pull request, each with oracle evidence: hint conflicts and arguments, DML-target hints, column permissions, trigger name/owner/event scope, window frames, COUNT arity, RETURN in scalar functions, CTE without a table source, scalar versus predicate.
- Import grammar-kit's 640 route cases as an oracle probe set with fresh verdicts on every verified engine, and its T-SQL queries.
- Record the decisions in grammar-kit's reports where they apply.

Done when no T-SQL check or asset remains only in grammar-kit, and every ported rule agrees with the engine on its oracle cases.

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

## M3: Remaining surface

The remaining inventory cells: DDL families, `QUOTED_IDENTIFIER OFF` (stateful scanner), `GO`/sqlcmd as a client
suite, the syntax false positives that are syntactic.

Done when every declared cell is verified; cells with only an issue count as open.

## M4: Upstream sync and consumers

- Check `alfpricot/tree-sitter-mssql` every two to four weeks and bundle useful changes in one `upstream-sync` pull
  request.
- Consumers (go-treesitter) move their pins in separate, reviewed changes. Each move records the node-type diff, the
  native tree diff, the cost and the memory.

Done when the cadence has run once and each consumer move has its record.
