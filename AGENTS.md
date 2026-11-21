# AGENTS.md — tree-sitter-mssql

Owner-maintained tree-sitter grammar for T-SQL (Microsoft SQL Server), continued from `alfpricot/tree-sitter-mssql` to
keep it correct and fix defects fast. Why and design: `README.md`, `docs/`. The repository is independent
(`docs/decisions/ADR-0007-independent-repository.md`, baseline in ADR-0001); the original grammar's later changes are
not merged.

`go-treesitter` records its grammar commit and CLI pins in `tools/gen/pins.json`. Propose this repository's latest
release tag in a separate PR under go-treesitter ADR-0024 rule 7, with a comparison against its current pin and owner
approval. go-treesitter regenerates `src/parser.c` with its own pinned CLI.

## Rules

- Source is `grammar.js`, `grammar/**/*.js` and hand-written `src/scanner.c`. `src/parser.c`, `src/grammar.json`,
  `src/node-types.json` and `src/tree_sitter/*.h` are generated. Regenerate with the pinned CLI using the `Regenerate
  the parser from grammar.js` step in `.github/workflows/verify.yml` and commit the generated files with the source
  change. Never edit generated files by hand or by post-processing. A generated `parser.c` must not contain the
  generator's optimize-off pragmas.
- The CLI pin is `TREE_SITTER_CLI_VERSION` in `.github/workflows/verify.yml`. `devDependencies.tree-sitter-cli`,
  `allowScripts` and the `prestart`/`start` scripts in `package.json` and `fuzz.yml` repeat it: change all together.
  It tracks the latest stable tree-sitter release (`gh release view -R tree-sitter/tree-sitter`). Move it in its own
  PR: regenerate, compare full CSTs on the fixed inputs and record generated-code differences. Identical trees are a
  patch release; otherwise follow the SemVer rules below.
- The ABI is the CLI default, `LANGUAGE_VERSION` in `src/parser.c` (the generate step passes no `--abi` yet). Binding
  runtime lower bounds are in the binding manifests. Changing the ABI or a runtime lower bound needs the owner's
  approval and a major release.
- Valid T-SQL means SQL Server's parser accepts it (`SET PARSEONLY ON`) at a stated compatibility level. Tests
  inherited from the original grammar (baseline in ADR-0001) and `docs/accepted-gaps.md` are that grammar's choices;
  where they disagree with SQL Server, SQL Server wins and the change names the case and level.
- Releases are `vMAJOR.MINOR.PATCH` tags with a GitHub Release; the latest is `gh release view -R
  wotjr1649/tree-sitter-mssql`. Record every release in `CHANGELOG.md`.
  - MAJOR: a node kind, field or supertype subtype is removed or renamed, or the CST of valid input that the previous
    release parsed without an error changes. Add migration notes. Valid input: see the SQL Server rule above.
  - MINOR: additions, error-to-ok fixes, accepted invalid input becoming an error, changes to the tree of input that
    is an error before and after (`ERROR` and `MISSING` included), and tree-identical performance work.
  - PATCH: build, tooling and documentation changes, and a CLI-pin move with identical trees.
- The release PR sets the release version in every manifest and its own lockfile entry, including `tree-sitter.json`
  `metadata.version`, and regenerates `src/` because `src/parser.c` embeds that version (`scripts/release-policy.mjs
  versions` checks `Cargo.toml`, `package.json` and `tree-sitter.json`). Fix PRs change none of them.
- Keep the grammar name `mssql` and the C symbol `tree_sitter_mssql`.
- A fix starts from a failing case (corpus test or SQL Server oracle case) and cites its evidence. Existing corpus
  tests and fixtures keep passing unless the change alters them on purpose and says why.
- Errors that need context (conflicting hints, boolean vs scalar, names, permissions) are outside the grammar: track
  them as `contextual` issues instead of bending the grammar.
- Keep every copyright line in `LICENSE` and the attribution in `NOTICE` (MIT).
- Agents prepare branches, commits and pull requests. Every merge, tag, release, publication, repository-setting
  change and deletion needs the owner's separate approval. No registry publishing (npm, crates.io, PyPI).
