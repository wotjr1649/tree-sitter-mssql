# Moving from 1.0.0 to 2.0.0

2.0.0 is the first release of this repository. 1.0.0 is the original grammar
[`alfpricot/tree-sitter-mssql`](https://github.com/alfpricot/tree-sitter-mssql) v1.0.0; this repository started at
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

This is why the version is 2.0.0. Re-measured at `825c964` with CLI/runtime 0.27.2, the comparison uses the
7,838 QUOTED_IDENTIFIER ON engine-accepted oracle inputs, all 156 fixtures at that commit and all 370 corpus
inputs of 1.0.0. Deduplicating by exact UTF-8 source bytes gives 8,103 inputs. Of the 5,379 that 1.0.0 parsed
without an error, 5,339 keep their complete tree (including fields and ranges) and **40 change**. Most of them 1.0.0 had read wrongly (`KILL 12 ROLLBACK` as two statements,
`WITH (NOLOCK NOWAIT)` as an OPENJSON schema). A query written against the
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
| `UPDATE t SET c = DEFAULT` | `right: field` | `right: keyword_default` |
| `UPDATE t SET @n = c += 1` | `right: binary_expression` | `right: assignment` |
| `GENERATED ALWAYS AS ROW START HIDDEN` | `identifier`s | `generated_always` |
| `ALTER QUEUE q REORGANIZE WITH (...)` | `with_clause` with a nested `option` | `with_options` |
| `CREATE TABLE ... AS NODE ON fg` | second `table_data_placement` | `on_filegroup` |
| `PARSE(x AS int)` / `TRY_PARSE(x AS int)` | `invocation`, type as an alias | `cast` with `name: keyword_parse` for `PARSE` and `name: keyword_try_parse` for `TRY_PARSE` |
| `CREATE PROCEDURE p AS ; WITH c AS (...) SELECT ...` | CTE outside the procedure body | inside `procedure_body` |
| `DECLARE @c AS CURSOR` | `custom_type` | `keyword_cursor` |
| `IF a IF b s ELSE t` (dangling ELSE) | `else_clause` on the outer `if_statement` | on the nearest `if_statement`, as SQL Server executes it |

In this comparison, no input 1.0.0 parsed cleanly gets an error now, and no other node covers a different span.

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

## Coming from a commit before 1.0.0

A consumer pinned to an earlier commit of the original grammar, such as `2e34d86e`, also meets a change that 1.0.0 made.
Seven node kinds list fewer types among their unfielded children. The nodes are still there; they are now under a field:

| node kind | child | now in field |
| --- | --- | --- |
| `create_function`, `alter_function` | `function_body` | `body` |
| `create_procedure`, `alter_procedure` | `procedure_body` | `body` |
| `create_trigger`, `alter_trigger` | `object_reference` | `name`, `target` |
| `cte` | `identifier` | `name` |

Child iteration is unchanged. `ChildByFieldName` now finds them.

## What else changes for a consumer

- **Every engine-accepted oracle input passes tree-check.** The oracle holds 10,709 sources (21,418 ON/OFF
  session/case pairs), with recorded SQL Server 2017, 2019, 2022 and 2025 verdicts at supported compatibility
  levels 100–170; acceptance at any recorded engine/level counts. All 7,838 ON and 7,747 OFF accepted cases
  pass, including session-aware re-parsing under OFF (the grammar alone misses three accepted OFF cases). LocalDB 2025 covers all sources in both sessions; Linux results cover earlier ON snapshots. One runner-controlled input was not submitted to the engines. Inputs that
  1.0.0 parsed with an `ERROR` may now parse cleanly, and their trees are new.
- **Incremental reparsing.** Use a tree-sitter runtime containing commit `f932671a5` for the reuse fix. Earlier
  runtimes can reuse the wrong first leaf when a keyword must be read as a name. This also affects 1.0.0.
  A fresh parse avoids that reuse defect.
- **Error recovery of invalid input.** Trees of inputs SQL Server rejects may differ inside their `ERROR` nodes. They
  were never a contract. The same holds for an incremental reparse after an edit that leaves invalid text: it can
  place the `ERROR` differently from a fresh parse of that text.
- **Validation is a separate layer.** Under QUOTED_IDENTIFIER ON, the grammar alone rejects 876 of the
  2,870 engine-rejected inputs (30.52 %); the constraint layer in `crates/tree-check` rejects 1,805 more. A CGO-free host can use it as a WebAssembly
  module ([constraint-wasm.md](constraint-wasm.md)). What remains clean is classified in
  `oracle/data/rejected-clean.json`: 445 session/case pairs (ON 190, OFF 255), including the runner exclusion
  in each session; context 111, environment 294, version 36 and runner 4, with no open grammar gap.
- **Highlights.** Every `keyword_*` kind has a highlight.
- **Size and time.** In the release audit on Windows x64 with gcc 16.2.0 `-O2`, the compiled library was 1.386x
  the M2 milestone (`e8a398e`), and median parse time over five alternating runs was 0.949x. Against the then-current
  milestone (`4ff4c5f`), the ratios were 1.000x and 0.952x. Timing varies by host and run; peak memory was not
  measured on Windows (ADR-0005). The direct comparison with 1.0.0 in [benchmarks.md](benchmarks.md) uses the
  2.0.0 release candidate at `d206c0a` (#62).
