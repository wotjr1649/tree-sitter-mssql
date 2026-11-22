# AGENTS.md — tree-sitter-mssql

Owner-maintained tree-sitter grammar for T-SQL (Microsoft SQL Server), continued independently from
`alfpricot/tree-sitter-mssql` (ADR-0007); that repository's later changes are not merged. `gh` may resolve to it from
this checkout, so pass `-R wotjr1649/tree-sitter-mssql`.

## Rules

- Edit `grammar.js`, `grammar/**/*.js` and `src/scanner.c`. The rest of `src/` is generated: regenerate it with the
  `Regenerate the parser from grammar.js` step in `.github/workflows/verify.yml` and commit it with the source change.
- Valid T-SQL is what SQL Server's parser accepts (`SET PARSEONLY ON`) at some compatibility level from 100, in the
  same `QUOTED_IDENTIFIER` session. Where inherited tests or `docs/accepted-gaps.md` disagree, SQL Server wins. A fix
  starts from a failing corpus or oracle case and names it.
- Rejections the tree alone decides belong in tree-check; ones that need names, types or the catalog are `contextual`.
  Neither bends the grammar.
- The tree-sitter CLI pin in `package.json` tracks the latest stable release and moves in its own PR with a full-CST
  comparison over corpus, fixtures and oracle sources. Changing the ABI (`ci/tree-sitter-abi.txt`) or a binding's
  runtime lower bound is a major release.
- Releases are `vMAJOR.MINOR.PATCH` tags with a GitHub Release and a `CHANGELOG.md` entry. Only the release PR sets
  the version, in every manifest and the repository's own lockfile entries, and regenerates `src/`.
  Compare with the previous release; the highest class of any change sets the version.
  - MAJOR: a node kind, field or supertype subtype is removed or renamed, or a correct tree of valid input changes.
  - MINOR: additions; error to clean; trees that have an error before and after; accepted invalid input becoming an
    error; tree-identical performance work; a wrong tree of valid input corrected (such as a keyword read as a name),
    with the wrong part named and a migration note.
  - PATCH: build, tooling and documentation, and a CLI move with identical trees.
- Keep the grammar name `mssql`, the C symbol `tree_sitter_mssql` and the `LICENSE` and `NOTICE` attributions.
- Agents prepare branches, commits and pull requests. Every merge, tag, release, repository-setting change and
  deletion needs the owner's separate approval. No registry publishing.
