# tree-sitter-mssql

[![Verify](https://github.com/wotjr1649/tree-sitter-mssql/actions/workflows/verify.yml/badge.svg?branch=main)](https://github.com/wotjr1649/tree-sitter-mssql/actions/workflows/verify.yml)

An owner-maintained [tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammar for Microsoft SQL Server Transact-SQL (T-SQL). It covers T-SQL only, and one grammar accepts the union of every SQL Server version's syntax, so a consumer never needs to know the server version a file targets. [docs/scope.md](docs/scope.md) holds the scope rules, version handling, and the list of covered syntax.

**Why it exists.** T-SQL has no well-maintained tree-sitter grammar. The only candidate, [`alfpricot/tree-sitter-mssql`](https://github.com/alfpricot/tree-sitter-mssql) (MIT), has one maintainer, changed its owner name twice in two months (`meloncholic` → `meloncholera` → `alfpricot`) and rewrote its Git history, which orphaned the commit a downstream consumer pinned. This repository continues that grammar so defects get fixed quickly and pinned commits stay reachable.

**Upstream relation.** This repository starts from upstream v1.0.0, commit [`852cbd2`](https://github.com/alfpricot/tree-sitter-mssql/commit/852cbd20d7dc8078bdea0ebbcbb621e78c86f0cf), merged with its full history on 2026-11-15. Upstream started as a fork of [`tree-sitter-sql`](https://github.com/DerekStride/tree-sitter-sql) by Derek Stride. Upstream changes are merged when useful; see [docs/decisions/ADR-0001-continue-from-upstream-v1.md](docs/decisions/ADR-0001-continue-from-upstream-v1.md) and [NOTICE](NOTICE).

**Consumer.** The first consumer is `go-treesitter`, a CGO-free Go tree-sitter library, which pins a commit of this repository and regenerates `src/parser.c` with tree-sitter CLI 0.27.2.

**Correctness reference.** Valid T-SQL means what SQL Server's own parser accepts under `SET PARSEONLY ON` at a stated compatibility level. Where upstream tests or [docs/accepted-gaps.md](docs/accepted-gaps.md) disagree with SQL Server, SQL Server wins. [docs/roadmap.md](docs/roadmap.md) lists the planned work, starting with a SQL Server oracle.

## Using it

Nothing is published to crates.io, npm or any other registry. Pin a commit of this repository: the generated parser (`src/parser.c`, `src/grammar.json`, `src/node-types.json`) is committed, and `src/scanner.c` must be compiled alongside `src/parser.c`.

For Rust, as a Git dependency pinned to a commit:

```toml
[dependencies]
tree-sitter-mssql = { git = "https://github.com/wotjr1649/tree-sitter-mssql", rev = "<commit>" }
```

### Requirements

- Rust 1.85 or newer (the crate uses edition 2024).
- For Node.js, the `tree-sitter` package `^0.25.0` is an optional peer dependency, and building the addon needs a C toolchain.

## Usage

```rust
let mut parser = tree_sitter::Parser::new();
let language = tree_sitter_mssql::LANGUAGE;
parser.set_language(&language.into()).expect("Error loading T-SQL parser");
let tree = parser.parse("SELECT TOP (10) name FROM sys.objects WHERE type = 'U';", None).unwrap();
assert!(!tree.root_node().has_error());
```

```js
import Parser from "tree-sitter";
import SQL from "tree-sitter-mssql";

const parser = new Parser();
parser.setLanguage(SQL);
```

[docs/consuming.md](docs/consuming.md) covers the exported symbol, the parser ABI, the query-file constants, node-kind stability, and `versions.json`.

## How to contribute

Fix a defect starting from a failing corpus case or SQL Server oracle case, regenerate the parser with tree-sitter CLI 0.27.2 after any grammar change, and run the checks described in [CONTRIBUTING.md](CONTRIBUTING.md). [AGENTS.md](AGENTS.md) holds the standing rules, [docs/development.md](docs/development.md) has the build and test details, and [ARCHITECTURE.md](ARCHITECTURE.md) maps the repository.

## License

MIT — see [LICENSE](LICENSE) and [NOTICE](NOTICE).
