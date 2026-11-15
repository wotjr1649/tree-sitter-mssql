# Divergence from upstream

The places where this repository deliberately differs from `alfpricot/tree-sitter-mssql` (ADR-0001, ADR-0002 §9). When an upstream change is merged, conflicts in these places are resolved in favour of SQL Server's verdict.

| Upstream state | Here | Evidence | Change |
| --- | --- | --- | --- |
| `test/corpus/errors.txt` pinned `SELECT 1 WHERE 1=1;` and `SELECT 1 GROUP BY 1;` as errors ("not legal T-SQL"); `grammar/statements/select.js` nested WHERE/GROUP BY/HAVING under FROM | The clauses are independent of FROM; both inputs are positive cases in `test/corpus/select-fromless.txt` | SQL Server 2025 LocalDB 17.0.1000.7, levels 100–170: accepted; oracle cases and `test/corpus/select-fromless.txt` | M2 class 2 (FROM-less clauses) |
| `IDENTIFIER_START`/`IDENTIFIER_CONTINUE` covered ASCII and Latin-1 letters | Classes generated from the engine-measured sets in `oracle/unicode/identifier-chars.json` | Every BMP code point probed at levels 100 and 170 | M2 Unicode identifiers |
| `CREATE/ALTER QUEUE` used the generic `with_clause` | Queue-specific option rules for ACTIVATION, plus constraint rules in `tools/tree-check` (ADR-0003) | `oracle/probes/queue-activation.jsonl` and the queue oracle cases | M2 class 1 (QUEUE activation) |
