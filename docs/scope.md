# Scope

What this grammar covers, what it deliberately leaves out, and how it treats SQL Server versions.

**Scope: T-SQL only.** This is not a general or multi-dialect SQL grammar. PostgreSQL, MySQL, SQLite, Hive, Spark, and Snowflake syntax are out of scope; the rule set inherited from tree-sitter-sql's other dialects has been removed rather than maintained.

**SQL Server versions.** One grammar accepts the union of every version's syntax, from what SQL Server 2008 accepted through the current release and Azure SQL. A tree-sitter parser has no runtime switch, and a consumer meets a file on disk without knowing the server it targets, so the grammar never refuses a construct on version grounds. Deprecated syntax that still executes (`SETUSER`, `READTEXT`, the bare `(NOLOCK)` hint) is in scope, since a linter has to see it to flag it. Version-specific syntax may reuse existing node kinds (for example, JSON aggregates use invocation nodes); a downstream rule must inspect the construct against its target version. Corpus and oracle cases record engine profiles where measured.

**Status.** Procedure, function, and trigger bodies (DML, DDL and logon triggers), control flow (`IF`/`ELSE`, `WHILE`, `TRY`/`CATCH`, `RETURN`, `GOTO`), transactions, variables and table variables, cursors, `MERGE`, `OUTPUT`, `CROSS`/`OUTER APPLY`, `PIVOT`/`UNPIVOT`, `FOR XML`/`JSON`, `OFFSET`/`FETCH`, `RAISERROR`/`THROW`, table and query hints, `TOP`, the full `CREATE`/`ALTER`/`DROP` surface for tables, constraints, indexes, views, types, sequences, synonyms, schemas, users, logins and roles, `GRANT`/`REVOKE`/`DENY`, the SSMS script header and `GO`, SMO-scripted objects with no statement separators, and bracketed parameterized types all parse. Downstream, a structural linter and a code-metrics tool both parse T-SQL with it, so node kinds and field names are a public contract (see [consuming.md](consuming.md)).

**Known mismatches.** [accepted-gaps.md](accepted-gaps.md) lists the places where this grammar and SQL Server disagree about what is valid. [`test/fixtures/README.md`](../test/fixtures/README.md) lists the constructs deliberately left unmodeled.

## Origin

This project started as a fork of [`tree-sitter-sql`](https://github.com/DerekStride/tree-sitter-sql) by Derek Stride (MIT licensed; see `LICENSE` and `NOTICE`). The general-SQL core — expression precedence, `SELECT`, CTEs, joins, window functions — comes from that project largely unchanged. The T-SQL-specific surface is added on top, and tree-sitter-sql's other-dialect surface is gone.
