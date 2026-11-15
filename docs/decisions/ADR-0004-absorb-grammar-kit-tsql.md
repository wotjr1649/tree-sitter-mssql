# ADR-0004: This repository absorbs grammar-kit's T-SQL work

- Status: accepted (owner decision, 2026-11-16)
- Date: 2026-11-16
- Amends: ADR-0002 §2, ADR-0003 §3

## Context

`tree-sitter-grammar-kit` (`D:\AIDEV\tree-sitter-grammar-kit`, MIT) built the first T-SQL verification assets:

- the 726 PARSEONLY cases and 353 edit histories this repository's oracle started from;
- 640 route cases of realistic T-SQL;
- a post-parse context checker, `CheckTSQLContext`. It is written in Go and produces 15 diagnostics computed from the parse tree alone: hint conflicts, boolean versus scalar, column permissions, trigger scope, window frames, COUNT arity, OUTPUT values and others;
- T-SQL queries and fact queries;
- a patch chain over the grammar;
- reports that record engine-verified boundaries.

ADR-0002 and ADR-0003 left context errors to grammar-kit. The owner has stopped grammar-kit's development. Its work moves into `go-treesitter` and this repository.

## Decision

1. **All T-SQL work lives here.**
   - Grammar, oracle, the constraint layer, the context checks, T-SQL queries and the knowledge in grammar-kit's reports belong to this repository.
   - Go-library concerns belong to `go-treesitter`.
   - grammar-kit is no longer the home of any T-SQL check.
2. **The constraint layer becomes the check layer for every rejection decidable from the tree.**
   - `tools/tree-check` (ADR-0003) covers option sets and every engine rejection that the parse tree alone decides, without names, types or catalog state.
   - This includes the conditions `CheckTSQLContext` diagnosed: hint conflicts and argument rules, DML-target hints, column permissions, trigger name, owner and event scope, window-frame rules, COUNT arity, value-less RETURN in scalar functions, CTE without a table source, OUTPUT values, and scalar-versus-predicate positions as far as the tree shows them.
   - Each rule is re-implemented with its engine evidence from the oracle. grammar-kit's code is not copied.
   - What still needs names, types or the catalog stays undecided and is classified `contextual` in the oracle. Examples are an undeclared variable and a non-updatable function as a write target.
3. **grammar-kit's remaining T-SQL assets are imported with attribution (MIT):**
   - the 640 route cases, as an oracle probe set with fresh engine verdicts from this repository's runner;
   - the T-SQL queries, checked against this grammar;
   - the report decisions, recorded where they apply (accepted gaps, ADRs, corpus titles).

   grammar-kit's stored trees, flags and expectations are not imported as truth.
4. ADR-0002 §2 and ADR-0003 §3 ("context errors stay with grammar-kit's `CheckTSQLContext`") are replaced by this decision.

## Consequences

- "Zero defects" is measured against the grammar plus one check layer in one repository. A consumer applies `tools/tree-check`'s rules to get the same guarantee.
- The check layer grows with many small rules. Each cites oracle cases and keeps its own unit tests.
- Consumers that used grammar-kit's Go API need another home for it. That is a go-treesitter decision, outside this repository.
