# Roadmap

Phases in order. Each phase ends when its "done when" holds. The reasons are in
[ADR-0001](decisions/ADR-0001-continue-from-upstream-v1.md).

## M1: SQL Server oracle

- A runner that sends each case to SQL Server LocalDB under `SET PARSEONLY ON` and records the verdict: accepted, or
  the error numbers. Optionally the SQL Server Linux container in CI.
- Verdict files per compatibility level, each recording the engine version.
- Import of the 726 PARSEONLY cases from tree-sitter-grammar-kit (MIT), with attribution.
- A report with the confusion matrix (engine accepts or rejects against tree clean or `ERROR`), split into syntax and
  context errors.
- CI fails when agreement drops below the committed baseline.

Done when the runner, the verdict files and the report are committed, the 726 cases are imported with attribution,
and CI fails on an agreement drop.

## M2: Small fixes

One issue per defect class, in this order (counts from the v1.0.0 evaluation):

1. `CREATE/ALTER QUEUE … WITH ACTIVATION (…, EXECUTE AS …)` (51 cases).
2. Clauses without FROM: `WHERE`, `GROUP BY`, `HAVING`.
3. Table-hint forms: space-separated hints, `INDEX(0)`, `INDEX = 0`, `FORCESEEK(ix(a))`, `SPATIAL_WINDOW_MAX_CELLS`.
4. Unicode identifiers and variables (`@변수`, `#가`).
5. Leading and empty `;` (`;WITH …`, `SELECT 1;;`).
6. Trailing comma in `CREATE TABLE`.

Done when each class has an issue, a failing case that now passes, and no drop in oracle agreement.

## M3: DDL coverage

Statement families that do not parse yet, chosen by oracle evidence: `ALTER AVAILABILITY GROUP`, `ALTER SECURITY
POLICY`, `ALTER ASSEMBLY`, `CREATE/ALTER QUEUE` sub-forms, `BACKUP/RESTORE MASTER KEY` and others.

Done when every DDL family found in the oracle cases has corpus coverage or an issue explaining why not.

## M4: Upstream sync cadence

A recurring check of `alfpricot/tree-sitter-mssql` for new commits, merged through `upstream-sync` issues, with
conflicts against our fixes resolved in favour of SQL Server's verdict.

Done when the cadence is written down and one sync (or a recorded "nothing to merge") has followed it.
