# AGENTS.md — tree-sitter-mssql

Owner-maintained tree-sitter grammar for T-SQL (Microsoft SQL Server), continued from
`alfpricot/tree-sitter-mssql` v1.0.0 (`852cbd2`) to keep it correct and fix defects fast. Intended consumer:
`go-treesitter`, which would pin a commit here and regenerate `src/parser.c` with tree-sitter CLI 0.27.2.
Why and design: `README.md`, `docs/`. Upstream v1.0.0 is commit `852cbd2` in this repository's history.

## Rules

- Source is `grammar.js`, `grammar/**/*.js` and hand-written `src/scanner.c`. `src/parser.c`, `src/grammar.json` and
  `src/node-types.json` are generated: edit the grammar, run `tree-sitter generate` with CLI 0.27.2, commit both.
- Valid T-SQL means SQL Server's parser accepts it (`SET PARSEONLY ON`) at a stated compatibility level. Upstream tests
  and `docs/accepted-gaps.md` are upstream's choices; where they disagree with SQL Server, SQL Server wins and the
  change names the case and level.
- A fix starts from a failing case (corpus test or SQL Server oracle case). Existing corpus tests and fixtures keep
  passing unless the change alters them on purpose and says why.
- Node kinds and field names are a public contract: removing or renaming one is breaking (major version, CHANGELOG);
  adding one is not. Keep the grammar name `mssql` and C symbol `tree_sitter_mssql`.
- Errors that need context (conflicting hints, boolean vs scalar, names, permissions) are outside the grammar: track
  them as `contextual` issues instead of bending the grammar.
- Keep every copyright line in `LICENSE` and the upstream attribution in `NOTICE` (MIT).
