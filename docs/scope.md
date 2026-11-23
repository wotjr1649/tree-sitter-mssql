# Scope

What this grammar covers, what it deliberately leaves out, and how it treats SQL Server versions.

**Scope: T-SQL only.** This is not a general or multi-dialect SQL grammar. PostgreSQL, MySQL, SQLite, Hive, Spark, and Snowflake syntax are out of scope; the rule set inherited from tree-sitter-sql's other dialects has been removed rather than maintained.

**SQL Server versions.** One grammar recognizes supported T-SQL forms across SQL Server versions and Azure SQL
without a runtime server-version switch. Its target range starts at SQL Server 2008, and the oracle records engine
verdicts for SQL Server 2017–2025. Removed forms such as `COMPUTE` are outside the supported syntax. Deprecated
syntax that still executes (`SETUSER`, `READTEXT`, the bare `(NOLOCK)` hint) is in scope, since a linter has to
see it to flag it. Version-specific syntax can reuse existing node kinds. For example, JSON aggregates use invocation
nodes. A downstream rule enforcing a target version must check the construct in context. Corpus and oracle cases
record engine profiles where measured.

**Status.** The grammar covers procedure, function, and trigger bodies (DML, DDL and logon triggers), control flow
(`IF`/`ELSE`, `WHILE`, `TRY`/`CATCH`, `RETURN`, `GOTO`), transactions, variables and table variables, cursors,
`MERGE`, `OUTPUT`, `CROSS`/`OUTER APPLY`, `PIVOT`/`UNPIVOT`, `FOR XML`/`JSON`, `OFFSET`/`FETCH`,
`RAISERROR`/`THROW`, table and query hints, `TOP`, `CREATE`, `ALTER` and `DROP` forms for tables, constraints,
indexes, views, types, sequences, synonyms, schemas, users, logins and roles, `GRANT`/`REVOKE`/`DENY`, the SSMS
script header and `GO`, SMO-scripted objects with no statement separators, and bracketed parameterized types.
Coverage varies by form. For example, schema elements in a compound `CREATE SCHEMA` currently appear as separate
statements in the tree. A structural linter and a code-metrics tool both parse T-SQL with this grammar, so node
kinds and field names are a public contract. See [consuming.md](consuming.md).

**Known mismatches.** [accepted-gaps.md](accepted-gaps.md) records current classifications and earlier grammar
mismatches.
The current oracle classifications are in [`oracle/data/rejected-clean.json`](../oracle/data/rejected-clean.json).

## Compatibility levels

SQL Server's parser result can depend on the database compatibility level. SQL Server 2017 to 2025 all support level
100 (the SQL Server 2008 level) as their lowest level, and SQL Server 2025 supports 100 to 170
([ALTER DATABASE compatibility level](https://learn.microsoft.com/en-us/sql/t-sql/statements/alter-database-transact-sql-compatibility-level)).
The oracle records every case at every level the engine supports: level 100 is present in all 174 committed verdict
files (SQL Server 2017, 2019, 2022 and 2025 on Linux, and LocalDB 2025), 63,652 case verdicts in total
(measured at 2.8.0).

The grammar and tree-check take the union of the levels: a case is positive when any recorded engine accepts it at
any level under the same QUOTED_IDENTIFIER setting. Neither has a compatibility-level option, so a form that only
some levels accept parses cleanly at every level. At 2.8.0, 95 oracle cases change verdict with the level:

| Accepted from | Cases | Forms |
| --- | ---: | --- |
| 110 | 30 | `WITHIN GROUP` ordered-set aggregates (`PERCENTILE_CONT`/`DISC`, `APPROX_PERCENTILE_*`, `STRING_AGG`, `JSON_ARRAYAGG`), `JSON_VALUE … WITHIN GROUP (GRAPH PATH)`, `TRY_CONVERT`, graph `MATCH`/`SHORTEST_PATH` |
| 130 | 16 | `OPENJSON`, including its `WITH` schema |
| 160 | 12 | the `WINDOW` clause and named windows, `TRIM(LEADING \| TRAILING \| BOTH ... FROM ...)` |
| 170 | 25 | `REGEXP_*` functions, `AI_GENERATE_*` |
| 100–120 only | 12 | encryption keys with deprecated algorithms (`TRIPLE_DES_3KEY`, `RC2`, `RC4`; Msg 102 from level 130; one route already from 110); `STRING_SPLIT` call shapes that SQL Server 2017 rejects from level 130 (Msg 8144); at level 100 only, `TRY_CONVERT` and the three `SEMANTIC*TABLE` names as unquoted names |

A tool that targets one compatibility level has to check these constructs itself. The per-case verdicts are in
`oracle/data/verdicts`.

## Origin

This repository continues `alfpricot/tree-sitter-mssql` v1.0.0, which began as a fork of
[`tree-sitter-sql`](https://github.com/DerekStride/tree-sitter-sql) by Derek Stride. The general-SQL core from
that project provided expression precedence, `SELECT`, CTEs, joins, and window functions. This repository
maintains the T-SQL dialect. See [LICENSE](../LICENSE) and [NOTICE](../NOTICE) for the MIT license and
attribution.
