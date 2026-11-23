# Consuming this grammar

What a downstream project needs to know to depend on `tree-sitter-mssql`.

## Symbol and ABI

The exported C symbol is `tree_sitter_mssql`, so the parser can link alongside a general-SQL tree-sitter grammar
in the same binary. The generated parser uses language ABI 15. Rust consumers supply an ABI-15-capable tree-sitter
runtime. The documented Rust runtime floor is `tree-sitter = "0.26"`. The grammar crate depends on
`tree-sitter-language = "0.1"` at runtime and uses `tree-sitter ~0.27` for its own tests. See the
[README's incremental parsing limits](../README.md#what-it-guarantees) for the additional runtime requirement
for incremental edits.

## Query files

Both bindings expose every query file — `HIGHLIGHTS_QUERY`, `INDENTS_QUERY`, `INJECTIONS_QUERY`, `LOCALS_QUERY`, `TAGS_QUERY` — so a consumer can load `queries/highlights.scm` without resolving the package's install path by hand. The Rust crate exports them as real `pub const` items (`tree_sitter_mssql::HIGHLIGHTS_QUERY`); the Node binding exposes them as properties of its default export (`SQL.HIGHLIGHTS_QUERY`, not a named import).

## WRITETEXT BULK migration

In v2.2.0, a data-less `WRITETEXT BULK` without `WITH LOG` could parse with `BULK` as the
`object_reference` and the target column as a `field`. Consumers of `writetext_statement`
can now match its `keyword_bulk` child and read the target from `object_reference`. In that
form, the text pointer is the only argument after the target, and the payload comes through TDS.

## CHAR VARYING and BINARY VARYING migration

Before v2.6.0, a column declared `char varying` or `binary varying` without a length parsed as a
`char` or `binary` type followed by a stray `identifier` (`varying`) in the `column_definition`,
and any `COLLATE` after it as two more identifiers. The type is now `varchar` or `varbinary` with a
`keyword_varying` child inside `keyword_varchar` or `keyword_varbinary`, the tree `character varying`
already had, and `COLLATE` is `keyword_collate` with its name. With a length, these types, `nchar
varying` and `national text` had error trees and now parse.

## Node-kind stability

`test/node-kinds.txt` lists named node kinds and their field names. Verify checks the snapshot for drift. On pull
requests, [`scripts/check-node-types.mjs`](../scripts/check-node-types.mjs) also compares `src/node-types.json` with
the PR's merge target (the test merge's first parent). It checks for removed kinds, fields, field or child
types and supertype subtypes, lost multiplicity, and existing entries becoming required.

Removing or renaming a kind, field or supertype subtype is a major change. So is changing a correct CST of valid
input. Additions that preserve those CSTs are minor changes, and so is correcting a wrong CST of valid input, with a
migration note. Call out contract
changes in the pull request or commit for `CHANGELOG.md`. See [AGENTS.md](../AGENTS.md) for the release rules.

## Version-specific and deprecated constructs

`versions.json` maps the node kinds that are specific to a SQL Server release (for example `named_window`, requiring SQL Server 2022) or that model deprecated-but-still-executing syntax (for example `readtext_statement`), so a downstream linter can flag either without maintaining its own list. See the file's own `_meta` section for the node kinds it does not yet cover.

## Packaging

Nothing is published to a registry. Pin a Git tag or commit from this repository. On Node.js, the package loads
the native addon with `node-gyp-build`. On Bun, it loads `prebuilds/<platform>-<arch>/tree-sitter-mssql.node`
directly. Supply that prebuild when using Bun with a Git tag or commit. A source build uses `binding.gyp`.
When vendoring the C parser, compile both `src/parser.c` and `src/scanner.c` to resolve the external scanner
symbols.

## Text encoding

- The tree-sitter C API accepts UTF-8, UTF-16LE and UTF-16BE through `TSInputEncoding`. A UTF-8 byte order mark at
  the start of the input parses cleanly. With the 0.27.2 CLI on little-endian Windows, transcode UTF-16BE input to
  UTF-8 before using `tree-sitter parse`.
- Other encodings, such as CP949 or EUC-KR, can be transcoded to UTF-8 or decoded through `TSInputEncodingCustom`
  and a `DecodeFunction` callback (tree-sitter 0.25 or newer). With a custom decoder, node byte offsets are in the
  original encoding. This was checked with a CP949 decoder built on Windows `MultiByteToWideChar` (code page 949):
  a 497-byte CP949 file parsed with 0 errors, and the identifier `고객` spans CP949 bytes 51-55. The same raw CP949
  bytes read as UTF-8 give parse errors.
- The constraint layer (`crates/tree-check`: the CLI, and `tc_check`/`tc_check_v2` in the WASM build) takes UTF-8 text only. Transcode other encodings to UTF-8 before calling it.
- Hangul identifiers: the identifier start and continue classes are generated from code points measured on SQL Server (`oracle/data/unicode/identifier-chars.json`, LocalDB 17.0.1000.7). Accepted: Hangul Syllables U+AC00-D7A3 (11,172 of 11,172), Hangul Jamo U+1100-11FF (240 of 256), Hangul Compatibility Jamo U+3131-318E (94 of 94) and Halfwidth Hangul U+FFA0-FFDC (52 of 61). The engine rejects Hangul Jamo Extended-A U+A960-A97C (0 of 29) and Extended-B U+D7B0-D7FB (0 of 76), and so does the grammar. Oracle set `korean` and `test/corpus/korean.txt` cover Hangul names in queries, DML, DDL, modules, variables, temporary tables, EXEC, CTEs, labels and cursors.

## Constraint checks on hosts without CGO

To check the grammar tree and the constraint layer together, use the WASI reactor of the shared Rust library. See [WASM ABI, pinned build and parity gate](constraint-wasm.md).
