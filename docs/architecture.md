# tree-sitter-mssql architecture

`tree-sitter-mssql` is a tree-sitter grammar for Microsoft SQL Server T-SQL. The grammar is written in JavaScript, compiled by the tree-sitter CLI into a C parser that is committed to the repository, and wrapped by Rust and Node.js bindings so editors, linters, and formatters can parse T-SQL into a concrete syntax tree.

One grammar handles supported forms across SQL Server versions without a runtime server-version switch. Some
constructs have dedicated node kinds, and others share kinds such as `invocation`. See [docs/scope.md](scope.md)
for the grammar's origins, coverage and limits.

## What tree-sitter-mssql is not

- Not a general or multi-dialect SQL grammar: only T-SQL.
- Not a SQL Server client, validator, or formatter: it produces syntax trees and nothing else.
- Not a semantic checker: it accepts some statements SQL Server rejects and rejects some it accepts, and [docs/accepted-gaps.md](accepted-gaps.md) records those differences.

## Major components

### [grammar.js](../grammar.js) and [grammar/](../grammar)

`grammar.js` is the grammar's entry point. The rules live in modules under `grammar/`: `expressions.js`, `types.js`, `keywords.js`, `column-lists.js`, `transactions.js`, `helpers.js`, and one file per statement family under [grammar/statements/](../grammar/statements) (`select.js`, `insert.js`, `create.js`, `alter.js`, `security.js`, and so on).

### [src/](../src)

Tree-sitter generates `parser.c`, `grammar.json`, `node-types.json` and the headers under `tree_sitter/`. These
files are committed so consumers can build without the CLI. The hand-written `scanner.c` handles nested block
comments, sqlcmd boundaries and external tokens for constructs such as spaced comparison operators and QUEUE options.

### [queries/](../queries)

Tree-sitter query files for editors: `highlights.scm`, `indents.scm`, `injections.scm`, `locals.scm`, and `tags.scm`. Both bindings export them.

### [bindings/](../bindings)

`bindings/rust` is the crate (`lib.rs`, with `build.rs` compiling the C sources). `bindings/node` is the Node.js addon (`binding.cc`, `index.js`, type definitions, and its tests), built through `binding.gyp`.

### [test/](../test)

`corpus/` holds per-construct expected trees, `fixtures/` holds larger T-SQL files that must parse without errors,
and `highlight/` holds highlight-query tests. `queries/` holds recorded query captures. `node-kinds.txt` is the
snapshot of named node kinds and their fields produced by `scripts/generate-node-kinds.mjs`.
`scripts/check-registries.mjs` checks rule registration, version metadata references and keyword highlighting.
Pull requests also check the broader `src/node-types.json` schema for compatibility. `parser-cost.json` and
`runtime-cost.json` are the cost baselines.

### [scripts/](../scripts)

Node scripts that check and maintain the repository, many with a `*.test.mjs` next to them: the oracle replay and its
session handling (`oracle-replay.mjs`, `verdict-diff.mjs`, `scriptdom-compare.mjs`), the metamorphic gate
(`comment-mutate.mjs`), cost gates (`parser-cost.mjs`, `runtime-cost.mjs`), query and identifier checks, the wasm
build and parity gate, node-type compatibility (`check-node-types.mjs`), and release and packaging checks
(`release-policy.mjs versions`, `package-check.mjs`).

### [crates/tree-check/](../crates/tree-check)

The constraint layer (ADR-0003): a Rust library and CLI that parses T-SQL with this grammar and reports parse problems
and constraint violations, also built as a WebAssembly reactor for CGO-free hosts
([docs/constraint-wasm.md](constraint-wasm.md)). It is the fixture gate and the checker the oracle replay runs.

### [oracle/](../oracle)

The SQL Server oracle (ADR-0002, [oracle/README.md](../oracle/README.md)). `data/` holds what is recorded: case sets
(`kit/`, `probes/`), engine verdicts (`verdicts/`), ScriptDOM verdicts (`scriptdom-verdicts/`), the grammar
baseline (`expected.json`) and the classified rejected-but-clean cases (`rejected-clean.json`). `runners/` holds
the two .NET programs that produce verdicts: `ParseOnly` (SQL Server, `SET PARSEONLY ON`) and `ScriptDomCheck`.

### [fuzz/](../fuzz)

The local Rust `parse` target parses fuzz inputs that decode as UTF-8. The `tree_check` target exercises the
checker and edit-history APIs. See [docs/development.md](development.md) for the local commands.

### [docs/](.)

Hand-written documents on scope, consuming the grammar, development, and the ledger of accepted gaps. `versions.json` at the root maps node kinds to minimum SQL Server versions and deprecation status.

## External systems and data flow

The repository talks to no database or service at run time. A consumer passes T-SQL text to the parser through one of the bindings and receives a syntax tree.

```text
T-SQL text -> binding (Rust crate or Node addon) -> src/parser.c + src/scanner.c -> syntax tree
```

Nothing is published to a registry. Consumers pin a tag or commit of this repository and build the committed
generated parser. See [docs/consuming.md](consuming.md).

## Build and test

- **Ready pull requests and pushes to `main`** run the full [Verify workflow](../.github/workflows/verify.yml).
  The suite covers CLI and ABI pins, parser regeneration and drift, node kinds and registries, lint and format,
  corpus, highlight and fixture tests, oracle replay, mutation and WASM parity, parser costs, Rust and Node
  builds and tests, workflow linting and secret scanning. On pull requests, Verify also checks the title and
  compares node types with the PR's merge target.
- **Oracle changes and weekly**, `Oracle engine` judges every case set on SQL Server 2017, 2019, 2022 and 2025 under both QUOTED_IDENTIFIER settings and compares with the committed verdicts.
- **Relevant pull requests, weekly and on demand**, [Fuzz parser](../.github/workflows/fuzz.yml) runs libFuzzer
  on the parser and scanner and separately fuzzes the tree-check APIs.
- **On a schedule**, `Cleanup stale caches` runs daily.
- Releases are tagged and documented on GitHub. Consumers pin a tag or commit.

## Where things live

The README's [Repository layout](../README.md#repository-layout) maps the tracked root files and directories,
including the parser ABI pin under `ci/`.
