# ADR-0003: Option-set constraints live in a constraint layer, not in the grammar

- Status: accepted (owner decision, 2026-11-16)
- Date: 2026-11-16

## Context

Many T-SQL statements take a set of options that SQL Server checks while parsing. Each option may appear at most once, some options are required, and some cannot be combined. Examples are `CREATE QUEUE ... WITH ACTIVATION (...)`, table and query hints, index options and `WITH` option lists. Violations are rejected with syntax errors such as Msg 102 under `SET PARSEONLY ON` (ADR-0002).

The first case was `CREATE/ALTER QUEUE ... ACTIVATION`, measured on 2026-11-16:

- **Every rule in the grammar** ("each setting once, any order, required ones present"): all 66 queue cases agree with the engine. But states grew 10.4 %, production IDs 19.7 % and inherited field-map entries 102 %, and field ownership changed in public nodes.
- **Structure only** (any order, repeats allowed; `DROP` only alone in `ALTER`; non-empty lists): all 51 engine-accepted cases are fixed. States grew 5.5 %, inherited field-map entries 0.9 %, and node kinds and fields were unchanged. Five rejected cases (duplicates, missing required settings) become clean.

A context-free grammar can express "each of n options at most once, in any order" only by enumerating the orders, so the cost grows with n!. The same choice will come up for hints and other option lists.

## Decision

1. **The grammar models the structure:**
   - which options exist;
   - their value shapes;
   - where the list can appear;
   - combinations that are cheap to state.

   It does not enumerate orders to enforce uniqueness or required options.
2. **A constraint layer in this repository enforces option-set rules:**
   - at most once;
   - required;
   - mutually exclusive.

   It lives in `tools/tree-check` as small per-node rules over the parse tree, each citing its engine evidence.
   - `tree-check` reports a constraint violation like a tree problem.
   - The SQL Server oracle (ADR-0002) judges the grammar plus the constraint layer.
   - "Zero defects" includes the layer.
   - Consumers that need the same guarantees apply the same rules.
3. **Context errors stay outside the grammar.** These are errors that need names, types or permissions (ADR-0002 §2). They are not option-set rules and stay with grammar-kit's `CheckTSQLContext`.
4. **Until a rule exists, the gap is recorded.** Each over-acceptance left by a structure-only change is listed in `docs/accepted-gaps.md`, with its oracle cases, until its rule lands.

## Rules in force

| Rule | Statements | Evidence |
| --- | --- | --- |
| `queue-activation-duplicate`: each ACTIVATION setting at most once | `CREATE QUEUE`, `ALTER QUEUE` | LocalDB 17.0.1000.7, levels 100–170, Msg 102: `queue-duplicate-create/alter`, 8 probes |
| `queue-activation-required`: CREATE needs PROCEDURE_NAME, MAX_QUEUE_READERS and EXECUTE AS | `CREATE QUEUE` | Msg 102: `queue-missing-48/49/50`, `queue-29`, 13 probes; accepted: `queue-missing-51` |

## Consequences

- Grammar changes for option lists stay within the cost budget (ADR-0002 §6), and public node shapes stay stable.
- The constraint layer needs node kinds or field names that identify each option. A grammar change that adds an option list should expose them; aliasing to the existing `option` and `identifier` kinds is enough.
- The grammar alone accepts a little more than SQL Server does. The documented guarantee is "grammar plus constraint layer".
