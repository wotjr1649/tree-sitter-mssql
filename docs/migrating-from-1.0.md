# Moving from 1.0.0 to 2.0.0

2.0.0 is the first release of this fork. 1.0.0 is upstream
[`alfpricot/tree-sitter-mssql`](https://github.com/alfpricot/tree-sitter-mssql) v1.0.0; this fork started at
its release commit `852cbd2`. Measured against it, `src/node-types.json` changes as follows:

| | count |
| --- | ---: |
| node kinds removed | 0 |
| node kinds added | 244 |
| node/field pairs removed | 0 |
| node/field pairs added (including new node kinds) | 87 |
| types a field accepts, removed | 0 |
| types a node lists among its unfielded children, removed | 0 |

## Trees that change for valid input

This is why the version is 2.0.0. The release audit parsed every input that SQL Server accepts in the oracle, every
fixture and every corpus input of 1.0.0 with both parsers. Of the 5,278 distinct inputs 1.0.0 parsed without an
error, 5,236 keep their tree exactly and **42 change**. Most of them 1.0.0 had read wrongly (`8 << 2` without a shift operator,
`KILL 12 ROLLBACK` as two statements, `WITH (NOLOCK NOWAIT)` as an OPENJSON schema). A query written against the
old tree of these forms stops matching:

| form (example) | 1.0.0 | 2.0.0 |
| --- | --- | --- |
| `ALTER DATABASE d MODIFY FILE (SIZE = 8 MB)` | `option` named FILE holding the options | `keyword_file` + `database_file` holding them |
| `FROM t WITH (NOLOCK NOWAIT)` | `openjson_schema` | `table_hint` with `table_hint_item`s |
| `CREATE TABLE t (timestamp)` | column `name: identifier`, `custom_type` | no name, `type: keyword_timestamp` |
| `ALTER QUEUE q WITH ACTIVATION (DROP)` | nested `option` | `keyword_drop` |
| `c int INDEX ix NONCLUSTERED` in a column | three `identifier`s | `keyword_index`, `name:`, `keyword_nonclustered` |
| `KILL 12 ROLLBACK` | `kill_statement` + `rollback_statement` | one `kill_statement` |
| `ALTER COLUMN c ADD PERSISTED` | `identifier` | `keyword_persisted` |
| `COLUMN_SET FOR ALL_SPARSE_COLUMNS` | `identifier`s | keywords |
| `8 << 2 + 1` | no shift operator; a `field` among the operands | shift operator, nested by precedence |
| `UPDATE t SET c = DEFAULT` | `right: field` | `right: keyword_default` |
| `UPDATE t SET @n = c += 1` | `right: binary_expression` | `right: assignment` |
| `GENERATED ALWAYS AS ROW START HIDDEN` | `identifier`s | `generated_always` |
| `ALTER QUEUE q REORGANIZE WITH (...)` | `with_clause` with a nested `option` | `with_options` |
| `CREATE TABLE ... AS NODE ON fg` | second `table_data_placement` | `on_filegroup` |
| `PARSE` / `TRY_PARSE(x AS int)` | `invocation`, type as an alias | `cast` with `name: keyword_try_parse` |
| `CREATE PROCEDURE p AS ; WITH c AS (...) SELECT ...` | CTE outside the procedure body | inside `procedure_body` |
| `DECLARE @c AS CURSOR` | `custom_type` | `keyword_cursor` |

No input 1.0.0 parsed cleanly gets an error now, and no other node covers a different span.

## Condition trees

A parenthesized condition of `IF`, `WHILE`, `MERGE ... ON` and `WHEN [NOT] MATCHED ... AND` is a
`parenthesized_expression` in the `condition` or `predicate` field. For example:

```sql
IF (a = 1) PRINT 1;
```

Both 1.0.0 and 2.0.0 give `condition: (parenthesized_expression (binary_expression ...))`.
The release audit compared parenthesized and unparenthesized examples for IF, WHILE, MERGE ON and each of the
three MERGE match clauses; their trees agree. Queries need no change for these examples.

The refactor in #32 removed redundant statement-level parentheses from the grammar. Against its preceding
commit, it reduced `parser.c` by 11 %, parse states by 8.5 % and large parse states by 15 %; those figures
describe the grammar refactor, not a demonstrated tree-shape change from 1.0.0.

## Coming from a commit before upstream 1.0.0

A consumer pinned to an earlier upstream commit, such as `2e34d86e`, also meets a change that upstream 1.0.0 made.
Seven node kinds list fewer types among their unfielded children. The nodes are still there; they are now under a field:

| node kind | child | now in field |
| --- | --- | --- |
| `create_function`, `alter_function` | `function_body` | `body` |
| `create_procedure`, `alter_procedure` | `procedure_body` | `body` |
| `create_trigger`, `alter_trigger` | `object_reference` | `name`, `target` |
| `cte` | `identifier` | `name` |

Child iteration is unchanged. `ChildByFieldName` now finds them.

## What else changes for a consumer

- **Every input SQL Server accepts parses without an error.** The oracle holds 10,300 cases judged by SQL Server
  2017, 2019, 2022 and 2025 at compatibility levels 100–170; acceptance at any recorded engine/level counts.
  All 7,504 accepted cases parse cleanly. One runner-controlled input was not submitted to the engines. Inputs that
  1.0.0 parsed with an `ERROR` may now parse cleanly, and their trees are new.
- **Incremental reparsing.** Use a tree-sitter runtime that contains upstream commit `f932671a5` (released after
  0.27.2). Older runtimes can give a reparse that differs from a fresh parse when a reused node starts with a keyword
  that must be read as a name; this is a runtime defect, the same for 1.0.0, and no grammar can avoid it.
- **Error recovery of invalid input.** Trees of inputs SQL Server rejects may differ inside their `ERROR` nodes. They
  were never a contract.
- **Validation is a separate layer.** The grammar alone rejects 852 of the 2,795 engine-rejected inputs (30.48 %).
  The constraint layer in `tools/tree-check` rejects 1,755 more. A CGO-free host can use it as a WebAssembly
  module ([constraint-wasm.md](constraint-wasm.md)). What remains clean is classified in
  `oracle/rejected-clean.json`: 189 context, environment, version and runner cases, including the one runner exclusion,
  with no oracle case classified as an open grammar gap.
- **Highlights.** Every `keyword_*` kind has a highlight.
- **Size and time.** In the release audit on Windows x64 with gcc 16.2.0 `-O2`, the compiled library was 1.386x
  the M2 milestone (`e8a398e`), and median parse time over five alternating runs was 0.949x. Against the current
  milestone (`4ff4c5f`), the ratios were 1.000x and 0.952x. Timing varies by host and run; peak memory was not
  measured on Windows (ADR-0005).
