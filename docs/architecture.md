# tree-sitter-mssql architecture

`tree-sitter-mssql` is a tree-sitter grammar for Microsoft SQL Server T-SQL. The grammar is written in JavaScript, compiled by the tree-sitter CLI into a C parser that is committed to the repository, and wrapped by Rust and Node.js bindings so editors, linters, and formatters can parse T-SQL into a concrete syntax tree.

The main design choice is one grammar for every SQL Server version: it accepts the union of all versions' syntax and gives version-specific constructs their own node kinds, instead of switching behavior at run time. The grammar began as a fork of a general-SQL grammar; the rest of this document describes the parts that exist now. See [docs/scope.md](scope.md) for the scope rules.

## What tree-sitter-mssql is not

- Not a general or multi-dialect SQL grammar: only T-SQL.
- Not a SQL Server client, validator, or formatter: it produces syntax trees and nothing else.
- Not a semantic checker: it accepts some statements SQL Server rejects and rejects some it accepts, and [docs/accepted-gaps.md](accepted-gaps.md) records those differences.

## Major components

### [grammar.js](../grammar.js) and [grammar/](../grammar)

`grammar.js` is the grammar's entry point. The rules live in modules under `grammar/`: `expressions.js`, `types.js`, `keywords.js`, `column-lists.js`, `transactions.js`, `helpers.js`, and one file per statement family under [grammar/statements/](../grammar/statements) (`select.js`, `insert.js`, `create.js`, `alter.js`, `security.js`, and so on).

### [src/](../src)

The generated parser: `parser.c`, `grammar.json`, and `node-types.json` come from `tree-sitter generate` and are committed, so consumers build the parser without the CLI. `scanner.c` is hand-written. It lexes nested block comments, and it checks what follows a sqlcmd command name (whitespace, or an argument on the same line), neither of which a regular expression can express.

### [queries/](../queries)

Tree-sitter query files for editors: `highlights.scm`, `indents.scm`, `injections.scm`, `locals.scm`, and `tags.scm`. Both bindings export them.

### [bindings/](../bindings)

`bindings/rust` is the crate (`lib.rs`, with `build.rs` compiling the C sources). `bindings/node` is the Node.js addon (`binding.cc`, `index.js`, type definitions, and its tests), built through `binding.gyp`.

### [test/](../test)

`corpus/` holds per-construct expected trees, `fixtures/` holds larger T-SQL files that must parse without errors, and `highlight/` holds highlight-query tests. `queries/` holds recorded query captures, and `node-kinds.txt` is the generated list of node kinds that serves as the public API surface (maintained by `scripts/generate-node-kinds.mjs` and `scripts/check-registries.mjs`). `parser-cost.json` and `runtime-cost.json` are the cost baselines.

### [scripts/](../scripts)

Node scripts that check and maintain the repository, each with a `*.test.mjs` next to it: the oracle replay and its
session handling (`oracle-replay.mjs`, `verdict-diff.mjs`, `scriptdom-compare.mjs`), the metamorphic gate
(`comment-mutate.mjs`), cost gates (`parser-cost.mjs`, `runtime-cost.mjs`), query and identifier checks, the wasm
build and parity gate, and release checks (`release-policy.mjs versions`, `package-check.mjs`).

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

A `cargo-fuzz` target that parses arbitrary bytes through the grammar. CI fuzzes the parser with libFuzzer on grammar changes and weekly (`.github/workflows/fuzz.yml`); see [docs/development.md](development.md).

### [docs/](.)

Hand-written documents on scope, consuming the grammar, development, and the ledger of accepted gaps. `versions.json` at the root maps node kinds to minimum SQL Server versions and deprecation status.

## External systems and data flow

The repository talks to no database or service at run time. A consumer passes T-SQL text to the parser through one of the bindings and receives a syntax tree.

```text
T-SQL text -> binding (Rust crate or Node addon) -> src/parser.c + src/scanner.c -> syntax tree
```

Nothing is published to a registry. Consumers pin a commit of this repository and build the committed generated parser; see [docs/consuming.md](consuming.md).

## Build and test

- **Pull requests and pushes to `main`** run `Verify` (`.github/workflows/verify.yml`): PR-title validation, secret scanning, regeneration of the parser with a drift check, the node-kind and registry checks, lint and format checks, corpus, highlight, and fixture tests, `cargo fmt`, `clippy`, and `cargo test`, the Node binding build and tests, and workflow linting.
- **Oracle changes and weekly**, `Oracle engine` judges every case set on SQL Server 2017, 2019, 2022 and 2025 under both QUOTED_IDENTIFIER settings and compares with the committed verdicts.
- **Grammar changes and weekly**, `Fuzz` runs libFuzzer on the parser.
- **On a schedule**, `Cleanup stale caches` runs daily.
- Nothing is deployed or published: consumers pin commits.

## Where things live

The README's [Repository layout](../README.md#repository-layout) lists every root file and folder.
