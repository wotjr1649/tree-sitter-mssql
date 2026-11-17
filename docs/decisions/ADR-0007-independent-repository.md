# ADR-0007: An independent repository

- Status: accepted (owner decision, 2026-11-17)
- Date: 2026-11-17
- Supersedes: ADR-0001 §5 and the merge purpose in §1; ADR-0002 §8 (tag prefix) and the merge purpose in §9

## Context

ADR-0001 continued the grammar from `alfpricot/tree-sitter-mssql` v1.0.0 (`852cbd2`) and planned to merge that
repository's later changes when useful. Since then this repository has replaced most of the grammar's behaviour with
SQL Server-verified behaviour (an oracle of 10,709 sources with recorded SQL Server 2017, 2019, 2022 and 2025 results, a constraint layer,
and its own corpus), and its owner maintains it alone. Merging changes from the original grammar would bring back
choices that SQL Server's verdicts already overrode, and nothing in this repository depends on it.

## Decision

1. This repository is independent. `alfpricot/tree-sitter-mssql` v1.0.0 is the original grammar it started from, not
   an upstream: its later changes are not tracked or merged, and defects are fixed here.
2. Its history stays in this repository (merge commits only, no rewrite) for attribution and for the 1.0.0 migration
   guide. Its v1.0.0 tag stays out of `refs/tags`, as `refs/upstream/v1.0.0`.
3. Releases are tagged `vMAJOR.MINOR.PATCH` (first `v2.0.0`) with a GitHub Release, still without registry publishing.
   The `fork-v` prefix of ADR-0002 §8 is not needed: the original repository's tags are not in `refs/tags`.
4. Defects in tools this repository builds with (the tree-sitter CLI and runtime) are worked around here where a
   grammar change can do it, and documented as limits where it cannot. Reports to those projects are optional and
   need the owner's approval for each post.
5. `docs/divergence-from-1.0.md` keeps the deliberate differences from the original grammar as a record for readers
   coming from 1.0.0.

## Consequences

- The `upstream-sync` label, the upstream-sync milestone and the "upstream" issue source are retired.
- The MIT attribution in `LICENSE` and `NOTICE` is unchanged.
