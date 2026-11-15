# ADR-0001: Continue the grammar from upstream v1.0.0

- Status: accepted (owner decision, 2026-11-15)
- Date: 2026-11-15

## Context

T-SQL is the owner's main SQL dialect, and no well-maintained tree-sitter grammar exists for it. The only candidate is
[`alfpricot/tree-sitter-mssql`](https://github.com/alfpricot/tree-sitter-mssql), MIT licensed. Facts as of 2026-11-15:

- The repository was created on 2026-09-08; its first commit is `chore: reinitialize repository`. Releases: v0.1.5
  (2026-09-08), v0.1.6 (09-12), v0.2.0 (09-26), v0.2.1 and v0.2.2 (10-04), v0.2.3 (10-29), v1.0.0 (2026-11-04). After
  v1.0.0 only dependabot pull requests landed (through 2026-11-09).
- One human maintainer; no forks. The owner name changed at least twice: `meloncholic` → `meloncholera` → `alfpricot`.
- Its Git history was rewritten. The commit `go-treesitter` pins, `8620fbcf` (2026-09-29), shares no ancestor with any
  current branch or tag; a new-history commit `2e34d86e` has the same tree. The pin is therefore unreachable upstream.
- v1.0.0 (`852cbd2`) moved several children into fields (`name`, `target`, `body`); node kinds and field names were
  otherwise only added. Regenerating v1.0.0 with tree-sitter CLI 0.27.2 reproduces `src/parser.c`,
  `src/grammar.json` and `src/node-types.json` byte for byte.
- Against SQL Server 2025 LocalDB (17.0.1000.7) with `SET PARSEONLY ON`, the v1.0.0 tree agrees with the engine on
  386 of 681 judged cases (56.7 %) at compatibility level 170, and 55.9 % at level 110. Most under-acceptance comes from
  a few small rule gaps; much of the over-acceptance needs context (conflicting hints, boolean versus scalar).
- Upstream's own corpus pins some choices that SQL Server does not share: `test/corpus/errors.txt` marks
  `SELECT 1 WHERE 1=1;` and `SELECT 1 GROUP BY 1;` as illegal, and SQL Server accepts both.

## Decision

1. Continue the grammar in this repository from upstream v1.0.0 (`852cbd2`), merged with its full history so upstream
   changes can be merged later. Upstream tags stay out of `refs/tags`; v1.0.0 is kept as `refs/upstream/v1.0.0`.
2. The correctness reference is SQL Server's parser (`SET PARSEONLY ON`) at a stated compatibility level, recorded with
   the engine version. Where upstream tests or `docs/accepted-gaps.md` disagree with SQL Server, SQL Server wins and the
   change names the case and the level.
3. The grammar stays in the tree-sitter DSL (`grammar.js`, `grammar/**/*.js`, plus the hand-written `src/scanner.c`).
   Generated files are regenerated with CLI 0.27.2 and committed.
4. No registry publishing (crates.io, npm, GitHub Packages) and no automated tagging. Consumers pin commits.
5. Upstream changes are merged when useful. Our fixes are offered upstream only with the owner's approval for each post.
6. Node kinds and field names stay a public contract: removing or renaming one is breaking. The grammar name `mssql` and
   the C symbol `tree_sitter_mssql` stay.

## Consequences

- Pinned commits stay reachable because this repository does not rewrite its history.
- Correctness can only rise as fast as the oracle: fixes need SQL Server verdicts, so the oracle comes first
  (`docs/roadmap.md`, M1).
- Fixing a case that upstream's corpus pins as an error changes that corpus case, and later upstream merges can
  conflict there.
- Context-dependent errors stay outside the grammar and are tracked as `contextual` issues.
- One maintainer still owns the grammar; the dependency moves from upstream's maintainer to this repository's owner.
- Upstream's release automation is gone; `tools/release-policy.mjs versions` keeps only the check that the three
  manifests agree, and `CHANGELOG.md` keeps upstream's history.

## Open points

The owner settled the open points on 2026-11-15; see
[ADR-0002](ADR-0002-verification-and-workflow.md).
