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
3. **Context errors stay outside the grammar.** Since ADR-0004 the same layer also takes every rejection the parse tree alone decides (hint conflicts, permission and trigger-scope combinations, arity, window frames and more). Only errors that need names, types or the catalog remain `contextual`.
4. **Until a rule exists, the gap is recorded.** Each over-acceptance left by a structure-only change is listed in `docs/accepted-gaps.md`, with its oracle cases, until its rule lands.

## Rules in force

| Rule | Statements | Evidence |
| --- | --- | --- |
| `queue-activation-duplicate`: each ACTIVATION setting at most once | `CREATE QUEUE`, `ALTER QUEUE` | LocalDB 17.0.1000.7, levels 100–170, Msg 102: `queue-duplicate-create/alter`, 8 probes |
| `queue-activation-required`: CREATE needs PROCEDURE_NAME, MAX_QUEUE_READERS and EXECUTE AS | `CREATE QUEUE` | Msg 102: `queue-missing-48/49/50`, `queue-29`, 13 probes; accepted: `queue-missing-51` |

| `cte-terminator`: a statement that starts with a CTE must not directly follow another statement or block without `;` (it may come first in a batch, block or module body, or after `;`, a label, IF, ELSE or WHILE) | any statement list | Msg 319 for the `semicolon/*-no-terminator` probes (after SELECT, END, IF, IF ... ELSE, WHILE, inside a procedure body); every terminated, leading, label and control-body form accepted |
| `exec-output-requires-at`: a value marked OUTPUT/OUT in `EXEC ( ... )` needs `AT linked_server` | `EXEC ( string, ... )` | accepted with AT (OUTPUT, OUT, several); Msg 102 without AT (`oracle/probes/exec-output.jsonl`) |
| `exec-output-variable`: OUTPUT/OUT marks only a local variable (`@v`, not `@@v`) | procedure-call arguments, `EXEC ( ... ) AT` | Msg 179 for constants, strings, DEFAULT, NULL, system variables and bare or bracketed names (kit `issue151-*`, 12 probes); Msg 102 for an expression |
| `column-permission`: a column list allows only SELECT, UPDATE, REFERENCES, UNMASK, ALL [PRIVILEGES] | `GRANT`, `DENY`, `REVOKE` | Msg 1020 for 10 other permissions, per permission or for the object (`context/perm-*`, kit `permission-*`); allowed ones accepted |
| `trigger-scope`: a DATABASE/ALL SERVER trigger has no schema in its name, no DML event and no EXECUTE AS OWNER; ON DATABASE takes no server-only event; a DML trigger takes only INSERT/UPDATE/DELETE | `CREATE`/`ALTER`/`CREATE OR ALTER TRIGGER` | Msg 1094, 1098, 1083, 1082 (`context/trig-*`: all 312 event types of `sys.trigger_event_types` on both scopes; kit `trigger-*`) |
| `builtin-arity`: fixed argument counts of the one-argument aggregates, ABS, LEN, ISNULL, STRING_AGG (unqualified names) | any expression | Msg 174 (`context/arity-*`, `context/count-*`, kit `count-*`); schema-qualified names accepted |
| `scalar-function-return`: a bare RETURN in a scalar function | `CREATE`/`ALTER FUNCTION` | Msg 1075 at any depth (`context/return-*`, kit `scalar-return-*`); table functions and procedures accepted |
| `cte-table-source`: a single SELECT after WITH needs a table source outside the CTE definitions | `WITH ... SELECT` | Msg 422 (`context/cte-*`, kit `cte-none`); accepted with FROM in a subquery, for UNION, INSERT, UPDATE |
| `window-frame-order`: ROWS/RANGE needs ORDER BY in the same OVER or a base window | `OVER ( ... )` | Msg 10756 or 102 (`context/window-*`, kit `window-no-order-*`) |
| `merge-terminator`: MERGE ends with `;` | `MERGE` anywhere | Msg 10713 (`context/merge-*`, `semicolon/merge-*`); a comment before the `;` is fine |
| `table-hint`: known names; at most one hint of each conflict group; FORCESEEK with an index next to INDEX or another FORCESEEK; FORCESEEK on index 0; one SPATIAL_WINDOW_MAX_CELLS; no NOLOCK/READUNCOMMITTED on a DML target, no INDEX/FORCESEEK/FORCESCAN on an UPDATE/DELETE/INSERT target, no READPAST/READCOMMITTEDLOCK on an INSERT target; each TABLE HINT alias once | `WITH ( ... )` on a table, `OPTION (TABLE HINT ...)` | Msg 321, 1047, 10746–10749, 1017, 1065, 1069, 10724, 10745, 4102, 4140, 8720 (`hint-rules`: every hint alone, all 406 pairs, every hint on each DML target; kit `hint-*`, `pair-*`; `table-hints/*`) |

| `external-option-*`, `external-platform`, `external-affinity` | EXTERNAL data sources, models, languages, libraries and resource pools: option names, assignment, uniqueness, required CREATE file options, platform and affinity shapes | `ddl-external/external-*`, LocalDB 17.0.1000.7 levels 100–170, 102/1039/39123/319; ALTER file specs may omit CONTENT |
| `fulltext-option-duplicate`, `fulltext-change-tracking`, `fulltext-no-population` | CREATE FULLTEXT CATALOG/INDEX: unique options, tracking mode, NO POPULATION with tracking OFF | `ddl-external/fulltext-*`, 102/7663 |
| `endpoint-*` | CREATE/ALTER ENDPOINT: option sets, uniqueness, CREATE port/mirroring role requirements, protocols/algorithms/roles, forwarding, IPv4 octets, CREATE STATE restriction | `ddl-external/endpoint-*`, 7816/7801/102/156/7810/7871; 7609/7878 environment errors remain contextual |
All following evidence is query-forms on LocalDB 17.0.1000.7, levels 100–170.

| Rule | Scope | Check | Evidence |
| --- | --- | --- | --- |
| query-cursor-options | DECLARE/SET CURSOR | supported flags and incompatible option groups; identical repeats remain accepted | cursor-iso, cursor-bogus, cursor-two-scope, cursor-fast-scroll, cursor-read-optimistic, cursor-static-keyset; 153/155/1048 |
| query-atomic-options | BEGIN ATOMIC | required ISOLATION/LANGUAGE, uniqueness, supported options and value shapes, module scope | atomic-no-*, atomic-repeat-*, atomic-unknown, atomic-*-var, atomic-durability-bad, plain-atomic; 10784/1039/195/102 |
| query-native-atomic | native modules | NATIVE_COMPILATION requires BEGIN ATOMIC | function-native-missing-atomic; 10783 |
| query-cast-using | CAST/PARSE | USING belongs only to PARSE/TRY_PARSE | cast-using, try-cast-using; 102 |
| query-exec-options | EXEC | RECOMPILE and RESULT SETS occur at most once | exec-repeat-*; 102 |
| query-rowset-schema | rowset schema | nullable/collated columns belong to PREDICT; a table schema reference belongs to OPENXML | predict-schema accepted; openjson-not-null/openxml-not-null; 156/319 |
| query-dml-source | DML row source | OUTPUT and an enclosing INSERT are required | select-delete-source/insert-delete-no-output; 156/10716 |
| query-assignment-chain | UPDATE assignment | only a variable can capture a column assignment | update-column-chain; 102 |
| query-offsets-options | SET OFFSETS | known statement types; identical repeats accepted | offsets-bogus; 195 |
| query-select-quantifier | SELECT DISTINCT | DISTINCT and ALL are mutually exclusive | select-distinct-all; 156 |
| query-cursor-parameter | module parameters | CURSOR parameters belong to procedures | function-cursor-param; 10725/178 |

| query-native-null-input | native functions | RETURNS NULL ON NULL INPUT is unsupported; CALLED ON NULL INPUT is accepted | query-forms/function-native-returns-null, kit B04-r052/r089; 10794 |

BACKUP and RESTORE evidence is the backup-restore probe set on LocalDB 17.0.1000.7, levels 100–170 (666 cases). Repeated and conflicting options are accepted there (`backup-duplicate-*`, `backup-init-noinit`, `restore-recovery-norecovery`), so these rules check names and shapes, not uniqueness.

| Rule | Scope | Check | Evidence |
| --- | --- | --- | --- |
| backup-restore-option | BACKUP/RESTORE WITH | the option belongs to the statement (BACKUP and RESTORE each have their own list); flags take no value, value options need one; values are a string, an integer or a variable; RESTORE FILE is a number or variable; AFTER only after STOPATMARK/STOPBEFOREMARK; MOVE only in RESTORE, with strings or variables; COMPRESSION (ALGORITHM [, LEVEL]), ENCRYPTION (ALGORITHM, SERVER ...) and FILESTREAM (DIRECTORY_NAME) keys and values | `opt-*` (every name bare, `= 1`, `= 'x'` in both statements), `value-*`, `compression-*`, `encryption-*`, `filestream-*`, `move-*`, `mark-*`; 155/153/102/156/1080 |
| backup-restore-device | TO / MIRROR TO / FROM, BACKUP GROUP | a device is a logical name or variable, or DISK/TAPE/URL/DATABASE_SNAPSHOT/VIRTUAL_DEVICE = string or variable; GROUP takes no variables | `device-*`, `backup-device-*`, `restore-from-*`, `backup-group-var`; 155/156/102 |
| backup-restore-file | file list after the database name | FILE/FILEGROUP/PAGE = string or variable, or bare READ_WRITE_FILEGROUPS (not in RESTORE LOG) | `backup-db-*`, `restore-db-*`, `restore-log-rwfg`; 102/156 |
| backup-certificate-options | BACKUP CERTIFICATE WITH | one FORMAT = string; the PRIVATE KEY lists together need FILE and take FILE, ALGORITHM (strings) and ENCRYPTION/DECRYPTION BY PASSWORD = string once each | `cert-*`, `backup-cert-*`; 102 |

## Consequences

- Grammar changes for option lists stay within the cost budget (ADR-0002 §6), and public node shapes stay stable.
- The constraint layer needs node kinds or field names that identify each option. A grammar change that adds an option list should expose them; aliasing to the existing `option` and `identifier` kinds is enough.
- The grammar alone accepts a little more than SQL Server does. The documented guarantee is "grammar plus constraint layer".

## Database DDL evidence (R11)

`oracle/probes/ddl-database.jsonl` and its LocalDB 17.0.1000.7 verdicts at levels 100–170
add small per-node checks rather than enumerating permutations in the grammar:

| Check | Evidence (ddl-database/) |
| --- | --- |
| File FILENAME required for normal CREATE/ALTER ADD; measured attributes unique | files/negative-suspected-003..007, create/negative-suspected-001, extra-file-duplicates/case-001..006 |
| Replica endpoint/mode/failover required; measured duplicates; MODIFY one outer/role setting | ag/negative-suspected-003..005, extra-ag-option-sets/case-003..010, edge/3..8 |
| ALTER AG SET one option, including distinct settings | extra-ag-option-sets/case-001/002 |
| Nested QUERY_STORE/FILESTREAM/CHANGE_TRACKING duplicates; capture policy keys and units | extra-query-store/case-001..007, extra-database-set/case-003/004, edge/13 |
| BPE FILENAME/SIZE required, ordered, assigned, typed and unique | server/negative-suspected-001/002, extra-buffer-pool/case-001..006, edge/9..11 |
| Audit TO/WITH clauses unique; repeated STATE in one WITH allowed | extra-audit-clauses/case-002/003/006 |
| Encryption one assigned supported ALGORITHM | extra-encryption-options/case-001..004/006..009 |
| Distributed option names (LISTENER_URL, not LISTENER) | distributed/listener, distributed/listener_url; route B03-r040 |

Each check has a unit test in constraints.rs. Accepted top-level database SET repeats are preserved
(extra-database-set/case-001/002); ALTER ADD file NAME and MODIFY file NAME are not required by PARSEONLY
(files/negative-suspected-002/006). Engine evidence takes precedence over assumed restrictions.

## Expression evidence (R15)

LocalDB 17.0.1000.7, compatibility levels 100–170, `expr-forms/*` probes.

| Rule | Tree-decidable restriction |
| --- | --- |
| expression-fulltext-language | CONTAINS/FREETEXT argument 3 or TABLE argument 4; one LANGUAGE; name, LCID or local variable; optional TABLE top value follows LANGUAGE |
| expression-static-reference | Type at most schema.name; no variable type/member or unquoted NULL member |
| expression-nested-set-clause | A parenthesized set operand has no local ORDER BY/OFFSET/OPTION/FOR clause |
| expression-rowset-provider | OPENROWSET second argument is server;[user];[password], all connection components are strings; provider and query/table occupy their own slots |

## Remaining DDL route evidence (R17)

The ddl-rest probes use LocalDB 17.0.1000.7 PARSEONLY at levels 100–170. New constraint functions group permission verb/tails; credential/principal/resource/workload/Broker/queue/authorization forms; aggregate/provider/event-session/index forms; graph/NULLS/implicit-EXEC boundaries; and COMMIT/statistics/sensitivity/signature options. Each cites measured probe IDs and has unit coverage. Syntax errors that the grammar already rejects need no duplicate constraint.

Keep the positive oddities in accepted-gaps.md: engine acceptance takes precedence over assumed numeric ranges or option requirements. Windows double-quoted provider FILE strings follow the existing QUOTED_IDENTIFIER state check. New field and node types only extend the public contract.

The closing-* neighbours cover XE scalar source parentheses versus whole-comparison groups; DROP SEQUENCE qualification; measured CONNECT ON AVAILABILITY GROUP rejections; and quoted multiword securable classes. The engine accepts ALTER/CONTROL with the same availability-group class and preserves ordinary SELECT parentheses. These checks close every newly clean syntax-negative committed case; undeclared variables remain contextual.


## Final query-form evidence (R18)

Evidence: `oracle/probes/last-forms.jsonl`, LocalDB 17.0.1000.7, levels 100-170.

| Rule | Tree-decidable restriction | Rejected neighbours |
| --- | --- | --- |
| expression-like-escape | ESCAPE directly follows an unparenthesized LIKE/NOT LIKE predicate | escape-relation/arithmetic/twice/parenthesized-predicate; 156 |
| expression-json-aggregate | One value or key:value; aggregate-specific ORDER BY, NULL handling, RETURNING json and window combinations; new JSON argument forms are not ordinary function arguments | json_arrayagg/json_objectagg-*, json-ordinary_*, json-quoted/schema; 102/156/174/10759 |
| query-select-into-scope | INTO is allowed in every set branch, outside subqueries, CTEs, INSERT sources and view bodies | into-subquery/cte/insert/view/derived-first/scalar/exists; 156 |

`last_forms_match_engine_evidence` checks every probe with explicit contextual/existing-gap
exceptions. Several INTO branches, long/empty/non-string ESCAPE expressions and bare
JSON_ARRAYAGG DISTINCT or WITHIN GROUP follow the measured PARSEONLY acceptance, not a guessed
restriction. Boolean/scalar operands and existing reserved-word gaps stay documented in accepted-gaps.md.
