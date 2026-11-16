# Accepted gaps

This is the ledger of genuine mismatches between what SQL Server itself accepts or rejects and
what this grammar's tree does — one row per gap, whether the grammar over- or under-accepts, and
the corpus case (or fixture) that pins the current behavior so a future change is a reviewable
diff instead of a silent regression. It consolidates the over- and under-acceptance notes that
were previously scattered across grammar comments with the subset of
`test/fixtures/README.md`'s "Not parsed on purpose" table that is an actual mismatch rather than a
construct the grammar correctly rejects because SQL Server rejects it too — see that file for the
complete list of constructs deliberately left unmodeled, matched or not.

An entry with no corpus case listed is a known gap in the pinning itself, not a claim that the
behavior is untested in general — see the note on each such row.

## Under-acceptance — constructs SQL Server accepts that this grammar rejects

`test/fixtures/README.md`'s "Not parsed on purpose" table already pins three of these: the
physical join hint with no join type (`HASH JOIN`/`LOOP JOIN`/`MERGE JOIN`/`REMOTE JOIN`), the rare
sub-forms of deprecated or console statements (`WRITETEXT BULK ...`,
`KILL STATS JOB`/`KILL QUERY NOTIFICATION`), and the parenthesized union branch with its own
`ORDER BY` alongside `TOP`/`OFFSET`/`FETCH`. See that file rather than duplicating the rows here.
That same table also lists `COMPUTE`, the `*=`/`=*` outer-join operators, and
`EXEC p 1 + 2`/`EXEC p @a = 1, 2` — those three are not gaps at all, since SQL Server itself
rejects them too (the first two are removed constructs; the third violates Msg 119's
argument-ordering rule), so the grammar's rejection already matches SQL Server's.

One further under-acceptance surfaced while consolidating this ledger, not yet in either source:

| Construct | Why | Pinned by |
| --- | --- | --- |
| `ALTER MESSAGE TYPE name VALIDATION = ...` | Valid, documented T-SQL (unlike its six sibling Service Broker object kinds, which each gained an `ALTER` form, message types were assumed immutable and given none) — this is a genuine missing rule, not a deliberate decision | Not modeled at all — no corpus case |

## Over-acceptance — constructs this grammar accepts that SQL Server rejects

| Construct | Why | Pinned by |
| --- | --- | --- |
| A body-less `CREATE TRIGGER ... AS` with no statements after `AS` | The procedure and trigger bodies share one rule; a body-less procedure is legitimate SSMS-scripted syntax, but SQL Server rejects a body-less trigger specifically, and modeling that as a second rule was judged not worth the duplication for one invalid combination | No corpus case exercises a body-less trigger yet — the existing "An empty procedure body and a numbered procedure" case in `test/corpus/procedures.txt` only pins the (correct) procedure form |
| `CREATE CERTIFICATE ... FROM PROVIDER name` | `CREATE CERTIFICATE` and `CREATE ASYMMETRIC KEY` share the same `_key_source` rule, but only `ASYMMETRIC KEY` accepts `PROVIDER` in real SQL Server | No corpus case exercises this combination yet — adding one needs a `tree-sitter generate` + `test --update` cycle; tracked here as a known gap in the pinning rather than fixed in this pass |
| `CREATE FUNCTION ... RETURNS TABLE ( columns )` with an inline (`RETURN query`) or `BEGIN ... END` body | The column list is the result shape of a CLR table-valued function, whose body is `EXTERNAL NAME`; the `RETURNS` clause and the body are separate rules, and tying the column list to the CLR body would duplicate the function definition for one invalid combination | No corpus case exercises the invalid combination; "CLR scalar and table-valued functions with EXTERNAL NAME" in `test/corpus/create_function.txt` pins the valid CLR form |

## FROM-less clauses: measured residual gaps

WHERE, GROUP BY and HAVING do not require FROM. SQL Server 2025 LocalDB 17.0.1000.7,
PARSEONLY at levels 100–170, accepts both upstream error tests `SELECT 1 WHERE 1=1;`
and `SELECT 1 GROUP BY 1;`. They are positive cases in
`test/corpus/select-fromless.txt` (ADR-0002 §9); the other existing error cases are unchanged.

Making those clauses independent exposes the following existing expression/session gaps. All
listed oracle cases now parse cleanly; no engine-accepted clean case becomes an error. This is
not a claim of zero defects, and these are not option-set constraints covered by ADR-0003.

| Cases | SQL Server verdict | Classification and disposition |
| --- | --- | --- |
| `count-empty`, `mut-02235`, `mut-02255` | Msg 174 on every committed engine/level | Rejected by the `builtin-arity` constraint (ADR-0003, ADR-0004) |
| `regex-79` | Msg 4145 on SQL Server 2017–2022 and 2025 levels 100–160; Msg 156 on both 2025 engines at level 170 | Mixed: contextual scalar predicate at lower levels, unresolved qualified REGEXP_LIKE syntax at 170. The syntax rejection is not covered by ADR-0002 context or ADR-0003 option-set rules |
| `regex-80` | Msg 4145 on 2025; 195 on older engines | Contextual scalar REGEXP_COUNT used as a predicate; older engines lack the function |
| `qi-on-contains-search`, `qi-on-freetext-search`, `qi-on-property`, `qi-on-language` | Msg 102 on every committed engine/level | Unresolved syntax over-acceptance: with QUOTED_IDENTIFIER ON, these full-text arguments require strings rather than double-quoted identifiers. Requires session-aware lexical/argument rules, not an option-set constraint. Matching `qi-off-*` cases are accepted |
| `SELECT NOT 1 WHERE 1=1;` | LocalDB Msg 156 at levels 100–170 | Unresolved SELECT expression over-acceptance. The corresponding FROM-bearing control is already clean in main; no corpus case pins this gap |
| `SELECT 1 WHERE CURRENT OF c;` | LocalDB Msg 102 at levels 100–170 | Unresolved statement restriction: CURRENT OF belongs to positioned UPDATE/DELETE. The corresponding FROM-bearing SELECT is already clean in main; no corpus case pins this gap |

The focused probes also leave the existing later-UNION-branch INTO restriction unchanged:
`SELECT 1 UNION SELECT 2 INTO #r7 WHERE 2=2;` is accepted by LocalDB PARSEONLY at
levels 100–170 but has a tree error. The grammar-kit issue-128 report distinguishes this
from NOEXEC Msg 196; compile-time rejection is not a syntax verdict.

| Blocks containing only empty statements, including TRY/CATCH bodies | The existing block rules allow zero real statements. Adding anonymous semicolons extends that existing gap to semicolon-only blocks. LocalDB 100–170 returns 102; a future nonempty-body constraint should require a real statement. | `semicolon/block-empty`, `try-empty`; corpus `semicolon.txt` |
| Consecutive bare ENABLE/DISABLE TRIGGER statements without a separator | LocalDB 100–170 returns 102; the semicolon-separated control is accepted. This pre-existing boundary gap needs its own constraint. | `semicolon/trigger-chain`; corpus `semicolon.txt` |
| `SELECT FROM;` (also with a leading `;`) | FROM is read as an identifier when the SELECT expression is absent; the new empty statement support exposes this existing SELECT gap. LocalDB 100–170 returns 156. | `semicolon/invalid-leading-select`; corpus `semicolon.txt` |
| A statement-less BEGIN/END block followed by a CTE | LocalDB 100–170 returns 156 and 102; the grammar accepts the empty block; the CTE after it is now rejected by the `cte-terminator` constraint, so the input fails as SQL Server says, though for that reason rather than the empty block. | `semicolon/cte-after-empty-block`; corpus `semicolon.txt` |

## Table hints: remaining gaps

The table-hint probes (`oracle/probes/table-hints.jsonl`, 165 cases) were measured with LocalDB
17.0.1000.7, PARSEONLY at levels 100–170. INDEX takes a name or a 0..2147483647 integer;
FORCESEEK also takes a non-empty column list. Their numbers allow up to 11 digits (leading zeros
included); SPATIAL_WINDOW_MAX_CELLS allows up to 128 digits, still bounded by 2147483647. WITH
accepts comma, space and mixed separators; TABLE HINT in OPTION requires commas and may omit its
inner hint list.

Names, conflicts, FORCESEEK arguments and DML-target restrictions are enforced by the
`table-hint` constraint (ADR-0003, measured in `oracle/probes/hint-rules.jsonl`). These rejected
cases still have a clean tree:

| Oracle cases | Engine verdict | Disposition |
| --- | --- | --- |
| `table-hints/bare-index`, `table-hints/bare-index-alone` | 1018 | Deprecated hint form without WITH after an alias: INDEX needs WITH there. Needs its own measured rule |
| `table-hints/maxdop-32768`, `table-hints/maxdop-2147483647` | 304 | Query-hint value gap: MAXDOP is 0..32767 |
| `table-hints/maxdop-2147483648`, `table-hints/maxdop-1e0`, `table-hints/maxdop-0x1` | 102 | Query-hint number syntax over-acceptance, outside the table-hint rule |
| `hint-rules/merge-source-nolock`, `hint-rules/merge-source-index` | 156 | `MERGE ... USING src WITH (...) AS s`: the grammar takes hints before the source alias, where SQL Server does not |

## Table and index DDL: measured residual gaps (R10)

The table/index forms in `oracle/probes/ddl-table.jsonl` were measured with the committed
PARSEONLY runner on LocalDB 17.0.1000.7 at levels 100–170. All 90 engine-accepted probes
parse cleanly. Ten rejected probes still parse cleanly; the five existing route rejections
that become clean are also listed below. These are not claims of syntax validity.

| Cases | Engine error | Remaining check |
| --- | --- | --- |
| `ddl-table/column-online-on`, `drop-constraint-online`; routes `alt-tsql-V22-r004`, `alt-tsql-B03-r118`, `r165`, `r166` | 1712; resumable routes also 11905 | Express edition does not support ONLINE/resumable operations. Environment-dependent, outside the grammar. |
| `ddl-table/index-resume` (already clean before this change) | 11905 | Same edition limit. |
| `ddl-table/system-versioning-year` | 13539 | HISTORY_TABLE requires a two-part name; the shared option rule also accepts three parts. A decidable name-shape constraint remains. |
| `ddl-table/index-partition-range`, `index-partition-reverse` | 10737 | These single DATA_COMPRESSION clauses need REBUILD PARTITION=ALL. Do not infer that the descending range itself caused the error. The accepted route r111 also uses XML_COMPRESSION without PARTITION=ALL, so a blanket restriction would contradict the oracle. |
| `ddl-table/index-wrong-partition-option` | 156 | ON PARTITIONS is currently a shared option suffix; restrict its applicability to the compression options in the constraint layer (ADR-0003). |
| `ddl-table/xml-null-schema` | 156 | The shared object-reference rule accepts the reserved word NULL as a schema-collection name. This is a syntax/name-token gap, not a catalog lookup. |
| `ddl-table/plain-keyword-names` (already clean) | 156 | Unquoted reserved-name acceptance in the existing identifier rule; separate identifier/keyword work. |
| `ddl-table/create-duplicate-online` (already clean) | 102 | Duplicate SYSTEM_VERSIONING options; an ADR-0003 uniqueness check remains. |
| `kit-routes/alt-tsql-B04-r040` | 35310 | Columnstore indexes are forbidden on table variables/types. The shared column-definition rule intentionally models their syntax; applicability is decidable by the check layer. |

The selective XML index reference documents a trailing WITH option list, but LocalDB rejects
`FOR (ADD p='/x') WITH (PAD_INDEX=ON)`, the MAXDOP and DROP_EXISTING controls, and route
`alt-tsql-B03-r120` with 319 at every measured level. They stay errors. Conversely, XQUERY
without a type is accepted when followed by MAXLENGTH or SINGLETON, but bare AS XQUERY is
102. Each is pinned in the new corpus. This follows engine evidence, not the broader diagram:
[ALTER INDEX (Selective XML Indexes)](https://learn.microsoft.com/en-us/sql/t-sql/statements/alter-index-selective-xml-indexes?view=sql-server-ver17).

Only LocalDB 2025 was run for these new probes; the committed verdicts from other engines
were replayed, not remeasured. The remaining 309 engine-accepted errors in the complete oracle
are outside this defect family.

## EXTERNAL/FULLTEXT/ENDPOINT probes (R12)

The 184 cases in `oracle/probes/ddl-external.jsonl` were measured on LocalDB 17.0.1000.7,
compatibility levels 100–170. Tree-check applies the option-set rules of ADR-0003; the raw
corpus trees in `test/corpus/ddl-external.txt` also pin the structural trees of constraint
negatives, whose case titles name the constraint. Full-Text component error 7609 and endpoint
edition error 7878 are contextual and do not make valid structure a tree error.

| Construct | Remaining difference | Pinned by |
| --- | --- | --- |
| `DROP EVENT NOTIFICATION ... ON SERVER/DATABASE` after a valid `DROP EXTERNAL TABLE` | The external drop now parses; the following notification DDL is outside the R12 family and remains unmodeled. The whole script is still an engine-accepted tree error. | `kit-routes/alt-tsql-B03-r308` |
| `ALTER EXTERNAL DATA SOURCE ... RESOURCE_MANAGER_LOCATION = ...` for Hadoop | SQL Server 2025 rejects this older documented form with 102. Keep the earlier SQL Server syntax; new probes do not claim older-engine verification. | `kit-routes/alt-tsql-B03-r095` |

## Adding an entry

When a review or a probe finds a new gap, add a row here in the same pass as the corpus case (or
fixture) that pins it, rather than leaving the behavior undocumented until the next audit finds it
again.

## Database and server DDL (R11)

Measured with SQL Server 2025 LocalDB 17.0.1000.7, PARSEONLY at levels 100–170.
The 259 cases in `oracle/probes/ddl-database.jsonl` are pinned in `test/corpus/ddl-database.txt`;
148 engine-accepted cases are clean. The remaining four engine-rejected clean probes are contextual:

| Cases | Verdict | Disposition |
| --- | --- | --- |
| `ddl-database/security-ddl/positive-009` | 15151 | The audit target object is absent or inaccessible; catalog/permissions are outside the grammar. |
| `ddl-database/server/positive-014/015/016` | 534 | MEMORY_OPTIMIZED options are unavailable in this Express edition. The union grammar retains their syntax. |
| `kit-routes/alt-tsql-B03-r031/r034` | 35246 | SQL Server cannot validate the named instance because an engine DLL is unavailable. The replica syntax is retained. |
| `kit-routes/alt-tsql-B03-r160` | 534 | The same MEMORY_OPTIMIZED edition restriction. |

No syntax-negative probe in this set becomes clean. `kit-routes/alt-tsql-B03-r040` remains an error:
LISTENER is not a distributed-group option; its LISTENER_URL neighbour is accepted.
HARDWARE_OFFLOAD ON/OFF and = ON/OFF are rejected by this LocalDB and remain errors despite the syntax reference.

The constraint layer covers the measured file-specification requirements/duplicates, replica requirements,
AG SET/MODIFY single-change rules, nested database option duplicates, QUERY_CAPTURE_POLICY keys/units,
BPE option order and values, audit clause duplicates and encryption ALGORITHM. It does not claim every
unmeasured database/AG option or value is validated. Distributed-group required options, other option
value bounds, and the wider remaining oracle failures are open inventory work.

## Query forms (R13): budget-stopped and preserved gaps

These positive cases are pinned in oracle/probes/query-forms.jsonl and test/corpus/query-forms.txt,
with LocalDB 17.0.1000.7 PARSEONLY verdicts at levels 100–170. They intentionally remain tree errors.
The R13 patch stays within every +10% parser-cost limit. This list does not imply that a scanner
change is needed or that every shape has had a full alternate implementation attempted.

| Positive probe IDs (query-forms/ prefix omitted) | Remaining surface |
| --- | --- |
| open-like-escape | LIKE ... ESCAPE |
| open-json-arrayagg, open-json-objectagg | JSON aggregates with ORDER BY, NULL handling and key:value |
| open-nested-set | A set operation used as another set operation's parenthesized operand |
| open-rowset-semicolon | OPENROWSET provider connection with semicolon-separated credentials |
| open-graph-columns, open-graph-path | Graph pseudo-columns, FOR PATH and recursive MATCH patterns |
| open-kill-query | KILL QUERY NOTIFICATION/SUBSCRIPTION (also KILL STATS JOB in kit routes) |
| open-stats-partitions | UPDATE STATISTICS ... RESAMPLE ON PARTITIONS |
| open-implicit-exec | Implicit procedure invocation at the start of a batch |
| open-signature | ADD [COUNTER] SIGNATURE |
| open-spaced-comparison | Spaced comparison operators: < >, ! >, ! < |
| select-all-top | Explicit ALL before TOP (plain SELECT ALL keeps the existing quantified-expression tree) |
| select-into-later-branch | Existing later-UNION-branch INTO restriction; PARSEONLY accepts it |

BACKUP/RESTORE, R11 server/database DDL and R12 EXTERNAL/FULLTEXT/ENDPOINT statement bodies are outside
this patch. The shared column-default precedence changes only the scalar/NOT NULL boundary; table DDL
corpus, existing tree shapes and its constraint rules remain gates.

## Expression forms (R15)

Recovered forms: language, static. Engine evidence: `oracle/probes/expr-forms.jsonl`,
LocalDB 17.0.1000.7 PARSEONLY, compatibility levels 100–170. The term/field/alias structure
of existing LANGUAGE string arguments is retained. No existing clean tree changes.

Undeclared variables (Msg 137) in the probes remain contextual; syntax is accepted.
LIKE ESCAPE and JSON aggregates remain outside this change.
