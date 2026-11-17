# Divergence from the original grammar (1.0.0)

The places where this repository deliberately differs from `alfpricot/tree-sitter-mssql` (ADR-0001, ADR-0002 §9). They are a record for readers coming from 1.0.0; changes of that repository are not merged (ADR-0007).

| 1.0.0 state | Here | Evidence | Change |
| --- | --- | --- | --- |
| `test/corpus/errors.txt` pinned `SELECT 1 WHERE 1=1;` and `SELECT 1 GROUP BY 1;` as errors ("not legal T-SQL"); `grammar/statements/select.js` nested WHERE/GROUP BY/HAVING under FROM | The clauses are independent of FROM; both inputs are positive cases in `test/corpus/select-fromless.txt` | SQL Server 2025 LocalDB 17.0.1000.7, levels 100–170: accepted; oracle cases and `test/corpus/select-fromless.txt` | M2 class 2 (FROM-less clauses) |
| `IDENTIFIER_START`/`IDENTIFIER_CONTINUE` covered ASCII and selected Latin letters through U+017F | Classes generated from the engine-measured sets in `oracle/data/unicode/identifier-chars.json` | Every BMP code point except C0 controls and surrogates probed at levels 100 and 170 (126,912 start/continue probes) | M2 Unicode identifiers |
| `CREATE/ALTER QUEUE` used the generic `with_clause` | Queue-specific option rules for ACTIVATION, plus constraint rules in `crates/tree-check` (ADR-0003) | `oracle/data/probes/queue-activation.jsonl` and the queue oracle cases | M2 class 1 (QUEUE activation) |
