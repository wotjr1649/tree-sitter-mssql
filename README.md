# tree-sitter-mssql

[![Verify](https://github.com/wotjr1649/tree-sitter-mssql/actions/workflows/verify.yml/badge.svg?branch=main)](https://github.com/wotjr1649/tree-sitter-mssql/actions/workflows/verify.yml)

An owner-maintained [tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammar for Microsoft SQL Server Transact-SQL
(T-SQL). One parser handles the supported T-SQL forms across SQL Server versions without a server-version switch.
[docs/scope.md](docs/scope.md) describes version handling and syntax coverage.

**Why it exists.** When this project began in November 2026, its downstream consumer had pinned
[`alfpricot/tree-sitter-mssql`](https://github.com/alfpricot/tree-sitter-mssql), an MIT-licensed grammar with one
maintainer. The original repository had changed owner names twice (`meloncholic` → `meloncholera` → `alfpricot`)
and rewritten its Git history, leaving the pinned commit unreachable there. This repository continues from its
v1.0.0 so consumers can keep reachable pins and get fixes.

**Origin.** This repository is independent (ADR-0007). It started from the original grammar's v1.0.0, commit [`852cbd2`](https://github.com/alfpricot/tree-sitter-mssql/commit/852cbd20d7dc8078bdea0ebbcbb621e78c86f0cf), merged with its full history on 2026-11-15; that grammar started as a fork of [`tree-sitter-sql`](https://github.com/DerekStride/tree-sitter-sql) by Derek Stride. Its later changes are not merged: defects are fixed here. See [ADR-0001](docs/decisions/ADR-0001-continue-from-upstream-v1.md), [ADR-0007](docs/decisions/ADR-0007-independent-repository.md) and [NOTICE](NOTICE).

**Consumer.** `go-treesitter`, a CGO-free Go tree-sitter library, pins a release of this repository and regenerates
its T-SQL parser with its own pinned tree-sitter CLI.

**Correctness reference.** Valid T-SQL means what SQL Server's own parser accepts under `SET PARSEONLY ON` at a stated compatibility level. Where tests inherited from 1.0.0 or [docs/accepted-gaps.md](docs/accepted-gaps.md) disagree with SQL Server, SQL Server wins. [docs/roadmap.md](docs/roadmap.md) records the milestones.

## What it guarantees

SQL Server's own parser is the reference (`SET PARSEONLY ON`, ADR-0002). The oracle in `oracle/data` holds 10,829
sources (21,658 QUOTED_IDENTIFIER session/case pairs), with recorded SQL Server 2017, 2019, 2022 and 2025 results at
supported compatibility levels 100–170. LocalDB 2025 covers the set under both settings except for one input that
sets PARSEONLY itself and is excluded by the runner. Linux engine observations cover earlier case-set snapshots
under ON. A case counts as accepted if at least one recorded engine and compatibility level accepts it under the
same session setting.

From 2.0.0:

- every engine-accepted oracle case passes tree-check; under QUOTED_IDENTIFIER OFF, tree-check re-parses a
  session-aware projection when needed (the grammar alone misses three accepted OFF cases);
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
  text, the incremental tree may also place its `ERROR` differently from a fresh parse. The oracle records one such
  edit, [#87](docs/accepted-gaps.md#bare-names-and-column-options-87). A fresh parse avoids reuse-related differences.
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

## How it compares with 1.0.0, and is it production-ready?

This repository is independent and maintained by its owner. 1.0.0 is the original grammar
(alfpricot/tree-sitter-mssql v1.0.0) this repository started from. The figures below compare 1.0.0 (`852cbd2`) with this
2.0.0 candidate (`d206c0a`) on one Windows machine, tree-sitter 0.27.2, in alternating A/B runs. Method, per-set tables
and every limit are in [docs/benchmarks.md](docs/benchmarks.md).

| | 1.0.0 | 2.0.0 |
| --- | ---: | ---: |
| SQL Server–accepted oracle cases judged clean (15,585) | 10,056 (64.5 %) | 15,585 (100 %) with tree-check |
| Engine-rejected or runner-excluded oracle pairs flagged (5,833) | 3,083 (52.9 %) | 5,388 (92.4 %) with tree-check |
| A private corpus of 6,006 production T-SQL scripts: accepted → clean / rejected → flagged | 5,874 / 5,987 and 8 / 19 | 5,987 / 5,987 and 18 / 19 |
| `IF … ELSE IF …` chain of 10,000 branches; `IF` nested 10 deep without `BEGIN` | 33.0 s; ERROR tree | 0.06 s; clean tree |
| Parse throughput on that corpus, clean inputs (B/ms) | 11,438 | 9,958 (−13 %) |
| Tree bytes per source byte / largest single parse peak | 24.6 / 80 MiB | 25.7 / 88 MiB |
| Compiled library, gcc `-O2`, stripped | 10.3 MB | 13.4 MB |

On the private corpus, the 2.0.0 candidate parses clean input about 6–20 % slower than 1.0.0 (13 % overall).
Tree-sitter CLI 0.27.2 emits pragmas that disable optimization of `parser.c` when the main lexer exceeds 300
states. #62 brought the candidate's main lexer down from 568 to 271 states, narrowing the earlier 30 % gap.

**2.1.0** (#65) closes most of the remaining gap from the grammar and scanner alone, so it survives regeneration:

- ASCII identifier characters and common separators are tested with plain comparisons.
- String literals no longer take an extra reduction.
- Procedure bodies no longer fork a GLR version at every statement.

**2.1.1** (#68) settles at generation time the procedure-body GLR forks that 2.1.0 still settled at run time, so GLR
version steps on clean input fall to the level of 1.0.0.

These figures were measured differently from the table above: one process, variants interleaved per input, on a pinned
core, gcc `-O2`. On inputs that every version parses cleanly, the ratio over 1.0.0 is:

| Set | 2.0.0 | 2.1.0 | 2.1.1 |
| --- | ---: | ---: | ---: |
| private corpus | 0.877 | 0.987 | 1.017 |
| private, <4 KB / ≥256 KB | 0.848 / 0.899 | 0.956 / 1.011 | 1.027 / 1.011 |
| fixtures | 0.878 | 0.991 | 1.022 |
| corpus tests | 0.884 | 0.973 | 0.984 |
| synthetic | 0.949 | 1.010 | 1.009 |
| oracle | 1.003 | 1.105 | 1.152 |

2.1.1 is faster than 1.0.0 in every set and private size band except the corpus tests (tiny inputs, 112 bytes on
average).

- **The corpus-test gap.** It comes from hidden rules kept for incremental reuse and table size. Every form that
  removed them made incremental reuse or the parse table worse.
- **clang/WASM.** A build that way is 0.95–0.98 times 1.0.0. Most of that gap is the generated lexer's binary search of
  the Unicode identifier table at the end of every identifier. The grammar cannot avoid it
  ([tree-sitter#6139](https://github.com/tree-sitter/tree-sitter/issues/6139)).
- **Tree size.** Tree bytes per source byte on the private corpus are +0.3 % over 1.0.0.

Details: [docs/benchmarks.md](docs/benchmarks.md#211).

**Assessment.** The criteria are SQL Server's own parser (`SET PARSEONLY ON`) as the reference, and each verdict says
what was measured.

| Criterion | Verdict | Evidence and limit |
| --- | --- | --- |
| Correctness against SQL Server | Meets, with limits | Every engine-accepted oracle case passes tree-check (SQL Server 2017–2025, levels 100–170, both QUOTED_IDENTIFIER settings); the grammar alone misses three OFF cases. The 445 rejected or runner-excluded pairs not flagged are classified (context, version, environment, runner). The oracle was written by this project, so it is a regression gate more than an independent test. |
| Real-world agreement | Meets, with limits | 5,987 of 5,987 accepted scripts clean, 18 of 19 rejected flagged; the one miss is a context error (Msg 137). One source, only 19 rejected scripts, and the corpus was also used during development. |
| Robustness | Meets | No crash, hang or superlinear time on 43 stress inputs (nesting to 50,000, 200,000-term chains, an 8 MB literal) or on 41 synthetic classes (exponents 0.96–1.06). |
| Performance and memory | Meets, with limits | About 10 MB/s parsing, 5 MB/s with tree-check, about 26 bytes of tree per source byte, a 2.25 MB script peaks at about 102 MiB of process memory; the WASM module starts at 14 MiB. 2.0.0 was about 13 % slower and 4–5 % more memory-hungry than 1.0.0 on clean input. 2.1.1 (native gcc `-O2`) is about 1–3 % faster than 1.0.0 on every private-corpus size band, the fixtures and synthetic inputs. Oracle throughput is 15.2 % higher, with a per-input median speed ratio of 0.984. Corpus tests are 1.6 % slower. A clang/WASM build is 2–5 % slower. Its trees are 0.3–4 % larger. |
| Incremental editing | Meets, with limits | An incremental tree equals a fresh parse on all 8,063 measured edits (an earlier candidate differed on 24 edits of one file; fixed by a grammar change). Reparse time per edit matches 1.0.0 in 2.1.1 (median per-edit ratio 0.986 on 528 edits of 24 real files; 2.0.0 took about 1.4–1.9 times as long); summed over edits, files of 100 KB–1 MB still take 10 % longer. The edits are four simple kinds, and tree-sitter 0.27.2 has known runtime defects in incremental parsing that 0.27.3 (not released) addresses; 0.27.3 was not tested. Per edit, the reuse repairs cut re-lexed tokens by 8.7 %; 52 edits that leave the text invalid re-lex more than before (by more than 4 tokens, at most +28). |
| API stability | Meets | 2.0.0 removed or renamed no node kinds or fields and added 244 named or anonymous node kinds. 40 of the 5,379 distinct inputs that 1.0.0 parsed cleanly got a different tree, so this was a SemVer major. The [migration guide](docs/migrating-from-1.0.md#trees-that-change-for-valid-input) shows representative changes. |
| Verification in CI | Meets (read from the workflow files) | Full Verify runs on ready pull requests and pushes to `main`. It includes oracle replay, corpus, fixtures, comment/case/whitespace mutation and WASM parity. Oracle engine runs against SQL Server 2017, 2019, 2022 and 2025 on relevant pull requests and weekly. Fuzz covers the parser, scanner and tree-check on relevant pull requests and weekly. |
| Maintenance | Meets, with limits | One maintainer. See the [release history](CHANGELOG.md) through 2.4.0. |

**Known limits.** Errors that need context (undeclared variables, conflicting hints, boolean versus scalar, names,
permissions) are outside a grammar and are not flagged. Some Azure Synapse syntax kept from 1.0.0 is accepted by the
grammar but not by SQL Server. Session settings other than QUOTED_IDENTIFIER are at their defaults, SQL Server
2008–2016 are not verified, and the WASM module is the unoptimised build.

## Using it

Nothing is published to crates.io, npm or any other registry. Pin a tag or commit of this repository.
The generated parser (`src/parser.c`, `src/grammar.json`, `src/node-types.json`) is committed, and
`src/scanner.c` must be compiled alongside `src/parser.c`.

For Rust, pin a release tag in `Cargo.toml`. To pin a commit, replace `tag = "v2.4.0"` with `rev = "<commit>"`.

```toml
[dependencies]
tree-sitter = "0.27"
tree-sitter-mssql = { git = "https://github.com/wotjr1649/tree-sitter-mssql", tag = "v2.4.0" }
```

### Requirements

- The grammar crate requires Rust 1.85 or newer (edition 2024). The example above uses the tree-sitter 0.27
  runtime, whose 0.27.2 release requires Rust 1.90 or newer.
- For Node.js, `tree-sitter` `^0.25.0` is an optional peer dependency. Building the addon from source requires Python
  and the platform's C/C++ build tools for `node-gyp`. This repository uses Node.js 24 (`.nvmrc`).

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

## Repository layout

Most root files sit where a tool expects them: npm, node-gyp, cargo, the tree-sitter CLI, GitHub, or an agent.

| Path | What it is |
| --- | --- |
| **Grammar** | |
| `grammar.js`, `grammar/` | Grammar source: the entry point and its rule modules |
| `src/` | Generated parser (`parser.c`, `grammar.json`, `node-types.json`, `tree_sitter/`) and the hand-written `scanner.c` |
| `queries/` | Tree-sitter query files (highlights, injections, …) |
| `bindings/` | Rust crate and Node.js addon |
| **Tests and tools** | |
| `test/` | Corpus, fixtures, highlight and query tests, node-kind snapshot, cost baselines |
| `oracle/` | SQL Server oracle: recorded cases and verdicts (`data/`) and the .NET runners (`runners/`) |
| `crates/tree-check/` | Constraint layer: Rust library, CLI and wasm reactor |
| `scripts/` | Node check and maintenance scripts with their tests |
| `fuzz/` | `cargo-fuzz` target |
| **Documents** | |
| `README.md`, `CHANGELOG.md` | This page and the release history |
| `docs/` | Architecture, scope, consuming, development, benchmarks, migration, accepted gaps, decisions (ADRs) |
| `LICENSE`, `NOTICE` | MIT license and attribution |
| `AGENTS.md`, `CLAUDE.md` | Standing rules for coding agents (`CLAUDE.md` only includes `AGENTS.md`) |
| `.github/` | CI workflows, the contribution guide (`CONTRIBUTING.md`) and the security policy (`SECURITY.md`) |
| **Package manifests** | |
| `tree-sitter.json` | Grammar metadata for the tree-sitter CLI |
| `package.json`, `package-lock.json`, `binding.gyp` | npm package and Node addon build |
| `Cargo.toml` | Rust crate |
| `versions.json` | Minimum SQL Server version and deprecation data per node kind, shipped in both packages ([docs/consuming.md](docs/consuming.md)) |
| **Tool settings** | |
| `ci/` | Parser ABI pin (`tree-sitter-abi.txt`), used by Verify for regeneration and ABI validation |
| `.editorconfig`, `.gitattributes`, `.gitignore` | Editor, line-ending and ignore rules |
| `.nvmrc` | Node.js version |
| `.oxlintrc.json`, `.oxfmtrc.json`, `.rustfmt.toml` | JavaScript lint and format, Rust format |

## How to contribute

Fix a defect starting from a failing corpus case or SQL Server oracle case, regenerate the parser with the tree-sitter CLI pinned in `package.json` after any grammar change, and run the checks described in [.github/CONTRIBUTING.md](.github/CONTRIBUTING.md). [AGENTS.md](AGENTS.md) holds the standing rules, [docs/development.md](docs/development.md) has the build and test details, and [docs/architecture.md](docs/architecture.md) maps the repository.

## License

MIT — see [LICENSE](LICENSE) and [NOTICE](NOTICE).
