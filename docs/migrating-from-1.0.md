# Moving from 1.0.0 to 2.0.0

2.0.0 is the first release of this fork. 1.0.0 is upstream
[`alfpricot/tree-sitter-mssql`](https://github.com/alfpricot/tree-sitter-mssql) v1.0.0; this fork started at
its release commit `852cbd2`. Measured against it, `src/node-types.json` changes as follows:

| | count |
| --- | ---: |
| node kinds removed | 0 |
| node kinds added | 244 |
| fields removed | 0 |
| fields added | 87 |
| types a field accepts, removed | 0 |
| types a node lists among its unnamed children, removed | 0 |

## The breaking change

A parenthesized condition of `IF`, `WHILE`, `MERGE ... ON` and `WHEN [NOT] MATCHED ... AND` is now one
`parenthesized_expression` in the `condition` or `predicate` field. Before, the statement owned the parentheses and the
field held their content.

```
IF (a = 1) PRINT 1;
```

| | `condition` field |
| --- | --- |
| 1.0.0 | `(binary_expression ...)`; `(` and `)` are children of `if_statement` |
| 2.0.0 | `(parenthesized_expression (binary_expression ...))` |

A query that matches `condition: (binary_expression)` directly must also accept
`condition: (parenthesized_expression (binary_expression))`. A condition without parentheses keeps its 1.0.0 shape.
The change removed a duplicated copy of the expression grammar: `parser.c` -11 %, large parse states -15 % (#32).

## Coming from a commit before upstream 1.0.0

A consumer pinned to an earlier upstream commit, such as `2e34d86e`, also meets a change that upstream 1.0.0 made.
Seven node kinds list fewer types among their unnamed children. The nodes are still there; they are now under a field:

| node kind | child | now in field |
| --- | --- | --- |
| `create_function`, `alter_function` | `function_body` | `body` |
| `create_procedure`, `alter_procedure` | `procedure_body` | `body` |
| `create_trigger`, `alter_trigger` | `object_reference` | `name`, `target` |
| `cte` | `identifier` | `name` |

Child iteration is unchanged. `ChildByFieldName` now finds them.

## What else changes for a consumer

- **Every input SQL Server accepts parses without an error.** The oracle holds 10,300 cases judged by SQL Server
  2017, 2019, 2022 and 2025 at compatibility levels 100–170; all 7,504 accepted cases parse cleanly. Inputs that
  1.0.0 parsed with an `ERROR` may now parse cleanly, and their trees are new.
- **Error recovery of invalid input.** Trees of inputs SQL Server rejects may differ inside their `ERROR` nodes. They
  were never a contract.
- **Validation is a separate layer.** The grammar alone rejects 30 % of the inputs SQL Server rejects. The constraint
  layer in `tools/tree-check` rejects the rest that the tree can decide. A CGO-free host can use it as a WebAssembly
  module ([constraint-wasm.md](constraint-wasm.md)). What remains clean is classified in
  `oracle/rejected-clean.json`: context, environment, version and runner cases only, with no open grammar gap.
- **Highlights.** Every `keyword_*` kind has a highlight.
- **Size.** The compiled library is about 1.39x the M2 milestone (gcc -O2), and parse time is 0.93x of it
  (ADR-0005).
