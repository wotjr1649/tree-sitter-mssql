# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

## [Unreleased]

### Changed

- A parenthesized `IF`, `WHILE`, `MERGE ... ON` or `WHEN [NOT] MATCHED ... AND` condition is now one `parenthesized_expression` in the `condition`/`predicate` field (for example `IF (a = 1)` gives `condition: (parenthesized_expression (binary_expression ...))`), instead of the statement owning the parentheses. No node kind or field is added or removed. This removes a duplicated copy of the expression grammar: `parser.c` -11 %, parse states -8.5 %, large states -15 %.

### Fixed

- Every character SQL Server takes as a token separator is whitespace to the grammar: the 53 code points it accepts between tokens (U+0000-0020, U+0085, U+00A0, U+1680, U+2000-200B, U+2028, U+2029, U+202F, U+205F, U+3000; not U+180E or U+FEFF), oracle set `metamorphic-whitespace` (110 cases, LocalDB 17.0.1000.7, levels 100-170). Keyword case and whitespace no longer change a verdict: `tools/comment-mutate.mjs --axes comment,case,whitespace` gates it in Verify (84,752 variants, 0 changed). tree-check reports no constraint violations on a tree that already has a parse error, instead of re-parsing a normalized copy.
- tree-check rejects a module that does not start its batch (`batch-module`: CREATE/ALTER PROCEDURE, VIEW, FUNCTION, TRIGGER, CREATE RULE/DEFAULT after another statement, inside IF/BEGIN or another module, and a VIEW/FUNCTION/RULE/DEFAULT followed by another statement; Msg 111/156) and a GO line inside a block comment or string, where every client splits the batch (`go-inside-token`; Msg 113/105). Found on a private real-world corpus; oracle set `batch-rules` (50 cases). Three fixtures that SQL Server rejected for these reasons get the missing `GO`. The oracle runner no longer stops on an empty script.
- A comment between two tokens no longer changes any verdict. tree-check rules skip comment nodes everywhere (a leading `-- note` inside a parenthesized condition made valid procedures fail `scalar-predicate`; 858 of 20,912 commented variants of the oracle and fixtures changed verdict). `FOR /*c*/ SYSTEM_TIME` parses. `tools/comment-mutate.mjs` gates this in Verify.
- Parse a foreign key with a column list inside a column definition (`a int CONSTRAINT f FOREIGN KEY (a) REFERENCES r (a)`), which SQL Server also reads when a table constraint follows the last column without a comma; found on real-world procedures, oracle set `column-constraints` (LocalDB 17.0.1000.7, levels 100-170). Reject a foreign key in a table variable, table type or RETURNS table in tree-check (`table-variable-foreign-key`, Msg 156).
- Parse LIKE ESCAPE, JSON_ARRAYAGG/JSON_OBJECTAGG arguments and INTO on every set branch; preserve existing clean trees and enforce engine-verified syntax boundaries in tree-check.
- Correct the graph fixture to grouped SHORTEST_PATH and complete MATCH predicates; reject engine-verified XE predicate-source, DROP SEQUENCE qualification and permission-class boundaries in tree-check.
- Parse the remaining engine-accepted kit routes: permission classes, principal/cryptographic/CLR/Broker/resource/event/index DDL, sensitivity and signature statements, graph paths, NULLS modifiers, implicit batch-first EXEC, COMMIT durability and statistics options. Preserve existing clean trees and add engine-backed syntax constraints.
- Parse OPENROWSET provider credentials, including omitted user/password, with engine-backed argument constraints.
- Parse spaced/comment-separated comparison operators and nested set operands while retaining existing clean trees.
- Parse BACKUP and RESTORE: file lists, MIRROR TO, BACKUP SERVER/GROUP, certificate, master, service master and symmetric key backups and restores, the metadata RESTOREs (FILELISTONLY, HEADERONLY, LABELONLY, VERIFYONLY, REWINDONLY) and their WITH options (MOVE ... TO, STOPATMARK ... AFTER, COMPRESSION/ENCRYPTION/FILESTREAM lists, signed numbers); tree-check checks option names, values, devices and files against the engine. `restore` is now a statement keyword, so a label named `restore` no longer parses (SQL Server rejects it too).
- Parse EXTERNAL object, FULLTEXT and TCP ENDPOINT DDL sub-forms; enforce engine-verified option sets in tree-check without changing existing clean trees.
- Parse oracle-verified query/DML/cursor/EXEC and native-module sub-forms while preserving existing clean trees; keep parser-cost growth below 10%.
- Parse table/column/constraint and partitioned index DDL verified by the SQL Server oracle; preserve existing clean trees.

### Added

- Parse full-text LANGUAGE arguments and general static references with method chaining; add the optional schema field to static field nodes, with no node kind or field removed.

- Add `restore_statement` and 7 keyword node kinds (`keyword_restore`, `keyword_mirror`, `keyword_filelistonly`, `keyword_headeronly`, `keyword_labelonly`, `keyword_verifyonly`, `keyword_rewindonly`); `backup_statement` and `option` gain child kinds (`option` also takes a `unary_expression` value). No node kind or field is removed, and the existing BACKUP trees keep their shape.
- Add 666 backup-restore probes (LocalDB 17.0.1000.7, levels 100-170) and 262 corpus cases.
- Add 174 query-form probes/corpus cases, local statement constraints and incremental edit coverage; existing node kinds and fields remain.
- Add `generated_always`, `selective_xml_path` and 20 keyword node kinds; no existing node kinds or fields are removed.
- Database file/filegroup, nested database SET, mirroring/HADR, database encryption-key and audit-specification DDL; availability-group replica/listener and server-configuration forms.
- 56 named kinds (17 structural and 39 keyword kinds) with no existing kind, field or child-type removed. Existing clean oracle and fixture trees retain their fields and ranges.
- Engine-backed database DDL probes and option-set constraints for required, repeated and single-change settings.

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
