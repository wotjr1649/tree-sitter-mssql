# Consuming this grammar

What a downstream project needs to know to depend on `tree-sitter-mssql`.

## Symbol and ABI

The exported C symbol is `tree_sitter_mssql` (not `tree_sitter_sql`), so linking this grammar alongside a general-SQL tree-sitter grammar in the same binary does not collide. The parser is `LANGUAGE_VERSION` (ABI) 15. The crate depends on `tree-sitter-language = "0.1"` at runtime and only dev-depends on `tree-sitter ~0.27`, so a consumer on `tree-sitter = "0.26"` or newer is compatible.

## Query files

Both bindings expose every query file — `HIGHLIGHTS_QUERY`, `INDENTS_QUERY`, `INJECTIONS_QUERY`, `LOCALS_QUERY`, `TAGS_QUERY` — so a consumer can load `queries/highlights.scm` without resolving the package's install path by hand. The Rust crate exports them as real `pub const` items (`tree_sitter_mssql::HIGHLIGHTS_QUERY`); the Node binding exposes them as properties of its default export (`SQL.HIGHLIGHTS_QUERY`, not a named import).

## Node-kind stability

`test/node-kinds.txt` is the public API surface. It is generated from `src/node-types.json`, and checked for drift on every pull request in `.github/workflows/verify.yml`, so two commits can be diffed mechanically. Node-kind and field-name additions are compatible; removals or renames are breaking; call out either in the pull request or commit that makes the change so it is visible in `CHANGELOG.md`.

## Version-specific and deprecated constructs

`versions.json` maps the node kinds that are specific to a SQL Server release (for example `named_window`, requiring SQL Server 2022) or that model deprecated-but-still-executing syntax (for example `readtext_statement`), so a downstream linter can flag either without maintaining its own list. See the file's own `_meta` section for the node kinds it does not yet cover.

## Packaging

Nothing is published to a registry: pin a commit of this repository. The Node binding compiles the committed generated parser through `node-gyp-build`. `src/scanner.c` must be compiled alongside `src/parser.c` — vendoring one without the other produces a link error, or a parser that can lex neither a block comment nor any sqlcmd command (`:setvar`, `:r`, `:on error` and the rest).

## Text encoding

- The parser reads UTF-8, UTF-16LE and UTF-16BE natively (tree-sitter `TSInputEncoding`). A UTF-8 byte order mark at the start of the input parses cleanly.
- Other encodings, such as CP949 or EUC-KR, must be either transcoded to UTF-8 before parsing, or decoded on the fly with `TSInputEncodingCustom` and a `TSDecodeFunction` (tree-sitter 0.25 or newer). With a custom decoder, node byte offsets are offsets in the original encoding. This was checked with a CP949 decoder built on Windows `MultiByteToWideChar` (code page 949): a 497-byte CP949 file parsed with 0 errors, and the identifier `고객` spans CP949 bytes 51-55. The same raw CP949 bytes read as UTF-8 give parse errors.
- The constraint layer (`crates/tree-check`: the CLI, and `tc_check`/`tc_check_v2` in the WASM build) takes UTF-8 text only. Transcode other encodings to UTF-8 before calling it.
- Hangul identifiers: the identifier start and continue classes are generated from code points measured on SQL Server (`oracle/data/unicode/identifier-chars.json`, LocalDB 17.0.1000.7). Accepted: Hangul Syllables U+AC00-D7A3 (11,172 of 11,172), Hangul Jamo U+1100-11FF (240 of 256), Hangul Compatibility Jamo U+3131-318E (94 of 94) and Halfwidth Hangul U+FFA0-FFDC (52 of 61). The engine rejects Hangul Jamo Extended-A U+A960-A97C (0 of 29) and Extended-B U+D7B0-D7FB (0 of 76), and so does the grammar. Oracle set `korean` and `test/corpus/korean.txt` cover Hangul names in queries, DML, DDL, modules, variables, temporary tables, EXEC, CTEs, labels and cursors.

## Constraint checks on hosts without CGO

To check the grammar tree and the constraint layer together, use the WASI reactor of the shared Rust library. See [WASM ABI, pinned build and parity gate](constraint-wasm.md).
