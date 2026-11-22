# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.3.0] - 2026-11-22

### Fixed

- `WRITETEXT BULK`, whose data SQL Server receives as a separate TDS stream: `writetext_statement` takes `BULK`, the target, the text pointer and an optional `WITH LOG`, with a new `keyword_bulk` child. Without `WITH LOG`, 2.2.0 parsed the data-less form cleanly but read `BULK` as the `object_reference` and the target as a `field`; see the [migration note](docs/consuming.md#writetext-bulk-migration). With `WITH LOG` it parsed with an error. Inline data after `BULK` and a plain `WRITETEXT` without data are now errors, as SQL Server rejects them (Msg 185, 186). Eight oracle probes carry LocalDB and ScriptDOM verdicts. Full CSTs of the 14,986 existing inputs are unchanged; parse states rise from 13,721 to 13,726 (#80).

### Tooling

- An on-demand `parse-bench` workflow counts parser instructions (callgrind Ir) across refs on pinned fixture, corpus and oracle packs. 2.2.0 measured within ±0.061 % of 2.1.1 (#79).
- The ScriptDOM `batch-rules` verdicts cover all 53 cases (#78).

### Documentation

- `docs/scope.md` records compatibility-level coverage. Level 100 is in every verdict file, and the grammar and tree-check take the union of levels; 89 oracle cases change verdict with the level (#81).
- `AGENTS.md` keeps only the rules an agent needs. Correcting a wrong tree of valid input is now a minor change with a migration note, and `docs/consuming.md` says the same (#82).

## [2.2.0] - 2026-11-22

### Changed

- A smaller parse table from the grammar alone (CLI 0.27.2 output): `BETWEEN` and `NOT BETWEEN` share one continuation, and an invocation's `WITHIN GROUP` suffix reduces through one hidden rule. Parse states fall from 14,080 to 13,721 (−2.55 %) and `parser.c` by 1.32 %; large states, node kinds, fields, valid-input trees and verdicts are unchanged. Full CSTs match 2.1.1 on 2,562 corpus cases, 156 fixtures and 10,712 oracle sources. Parse time is unchanged within measurement noise (#75).

### Tooling

- Verify reads the tree-sitter CLI pin from `package.json` and checks the lockfile entry, the `allowScripts` key and the installed binary against it before generating; generation passes the ABI from `ci/tree-sitter-abi.txt` (15) with `--js-runtime native`, the drift check covers `src/tree_sitter/*.h`, the emitted `LANGUAGE_VERSION` must equal the ABI pin, and the generator's optimize-off pragmas fail the job. The parser fuzz job uses the same pin (#73).
- A node-types compatibility gate compares `src/node-types.json` with the base the pull request is merged onto and fails on a removed kind, field, field type, child type or supertype subtype, lost multiplicity, or an existing entry becoming required (#74).
- `docs/benchmarks.md` records the lexer and LR analysis: 26 states of main-lexer headroom below the generator's 300-state threshold, and the rejected experiments (#75).

### Documentation

- Every document was re-checked against this release: current oracle counts (10,712 sources, 21,424 session/case pairs), accepted-gap rows that tree-check now rejects or that later rounds fixed, the CLI/ABI pin locations, the WASM ABI contract, fuzzing in CI, consumer pinning by tag, and broken links. At this release, the documented `WRITETEXT BULK ...` form still parsed with an error and the ScriptDOM `batch-rules` verdicts covered 50 of 53 cases. The issue labels named in `.github/CONTRIBUTING.md` now exist.

## [2.1.1] - 2026-11-18

### Changed

- Parse about 3 % faster than 2.1.0 on real-world scripts (2–4 % by set, gcc `-O2`), from the grammar alone (CLI 0.27.2 output): precedence now settles at generation time the procedure-body GLR forks that dynamic precedence settled at run time (an empty body against a statement after `AS`, and ending the body after its first item), so GLR version steps on clean input fall to the level of 1.0.0; an OPENROWSET connection's first value no longer keeps a unit reduction alive after every literal call argument, and a string literal no longer goes through a one-symbol hidden rule (`parser.c` −0.3 %, large states −25). Node kinds, fields, valid-input trees and verdicts are unchanged, and no sampled incremental edit relexes more. Only trees of inputs SQL Server rejects change: `BEGIN CREATE PROCEDURE p AS SELECT 1; END` now has an ERROR (`END` continues the body, as `END CONVERSATION` would); a final `;` after a nested or native-module body joins the innermost body; and a body that 2.1.0 could end right after `AS` when GLR version pruning lost the dynamic choice now always runs on.

### Added

- Engine verdicts in the oracle for `SELECT 1 WHERE;` and `SELECT 1 HAVING;` (Msg 102) and an unterminated block comment (Msg 113), LocalDB 17.0.1000.7 at levels 100–170 under both QUOTED_IDENTIFIER settings: three corpus error cases that were documented only by their test names (#67).

## [2.1.0] - 2026-11-18

### Changed

- Parse 6–13 % faster than 2.0.0 with about 4 % smaller trees on real-world scripts, from the grammar and scanner alone (CLI 0.27.2 output): the lexer tests ASCII identifier characters and the common separators (space, tab, CR, LF) with plain comparisons instead of binary-searching the Unicode tables (tree-sitter/tree-sitter#6139); a string literal no longer takes an extra hidden reduction (one `alias` kept the generator from removing it); a procedure body no longer forks a GLR version at every statement; the external scanner returns at once when no external token can start. Node kinds, fields, valid-input trees and verdicts are unchanged, and no sampled incremental edit relexes more; in one rejected input (`BEGIN CREATE PROCEDURE p AS SELECT 1; END`) the final `;` moves from the procedure body to the block.

## [2.0.0] - 2026-11-18

The first release of this independent repository. Every input SQL Server accepts in the oracle (10,709 sources,
21,418 QUOTED_IDENTIFIER session/case pairs; recorded SQL Server 2017, 2019, 2022 and 2025 results at supported
compatibility levels 100–170) passes tree-check, including its session-aware re-parsing under OFF; with
the constraint layer, every remaining oracle input is rejected or classified, including runner exclusions. Acceptance
means acceptance by at least one recorded engine/level. Breaking: 40 of the 5,379 distinct inputs that 1.0.0
parsed without an error get a different tree, mostly where 1.0.0 read them wrongly; no node kind or field is
removed. See [docs/migrating-from-1.0.md](docs/migrating-from-1.0.md).

### Changed

- Remove redundant statement-level parentheses from the grammar for `IF`, `WHILE`, `MERGE ... ON` and `WHEN [NOT] MATCHED ... AND` conditions (#32). A parenthesized condition is a `parenthesized_expression` in the `condition`/`predicate` field; the checked examples already had that shape in 1.0.0. This refactor adds or removes no node kind or field: `parser.c` -11 %, parse states -8.5 %, large states -15 % against its preceding commit.
- A dangling `ELSE` binds to the nearest `IF`: `IF a IF b s ELSE t` attached it to the outer `IF` in 1.0.0; SQL Server binds the inner one (LocalDB 17.0.1000.7, run-checked).
- Share nonempty EVENT SESSION, SELECT and UPDATE/DELETE suffixes and inline 50 unaliased helpers to reduce the generated parse table (CLI 0.27.2). All node kinds, fields and named corpus trees are unchanged; no keyword is generalized and no checker rule is weakened.
- Parse about 20 % faster: the generated `parser.c` is compiled with optimization again (#62). tree-sitter 0.27.2 turns optimization off (`#pragma GCC optimize ("O0", "jump-tables")`) when the main lexer has more than 300 states; 8 keyword-like rules defined after `_identifier` kept 11 keywords (REDUCE, REPLICATE, MERGE, JOIN, OUTER, HASH, LOOP, REMOTE, EXEC, EXECUTE, DROP) out of keyword extraction, and the main lexer grew to 568 states. Two move before `_identifier`; the Service Broker queue option names become scanner tokens (still `identifier` nodes); graph MATCH names use a hidden copy of `identifier`; `WITH` after RECONFIGURE is always its OVERRIDE option (SQL Server needs `;` before a CTE, Msg 319). The main lexer has 271 states and parse states fall from 15,020 to 14,107. Incremental reparses relex 8.7 % fewer tokens, but 52 edits that leave the text invalid relex up to 28 more. Valid trees, node kinds and verdicts are unchanged; rejected-input ERROR trees change in 46 oracle cases (42 named trees) and five corpus tests.

### Fixed

- Restore bounded incremental reuse for semicolon-free statements, CREATE TABLE name edits and procedure headers over UPDATE bodies (#57); preserve valid trees and verdicts (20 rejected-input ERROR trees change).
- Reuse factored MERGE sources, Service Broker and CTE subtrees after header edits without changing valid trees or verdicts (#59).
- Fix the procedure/trigger-body boundary tie that made incremental and fresh parses differ (#60): the body takes every statement up to GO by a strict dynamic-precedence score. In a fresh parse, a module that follows other text in its batch no longer ends its body early; one rejected-input ERROR tree changes.

- Nested `IF` without `BEGIN` parses at any depth (it was an ERROR from depth 10), and `IF ... ELSE IF ...` chains parse in linear time (252 KB: 15.9 s before, 0.1 s now). The `;` of an `IF x s;; ELSE t` clause is chosen by the external scanner (`_else_semicolon`, aliased to `;`, one per semicolon, only when ELSE follows) instead of a GLR fork per `;`. Oracle set `if-nesting` (39 cases, LocalDB 17.0.1000.7, levels 100-170, both QUOTED_IDENTIFIER settings).
- Every character SQL Server takes as a token separator is whitespace to the grammar: the 53 code points it accepts between tokens (U+0000-0020, U+0085, U+00A0, U+1680, U+2000-200B, U+2028, U+2029, U+202F, U+205F, U+3000; not U+180E or U+FEFF), oracle set `metamorphic-whitespace` (110 cases, LocalDB 17.0.1000.7, levels 100-170). Keyword case and whitespace no longer change a verdict. tree-check reports no constraint violations on a tree that already has a parse error, instead of re-parsing a normalized copy.
- tree-check rejects a module that does not start its batch (`batch-module`: CREATE/ALTER PROCEDURE, VIEW, FUNCTION, TRIGGER, CREATE RULE/DEFAULT after another statement, inside IF/BEGIN or another module, and a VIEW/FUNCTION/RULE/DEFAULT followed by another statement; Msg 111/156) and a GO line inside a block comment or string, where every client splits the batch (`go-inside-token`; Msg 113/105). Found on a private real-world corpus; oracle set `batch-rules` (50 cases). Three fixtures that SQL Server rejected for these reasons get the missing `GO`.
- A comment between two tokens no longer changes any verdict. tree-check rules skip comment nodes everywhere (a leading `-- note` inside a parenthesized condition made valid procedures fail `scalar-predicate`; 858 of 20,912 commented variants of the oracle and fixtures changed verdict). `FOR /*c*/ SYSTEM_TIME` parses.
- Parse a foreign key with a column list inside a column definition (`a int CONSTRAINT f FOREIGN KEY (a) REFERENCES r (a)`), which SQL Server also reads when a table constraint follows the last column without a comma; found on real-world procedures, oracle set `column-constraints` (LocalDB 17.0.1000.7, levels 100-170). Reject a foreign key in a table variable, table type or RETURNS table in tree-check (`table-variable-foreign-key`, Msg 156).
- tree-check runs in linear time on deep expressions (AND/OR chains, nested CASE, WHILE, function calls and subqueries were quadratic or cubic: a 13 KB AND chain took 4.2 s, now 3.2 ms) and is about twice as fast on real-world scripts, with byte-identical output.
- Parse LIKE ESCAPE, JSON_ARRAYAGG/JSON_OBJECTAGG arguments and INTO on every set branch; preserve existing clean trees and enforce engine-verified syntax boundaries in tree-check.
- Correct the graph fixture to grouped SHORTEST_PATH and complete MATCH predicates; reject engine-verified XE predicate-source, DROP SEQUENCE qualification and permission-class boundaries in tree-check.
- Parse the remaining engine-accepted kit routes: permission classes, principal/cryptographic/CLR/Broker/resource/event/index DDL, sensitivity and signature statements, graph paths, NULLS modifiers, implicit batch-first EXEC, COMMIT durability and statistics options. Preserve existing clean trees and add engine-backed syntax constraints.
- Parse OPENROWSET provider credentials, including omitted user/password, with engine-backed argument constraints.
- Parse spaced/comment-separated comparison operators and nested set operands while retaining existing clean trees.
- Parse BACKUP and RESTORE: file lists, MIRROR TO, BACKUP SERVER/GROUP, certificate, master, service master and symmetric key backups and restores, the metadata RESTOREs (FILELISTONLY, HEADERONLY, LABELONLY, VERIFYONLY, REWINDONLY) and their WITH options (MOVE ... TO, STOPATMARK ... AFTER, COMPRESSION/ENCRYPTION/FILESTREAM lists, signed numbers); tree-check checks option names, values, devices and files against the engine. `restore` is now a statement keyword, so a label named `restore` no longer parses (SQL Server rejects it too).
- Parse EXTERNAL object, FULLTEXT and TCP ENDPOINT DDL sub-forms; enforce engine-verified option sets in tree-check without changing existing clean trees.
- Parse oracle-verified query/DML/cursor/EXEC and native-module sub-forms while preserving existing clean trees; record the approved parser-cost budget (including 10.32 % parser.c growth in #33).
- Parse table/column/constraint and partitioned index DDL verified by the SQL Server oracle; preserve existing clean trees.

### Added

- Add 60 Korean identifier oracle cases and 15 corpus cases for Hangul names, variables, temporary objects, modules, Jamo and invalid neighbours (#58); LocalDB 17.0.1000.7 levels 100–170, QUOTED_IDENTIFIER ON 50 accepted/10 rejected and OFF 48/12. Document native UTF-8/UTF-16 parsing, custom decoding and the UTF-8-only constraint interface in `docs/consuming.md`.

- QUOTED_IDENTIFIER OFF is checked too (ADR-0006). The oracle runner, ScriptDOM and tree-check take `--quoted-identifier on|off` (default on); the wasm module adds `tc_check_v2` with the session (ABI 2, `tc_check` unchanged). The tree stays the same in both sessions; tree-check judges `"text"` as a string under OFF, follows `SET QUOTED_IDENTIFIER` inside the source and the setting a module was created with. Every oracle set is judged under both settings (`.qi-off.json` verdicts, LocalDB 17.0.1000.7, levels 100-170), plus a `quoted-identifier` probe set (122 cases). Found on the way: `BULK INSERT ... FROM "path"`, any string as a filegroup name in ON/FILESTREAM_ON/MOVE positions, and string-only positions under ON (RAISERROR message, WAITFOR DELAY, parenthesized dynamic EXEC, COLLATE, CONVERT style type).
- Parse full-text LANGUAGE arguments and general static references with method chaining; add the optional schema field to static field nodes, with no node kind or field removed.
- Add `restore_statement` and 7 keyword node kinds (`keyword_restore`, `keyword_mirror`, `keyword_filelistonly`, `keyword_headeronly`, `keyword_labelonly`, `keyword_verifyonly`, `keyword_rewindonly`); `backup_statement` and `option` gain child kinds (`option` also takes a `unary_expression` value). No node kind or field is removed, and the existing BACKUP trees keep their shape.
- Add 666 backup-restore probes (LocalDB 17.0.1000.7, levels 100-170) and 262 corpus cases.
- Add 174 query-form probes/corpus cases, local statement constraints and incremental edit coverage; existing node kinds and fields remain.
- Add `generated_always`, `selective_xml_path` and 20 keyword node kinds; no existing node kinds or fields are removed.
- Database file/filegroup, nested database SET, mirroring/HADR, database encryption-key and audit-specification DDL; availability-group replica/listener and server-configuration forms.
- 56 named kinds (17 structural and 39 keyword kinds) with no existing kind, field or child-type removed. Existing clean oracle and fixture trees retain their fields and ranges.
- Engine-backed database DDL probes and option-set constraints for required, repeated and single-change settings.

### Queries

- `queries/highlights.scm` captures every `keyword_*` kind; 138 were missing, among them `TRY_PARSE`, `ESCAPE` and
  the BACKUP/RESTORE keywords. `scripts/check-registries.mjs` fails on a keyword kind without a highlight.

### Tooling

- The constraint layer (`crates/tree-check`) is a library split by rule family, and it builds into a WebAssembly
  reactor with a C ABI and a native/wasm parity gate over every oracle case
  ([docs/constraint-wasm.md](docs/constraint-wasm.md)).
- `oracle/data/rejected-clean.json` classifies every engine-rejected case that parses cleanly; the replay fails on an
  unclassified one. ScriptDOM is a second opinion (`oracle/runners/ScriptDomCheck`).
- `scripts/comment-mutate.mjs --axes comment,case,whitespace` gates in Verify that comments, keyword case and
  whitespace change no verdict, under both QUOTED_IDENTIFIER settings.
- libFuzzer fuzzing in CI of the parser and scanner, and of tree-check inspection and edit histories under both QUOTED_IDENTIFIER settings (#61); `scripts/runtime-cost.mjs` bounds library size, parse time and peak memory where the host
  can measure it.
- Layout: `scripts/` (Node tools and gates), `crates/` (Rust), `oracle/data` and `oracle/runners`
  ([docs/architecture.md](docs/architecture.md)).

## 1.0.0 (2026-11-04)

### Breaking Changes

* **grammar:** bind IS predicates below arithmetic, reject open comments (#77)

### Features

* **grammar:** parse more sqlcmd commands and require a space after each (#81)
* **queries:** inject concatenated EXEC strings and parse :reset (#80)
* **grammar:** add name, target, and body fields to modules and CTEs (#79)
* **grammar:** parse APPLY over VALUES, CLR modules, and :on error (#78)

## 0.2.3 (2026-10-29)

### Bug Fixes

* **grammar:** parse the $IDENTITY and $ROWGUID pseudo-columns (#73)

## [0.2.2](https://github.com/meloncholera/tree-sitter-mssql/compare/v0.2.1...v0.2.2) - 2026-10-04

### Other

- *(release)* promote dev to main — shared release pipeline ([#57](https://github.com/meloncholera/tree-sitter-mssql/pull/57))
- *(release)* align the release pipeline with the shared grammar repository standard ([#56](https://github.com/meloncholera/tree-sitter-mssql/pull/56))

## [0.2.1](https://github.com/meloncholera/tree-sitter-mssql/compare/v0.2.0...v0.2.1) - 2026-10-04

### Fixed

- *(release)* point repository metadata and package scope at meloncholera ([#48](https://github.com/meloncholera/tree-sitter-mssql/pull/48))
- *(grammar)* give every SELECT a query_specification node ([#46](https://github.com/meloncholera/tree-sitter-mssql/pull/46))
- *(grammar)* accept the documented forms of four statements ([#45](https://github.com/meloncholera/tree-sitter-mssql/pull/45))
- *(linguist)* mark generated grammar JSON as linguist-generated ([#44](https://github.com/meloncholera/tree-sitter-mssql/pull/44))

### Other

- *(deps)* update eslint and node-gyp ([#51](https://github.com/meloncholera/tree-sitter-mssql/pull/51))
- align tooling with the shared grammar repository standard ([#50](https://github.com/meloncholera/tree-sitter-mssql/pull/50))
- *(deps)* bump actions/upload-artifact from 4.6.2 to 7.0.1 in the github-actions-major group across 1 directory ([#21](https://github.com/meloncholera/tree-sitter-mssql/pull/21))
- sync dev from main ([#43](https://github.com/meloncholera/tree-sitter-mssql/pull/43))

## [0.2.0](https://github.com/meloncholic/tree-sitter-mssql/compare/v0.1.6...v0.2.0) - 2026-09-26

### Added

- publish prebuilt native addons and WASM, add fuzzing and a perf summary ([#39](https://github.com/meloncholic/tree-sitter-mssql/pull/39))
- [**breaking**] group GO-separated statements into a batch node, add query files ([#33](https://github.com/meloncholic/tree-sitter-mssql/pull/33))

### Fixed

- *(grammar)* add missing DDL statements and fix trailing-clause bugs ([#32](https://github.com/meloncholic/tree-sitter-mssql/pull/32))
- *(grammar)* give union branches a visible boundary and fix two DML gaps ([#29](https://github.com/meloncholic/tree-sitter-mssql/pull/29))
- *(grammar)* unify DML write-target modeling and fix two DDL gaps ([#25](https://github.com/meloncholic/tree-sitter-mssql/pull/25))
- *(grammar)* correct operator precedence and identifier/alias typing ([#22](https://github.com/meloncholic/tree-sitter-mssql/pull/22))

### Other

- drop macOS from the release prebuild matrix ([#40](https://github.com/meloncholic/tree-sitter-mssql/pull/40))
- clean up grammar structure, field tagging, and JS tooling ([#38](https://github.com/meloncholic/tree-sitter-mssql/pull/38))
- add consumer-facing usage docs and node-kind stability metadata ([#37](https://github.com/meloncholic/tree-sitter-mssql/pull/37))
- remove .npmrc from the tree ([#36](https://github.com/meloncholic/tree-sitter-mssql/pull/36))
- scope gitattributes and editorconfig to tracked file types ([#35](https://github.com/meloncholic/tree-sitter-mssql/pull/35))
- sync .gitattributes and .editorconfig with org template ([#34](https://github.com/meloncholic/tree-sitter-mssql/pull/34))

## [0.1.6](https://github.com/meloncholic/tree-sitter-mssql/compare/v0.1.5...v0.1.6) - 2026-09-12

### Added

- *(grammar)* add column security, column-level GRANT, and resumable index DDL ([#13](https://github.com/meloncholic/tree-sitter-mssql/pull/13))

### Fixed

- *(node)* correct binding package contract ([#16](https://github.com/meloncholic/tree-sitter-mssql/pull/16))

### Other

- *(release)* verify package contents before publishing ([#15](https://github.com/meloncholic/tree-sitter-mssql/pull/15))
- *(tooling)* adopt Rust 2024 ([#14](https://github.com/meloncholic/tree-sitter-mssql/pull/14))
- *(grammar)* move select trailing clauses off FROM, add named windows
- *(deps)* bump cargo-bins/cargo-binstall from 1.22.0 to 1.23.0 in the github-actions group across 1 directory ([#1](https://github.com/meloncholic/tree-sitter-mssql/pull/1))

## [0.1.5](https://github.com/meloncholic/tree-sitter-mssql/compare/v0.1.4...v0.1.5) - 2026-09-08

### Added

- parse ALTER TABLE SWITCH PARTITION, the $PARTITION pseudo-function, and graph table MATCH querying ([#8](https://github.com/meloncholic/tree-sitter-mssql/pull/8))
- parse FOR SYSTEM_TIME, AT TIME ZONE, TABLESAMPLE, and rowset function coverage ([#5](https://github.com/meloncholic/tree-sitter-mssql/pull/5))

### Fixed

- *(grammar)* correct identifier and multipart-name parsing, dedup helpers ([#3](https://github.com/meloncholic/tree-sitter-mssql/pull/3))

### Other

- *(docs)* correct LICENSE copyright name ([#4](https://github.com/meloncholic/tree-sitter-mssql/pull/4))
- promote dev to main ([#6](https://github.com/meloncholic/tree-sitter-mssql/pull/6))
- sync dev from main ([#7](https://github.com/meloncholic/tree-sitter-mssql/pull/7))
