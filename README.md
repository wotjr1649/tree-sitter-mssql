# tree-sitter-mssql

[![Verify](https://github.com/wotjr1649/tree-sitter-mssql/actions/workflows/verify.yml/badge.svg?branch=main)](https://github.com/wotjr1649/tree-sitter-mssql/actions/workflows/verify.yml)

An owner-maintained [tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammar for Microsoft SQL Server Transact-SQL (T-SQL). It covers T-SQL only, and one grammar accepts the union of every SQL Server version's syntax, so a consumer never needs to know the server version a file targets. [docs/scope.md](docs/scope.md) holds the scope rules, version handling, and the list of covered syntax.

**Why it exists.** T-SQL has no well-maintained tree-sitter grammar. The only candidate, [`alfpricot/tree-sitter-mssql`](https://github.com/alfpricot/tree-sitter-mssql) (MIT), has one maintainer, changed its owner name twice in two months (`meloncholic` → `meloncholera` → `alfpricot`) and rewrote its Git history, which orphaned the commit a downstream consumer pinned. This repository continues that grammar so defects get fixed quickly and pinned commits stay reachable.

**Origin.** This repository is independent (ADR-0007). It started from the original grammar's v1.0.0, commit [`852cbd2`](https://github.com/alfpricot/tree-sitter-mssql/commit/852cbd20d7dc8078bdea0ebbcbb621e78c86f0cf), merged with its full history on 2026-11-15; that grammar started as a fork of [`tree-sitter-sql`](https://github.com/DerekStride/tree-sitter-sql) by Derek Stride. Its later changes are not merged: defects are fixed here. See [ADR-0001](docs/decisions/ADR-0001-continue-from-upstream-v1.md), [ADR-0007](docs/decisions/ADR-0007-independent-repository.md) and [NOTICE](NOTICE).

**Consumer.** The intended first consumer is `go-treesitter`, a CGO-free Go tree-sitter library, which is to pin a commit of this repository and regenerate `src/parser.c` with tree-sitter CLI 0.27.2.

**Correctness reference.** Valid T-SQL means what SQL Server's own parser accepts under `SET PARSEONLY ON` at a stated compatibility level. Where tests inherited from 1.0.0 or [docs/accepted-gaps.md](docs/accepted-gaps.md) disagree with SQL Server, SQL Server wins. [docs/roadmap.md](docs/roadmap.md) records the milestones.

## What it guarantees

SQL Server's own parser is the reference (`SET PARSEONLY ON`, ADR-0002). The oracle in `oracle/data` holds 10,709 sources (21,418 QUOTED_IDENTIFIER session/case pairs), with recorded
SQL Server 2017, 2019, 2022 and 2025 results at supported compatibility levels 100–170. LocalDB 2025 covers every
source under both settings; the Linux engine observations cover earlier case-set snapshots under ON. Acceptance means
acceptance by at least one recorded engine/level; the runner excludes one input that sets PARSEONLY itself.
From 2.0.0:

- every case SQL Server accepts parses without an error;
- every case SQL Server rejects is rejected by the grammar or by the constraint layer in `crates/tree-check` (also a
  WebAssembly module for CGO-free hosts, [docs/constraint-wasm.md](docs/constraint-wasm.md)), or is listed with its
  reason in `oracle/data/rejected-clean.json` (including runner exclusions). Those reasons are declarations or the catalog,
  the edition, platform or server configuration, a feature these builds lack, or a limit of the oracle runner;
  no oracle case is classified as an open grammar gap.

Scope and known limits:

- both QUOTED_IDENTIFIER settings (the tree is the same; `tree-check --quoted-identifier on|off` judges each, ADR-0006);
  other session settings at their defaults;
- Azure Synapse and Data Warehouse syntax kept from 1.0.0 is accepted although SQL Server rejects it;
- incremental reparsing needs a tree-sitter runtime with tree-sitter commit
  [`f932671a5`](https://github.com/tree-sitter/tree-sitter/commit/f932671a5) (after 0.27.2). With 0.27.2, a reparse after an
  edit can differ from a fresh parse of the same text when a reused node starts with a keyword that has to be read
  as a name there: 160 of 123,094 edit steps over the oracle did, 0 with the fix. After an edit that leaves invalid
  text, the incremental tree may also place its `ERROR` differently from a fresh parse. A fresh parse is always exact.
- incremental reparsing re-reads more tokens than needed at some boundaries; the tree is unaffected. The nested block
  comment is an external-scanner token, so tree-sitter 0.27.2 re-reads a token whose lex mode changed instead of
  reusing it. Over 217,563 sampled edits of the test inputs, 314 re-read more than 4 tokens beyond the best earlier
  build, 16 of them on valid text before and after the edit (worst of those: 47 tokens in a 0.5 KB script, at a
  `GO` after a `SELECT`).

The 2.0.0 regression coverage includes 60 Korean identifier oracle cases (120 ON/OFF session/case pairs).
The parser accepts UTF-8 and UTF-16; other encodings need transcoding or a custom decoder, and the constraint
interface takes UTF-8 only ([text encoding](docs/consuming.md#text-encoding)). Incremental reuse is bounded by
regression tests for semicolon-free statements, CREATE TABLE names, procedure headers and factored MERGE sources
(#57, #59); valid trees and verdicts are preserved.

Moving from 1.0.0: [docs/migrating-from-1.0.md](docs/migrating-from-1.0.md).

## Using it

Nothing is published to crates.io, npm or any other registry. Pin a commit of this repository: the generated parser (`src/parser.c`, `src/grammar.json`, `src/node-types.json`) is committed, and `src/scanner.c` must be compiled alongside `src/parser.c`.

For Rust, as a Git dependency pinned to a release tag (`v2.0.0`) or a commit:

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
