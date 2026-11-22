# Accepted gaps

## Current oracle status

The replay covers 10,759 sources and 21,518 session/case pairs. All 15,626
engine-accepted pairs pass tree-check. The [rejected-clean ledger](../oracle/data/rejected-clean.json)
contains 445 pairs (190 ON, 255 OFF): context 111, environment 294, version 36 and runner 4.
There are zero entries classified as `gap` and zero edit histories with incremental differences.

The four runner entries cover two sources in both sessions. The PARSEONLY guard excludes one
source, and the runner's GO splitter accounts for the other. The committed replay and ledger
define the status of the recorded oracle cases. The sections below record historical measurements
and follow-up checks, including gaps outside the recorded oracle.

## R20: historical rejected-but-clean ledger

The historical measurements below are superseded by `oracle/data/rejected-clean.json` for current status.
R20 closes 125 of its original 127 `gap` entries through tree-local constraints, without changing
any grammar, scanner, node kind, field or concrete tree. `gap-forms.jsonl` remeasures all 127 originals
and adjacent accepted/rejected forms at LocalDB 17.0.1000.7 compatibility levels 100-170;
`test/corpus/gap-forms.txt` pins representative trees, including trees whose rejection belongs to
the constraint layer. Empty CATCH, qualified user functions, documented contextual names and
engine-accepted option orders remain accepted.

The two original entries left at R20 were subsequently classified (neither is now a `gap`):

- `ddl-rest/commit-off` (`context`): declaring the transaction variable makes the identical COMMIT WITH clause
  accepted. Rejecting its shape would reject valid programs; resolving the variable needs batch context.
- `kit-routes/alt-tsql-V22-r018` (`environment`): VECTOR_SEARCH needs a preview-enabled engine/database to separate syntax
  from feature availability. The documented source alias is optional; no alias requirement is inferred.

Those final classifications record why the tree cannot decide the rejection: COMMIT requires batch
declarations, and VECTOR_SEARCH requires preview-enabled engine configuration. Other context/environment
observations are classified separately with their measured reason. The grammar still builds the same trees;
consumers needing the validation results must also use the constraint layer (ADR-0003).

## Under-acceptance and unmeasured syntax

The [WRITETEXT BULK probes](../oracle/data/probes/quoted-identifier.jsonl) record LocalDB 17.0.1000.7
at levels 100–170 with QUOTED_IDENTIFIER ON and OFF. The engine accepts BULK with a text pointer
and optional WITH LOG, with the data supplied through TDS. Inline data with BULK is rejected
(Msg 185). Plain WRITETEXT requires an inline data argument (Msg 186 when omitted).

R17 added ALTER MESSAGE TYPE, KILL STATS JOB and KILL QUERY NOTIFICATION SUBSCRIPTION.
The probes in [ddl-rest.jsonl](../oracle/data/probes/ddl-rest.jsonl) and the trees in
[ddl-rest.txt](../test/corpus/ddl-rest.txt) pin their supported forms.

For join hints, removed syntax, EXEC arguments and branch-local ORDER BY, see the
[fixture syntax notes](../test/fixtures/README.md#syntax-notes).

## Over-acceptance resolved in tree-check

Tree-check rejects these shapes in otherwise clean parse trees. The
[batch-rules probes](../oracle/data/probes/batch-rules.jsonl) record LocalDB 17.0.1000.7
PARSEONLY verdicts at levels 100–170 with QUOTED_IDENTIFIER ON and OFF. IDs below
have the `batch-rules/` prefix.

| Construct | Rejected cases | SQL Server verdict |
| --- | --- | --- |
| `CREATE` or `ALTER TRIGGER` with no statement or `EXTERNAL NAME` body | `create-trigger-no-body`, `alter-trigger-no-body`, `trigger-semicolon-only` | Msg 102 |
| `CREATE CERTIFICATE` with a bare or delimited `PROVIDER` source | `certificate-from-provider`, `certificate-from-bracketed-provider`, `certificate-from-bracketed-provider-assignment`, `certificate-from-quoted-provider-assignment` | Msg 102 |
| `RETURNS TABLE (columns)` with an inline or block body | `create-function-table-columns-inline`, `create-function-table-columns-block`, `alter-function-table-columns-inline`, `alter-function-table-columns-block` | Msg 156 |
| `RETURNS TABLE` without columns with an `EXTERNAL NAME` or block body | `create-function-inline-table-external` (156), `create-function-inline-table-block` (102) | Msg 156, 102 |
| `RETURNS @r TABLE (columns)` with an `EXTERNAL NAME` or inline body | `alter-function-table-variable-external`, `create-or-alter-function-table-variable-external` (156), `create-function-table-variable-inline` (102) | Msg 156, 102 |
| A statement in the same batch after a CLR trigger or procedure | `clr-trigger-then-select`, `alter-clr-trigger-then-select`, `clr-proc-then-select`, `alter-clr-proc-then-select` | Msg 156 |
| A statement in the same batch after a CLR function | `clr-function-then-select`, `alter-clr-function-then-select` | Msg 156 |

`create-procedure-no-body`, `create-trigger-select`, `alter-trigger-select`, `trigger-external-name`,
`certificate-from-file` and `asymmetric-key-from-provider` pass. The accepted TVF pairings are
`RETURN` followed by a query for inline `RETURNS TABLE`, a block for `RETURNS @r TABLE (columns)`,
and `EXTERNAL NAME` for `RETURNS TABLE (columns)`. `GO` lets a new statement follow any of the three
CLR modules, and a T-SQL trigger can contain multiple statements.

## FROM-less clauses: measured residual gaps

WHERE, GROUP BY and HAVING do not require FROM. SQL Server 2025 LocalDB 17.0.1000.7,
PARSEONLY at levels 100–170, accepts both error tests inherited from 1.0.0 `SELECT 1 WHERE 1=1;`
and `SELECT 1 GROUP BY 1;`. They are positive cases in
`test/corpus/select-fromless.txt` (ADR-0002 §9); the other existing error cases are unchanged.

The listed FROM-less forms produce clean raw trees. The dispositions below identify tree-check
rejections separately from the raw parser result.

| Cases | SQL Server verdict | Classification and disposition |
| --- | --- | --- |
| `count-empty`, `mut-02235`, `mut-02255` | Msg 174 on every committed engine/level | Rejected by the `builtin-arity` constraint (ADR-0003, ADR-0004) |
| `regex-79` | Msg 4145 on SQL Server 2017–2022 and 2025 levels 100–160; Msg 156 on both 2025 engines at level 170 | Rejected by the `scalar-predicate` constraint (a scalar function call as the WHERE condition) |
| `regex-80` | Msg 4145 on 2025; 195 on older engines | Rejected by the `scalar-predicate` constraint |
| `qi-on-contains-search`, `qi-on-freetext-search`, `qi-on-property`, `qi-on-language` | Msg 102 on every committed engine/level | Tree-check rejects these forms. `gap-fulltext-string` covers CONTAINS and FREETEXT search strings and the PROPERTY argument. `expression-fulltext-language` covers LANGUAGE. The matching `qi-off-*` controls set QUOTED_IDENTIFIER OFF in the source and pass. |
| `SELECT NOT 1 WHERE 1=1;` | LocalDB Msg 156 at levels 100–170 | Open gap. The FROM-less example and the corresponding FROM-bearing form both produce clean trees and pass tree-check. The historical LocalDB result refers to the FROM-less example. Neither form has a committed oracle case for this discrepancy. |
| `SELECT 1 WHERE CURRENT OF c;` | LocalDB Msg 102 at levels 100–170 | Open gap. The FROM-less SELECT and the corresponding FROM-bearing SELECT both produce clean trees and pass tree-check. The historical LocalDB result refers to the FROM-less example. Positioned UPDATE and DELETE have separate oracle coverage. |

R18 also recovers the later-UNION-branch INTO form:
`SELECT 1 UNION SELECT 2 INTO #r7 WHERE 2=2;` is accepted by LocalDB PARSEONLY at
levels 100–170 and now parses cleanly. The grammar-kit issue-128 report distinguishes this
from NOEXEC Msg 196; compile-time rejection is not a syntax verdict.

| Construct | LocalDB verdict and current tree-check rule | Evidence |
| --- | --- | --- |
| Blocks containing only empty statements, including the measured TRY body | Msg 102. `gap-statement-shape` requires a real statement in these bodies. | `semicolon/block-empty`, `semicolon/try-empty`; `test/corpus/semicolon.txt` |
| Consecutive bare ENABLE/DISABLE TRIGGER statements without a separator | Msg 102. `gap-statement-shape` rejects the chain. The semicolon-separated control passes. | `semicolon/trigger-chain`; `test/corpus/semicolon.txt` |
| `SELECT FROM;` with or without a leading semicolon | Msg 156. `gap-reserved-name` rejects FROM in the expression position. | `semicolon/invalid-leading-select`, `semicolon/control-select-from`; `test/corpus/semicolon.txt` |
| An empty BEGIN/END block followed by a CTE | Msg 156 and 102. Tree-check reports both `gap-statement-shape` and `cte-terminator`. | `semicolon/cte-after-empty-block`; `test/corpus/semicolon.txt` |

## Table hints: constraints on clean raw trees

The table-hint probes (`oracle/data/probes/table-hints.jsonl`, 165 cases) were measured with LocalDB
17.0.1000.7, PARSEONLY at levels 100–170. INDEX takes a name or a 0..2147483647 integer;
FORCESEEK also takes a non-empty column list. Their numbers allow up to 11 digits (leading zeros
included); SPATIAL_WINDOW_MAX_CELLS allows up to 128 digits, still bounded by 2147483647. WITH
accepts comma, space and mixed separators; TABLE HINT in OPTION requires commas and may omit its
inner hint list.

The `table-hint` constraint checks the names, conflicts, FORCESEEK arguments and DML-target
restrictions measured in `oracle/data/probes/hint-rules.jsonl`. The following engine-rejected
cases still have clean raw trees.

| Oracle cases | LocalDB verdict | Disposition |
| --- | --- | --- |
| `table-hints/bare-index`, `table-hints/bare-index-alone` | 1018 | `gap-option-shape` rejects INDEX without WITH in the measured post-alias forms. |
| `table-hints/maxdop-32768`, `table-hints/maxdop-2147483647` | 304 | `gap-option-shape` enforces the MAXDOP bound of 0–32767. |
| `table-hints/maxdop-2147483648`, `table-hints/maxdop-1e0`, `table-hints/maxdop-0x1` | 102 | `gap-option-shape` rejects out-of-range, exponential and hexadecimal MAXDOP values. The recorded SQL Server 2017–2022 runs report Msg 1080 for `maxdop-2147483648`. |
| `hint-rules/merge-source-nolock`, `hint-rules/merge-source-index` | 156 | `gap-option-shape` rejects the measured MERGE source hints before the source alias. |

## Table and index DDL: R10 measurements and current checks

At R10, LocalDB 17.0.1000.7 at levels 100–170 accepted 90 probes in
`oracle/data/probes/ddl-table.jsonl`. All 90 produced clean trees. Ten rejected probes and
five route rejections also produced clean raw trees at that round. The table records their
dispositions.

| Cases | Engine error | Disposition |
| --- | --- | --- |
| `ddl-table/column-online-on`, `drop-constraint-online`; routes `alt-tsql-V22-r004`, `alt-tsql-B03-r118`, `r165`, `r166` | 1712; resumable routes also 11905 | Express edition does not support ONLINE/resumable operations. Environment-dependent, outside the grammar. |
| `ddl-table/index-resume` (already clean before this change) | 11905 | Same edition limit. |
| `ddl-table/system-versioning-year` | 13539 | `gap-name-parts` rejects the measured three-part HISTORY_TABLE name. |
| `ddl-table/index-partition-range`, `index-partition-reverse` | 10737 | `gap-option-shape` rejects these DATA_COMPRESSION clauses without REBUILD PARTITION=ALL. The accepted XML_COMPRESSION control is `alt-tsql-B03-r111`. |
| `ddl-table/index-wrong-partition-option` | 156 | `gap-option-shape` rejects ON PARTITIONS on the measured MAXDOP option. |
| `ddl-table/xml-null-schema` | 156 | `gap-name-parts` rejects the unquoted NULL schema-collection name. |
| `ddl-table/plain-keyword-names` (already clean) | 156 | `gap-reserved-name` rejects the measured unquoted reserved names. |
| `ddl-table/create-duplicate-online` (already clean) | 102 | `gap-option-shape` rejects duplicate SYSTEM_VERSIONING options. |
| `kit-routes/alt-tsql-B04-r040` | 35310 | `gap-option-shape` rejects columnstore indexes on table variables and table types in the measured route. |

The selective XML index reference documents a trailing WITH option list, but LocalDB rejects
`FOR (ADD p='/x') WITH (PAD_INDEX=ON)`, the MAXDOP and DROP_EXISTING controls, and route
`alt-tsql-B03-r120` with 319 at every measured level. They stay errors. Conversely, XQUERY
without a type is accepted when followed by MAXLENGTH or SINGLETON, but bare AS XQUERY is
102. Each is pinned in the new corpus. This follows engine evidence, not the broader diagram:
[ALTER INDEX (Selective XML Indexes)](https://learn.microsoft.com/en-us/sql/t-sql/statements/alter-index-selective-xml-indexes?view=sql-server-ver17).

At R10, the new probes were measured on LocalDB 2025, and the existing verdicts from other
engines were replayed. The complete oracle still had 309 engine-accepted errors outside the
R10 defect family at that round.

## EXTERNAL/FULLTEXT/ENDPOINT probes (R12)

The 184 cases in `oracle/data/probes/ddl-external.jsonl` were measured on LocalDB 17.0.1000.7,
compatibility levels 100–170. Tree-check applies the option-set rules of ADR-0003; the raw
corpus trees in `test/corpus/ddl-external.txt` also pin the structural trees of constraint
negatives, whose case titles name the constraint. Full-Text component error 7609 and endpoint
edition error 7878 are contextual and do not make valid structure a tree error.

| Construct | Disposition | Oracle case |
| --- | --- | --- |
| `DROP EVENT NOTIFICATION ... ON SERVER/DATABASE` after a valid `DROP EXTERNAL TABLE` | R12 recovered the external drop. R17 added DROP EVENT NOTIFICATION, and the complete case now passes tree-check. | `kit-routes/alt-tsql-B03-r308` |
| `ALTER EXTERNAL DATA SOURCE ... RESOURCE_MANAGER_LOCATION = ...` for Hadoop | The raw tree retains the documented Hadoop form. Tree-check rejects the measured case with `gap-option-shape`, matching Msg 102 on every committed engine and level. | `kit-routes/alt-tsql-B03-r095` |

## Recording a newly measured case

Add an oracle case and its engine verdict, and pin the tree or checker behaviour in the relevant
test. Update the replay baseline for a verified change and classify engine-rejected cases that
remain clean in `oracle/data/rejected-clean.json`. Record a reproduction and any measurement
limits for cases outside the committed oracle.

## Database and server DDL (R11)

Measured with SQL Server 2025 LocalDB 17.0.1000.7, PARSEONLY at levels 100–170.
The 259 cases in `oracle/data/probes/ddl-database.jsonl` are pinned in `test/corpus/ddl-database.txt`.
LocalDB accepted 148 probes. Four LocalDB-rejected probes still pass tree-check because of catalog,
permissions or edition limits. The table also records three groups of kit routes.

| Cases | Verdict | Disposition |
| --- | --- | --- |
| `ddl-database/security-ddl/positive-009` | 15151 | The audit target object is absent or inaccessible; catalog/permissions are outside the grammar. |
| `ddl-database/server/positive-014/015/016` | 534 | MEMORY_OPTIMIZED options are unavailable in this Express edition. The union grammar retains their syntax. |
| `kit-routes/alt-tsql-B03-r031/r034` | 35246 | SQL Server cannot validate the named instance because an engine DLL is unavailable. The replica syntax is retained. |
| `kit-routes/alt-tsql-B03-r160` | 534 | The same MEMORY_OPTIMIZED edition restriction. |

Tree-check rejects `kit-routes/alt-tsql-B03-r040` with `distributed-availability-option`. The raw
tree accepts LISTENER in the distributed group, and LocalDB returns Msg 155. The LISTENER_URL
control passes.
HARDWARE_OFFLOAD ON/OFF and = ON/OFF are rejected by this LocalDB and remain errors despite the syntax reference.

The constraint layer covers the measured file-specification requirements/duplicates, replica requirements,
AG SET/MODIFY single-change rules, nested database option duplicates, QUERY_CAPTURE_POLICY keys/units,
BPE option order and values, audit clause duplicates and encryption ALGORITHM. R11 left distributed-group
required options, other option value bounds and additional engine-accepted errors for later rounds.

## Query forms deferred at R13

The cases in `oracle/data/probes/query-forms.jsonl` and `test/corpus/query-forms.txt` were measured
with LocalDB 17.0.1000.7 PARSEONLY at levels 100–170. They were tree errors at R13 and now pass
tree-check after R17 and R18. The owner approved going over the +10% per-change limit for
PR #33, which increased `parser.c` size by 10.32%.

| Positive probe IDs (query-forms/ prefix omitted) | Form deferred at R13 |
| --- | --- |
| open-like-escape | LIKE ... ESCAPE |
| open-json-arrayagg, open-json-objectagg | JSON aggregates with ORDER BY, NULL handling and key:value |
| open-graph-columns, open-graph-path | Graph pseudo-columns, FOR PATH and recursive MATCH patterns |
| open-kill-query | KILL QUERY NOTIFICATION/SUBSCRIPTION (also KILL STATS JOB in kit routes) |
| open-stats-partitions | UPDATE STATISTICS ... RESAMPLE ON PARTITIONS |
| open-implicit-exec | Implicit procedure invocation at the start of a batch |
| open-signature | ADD [COUNTER] SIGNATURE |
| select-all-top | Explicit ALL before TOP (plain SELECT ALL keeps the existing quantified-expression tree) |
| select-into-later-branch | Existing later-UNION-branch INTO restriction; PARSEONLY accepts it |

BACKUP/RESTORE, R11 server/database DDL and R12 EXTERNAL/FULLTEXT/ENDPOINT statement bodies are outside
this patch. The shared column-default precedence changes only the scalar/NOT NULL boundary; table DDL
corpus, existing tree shapes and its constraint rules remain gates.

## Expression forms (R15)

Recovered forms: language, static, comparison, set, provider. Engine evidence: `oracle/data/probes/expr-forms.jsonl`,
LocalDB 17.0.1000.7 PARSEONLY, compatibility levels 100–170. The term/field/alias structure
of existing LANGUAGE string arguments is retained. No existing clean tree changes.

Spaced comparisons retain the joined operator kinds. The external scanner consumes the complete
operator including whitespace and nested/line comments; comments inside it are not separate nodes.
The comparison-word and scalar-comparison probes retain the existing boolean/scalar contextual gap.

`expr-forms/set/intoLast` was deferred at R15 and is recovered by R18.
Nested set operands recover on both sides; operand-local trailing clauses are constraint violations.

Undeclared variables (Msg 137) in the probes remain contextual; syntax is accepted.
LIKE ESCAPE and JSON aggregates remain outside this change.

## Remaining routes (R17)

R17 recovers all 119 remaining engine-accepted kit routes in scope, including the six R13 open query forms listed above. Positivity merges every committed engine and compatibility level. At R17, LIKE ESCAPE (B01-r006) and JSON aggregates (V22-r014) were excluded; R18 recovers both. Evidence: oracle/data/probes/ddl-rest.jsonl and LocalDB 17.0.1000.7 levels 100–170; the committed Linux verdicts identify edition-limited positive routes.

At R17, 328 of the 533 neighbour probes were LocalDB positives and passed tree-check. The two
permission-name negatives produced clean trees at R17 and are now rejected by
`gap-statement-shape`. The undeclared-variable cases `routes-implicit-exec-args`, `commit-off` and
`issue151-implicit-local` remain classified as context.

Closing probes reject scalar XE source parentheses, database/server-qualified DROP SEQUENCE names,
measured CONNECT ON AVAILABILITY GROUP combinations, and quoted multiword class qualifiers. At
R17, the `graph_match.sql` fixture was corrected to use grouped SHORTEST_PATH and OR of complete
MATCH predicates. LocalDB returned Msg 102 for the original and shortest-path-only correction and
accepted the final fixture at every measured level. Original fixture bytes remain in the
tree-preservation snapshot inputs.

PARSEONLY accepts PRIORITY_LEVEL = 11, several spatial grid/bounding-box forms, and an application role without PASSWORD; no guessed restrictions are added. Event-session clause reordering beyond the fixed ADD/DROP sequence remains a pre-existing gap. Later set-operation INTO was deferred at R17 and is recovered by R18.


## Final engine-accepted forms (R18)

All seven deferred engine-positive LIKE ESCAPE, JSON aggregate and later set-branch INTO cases
are clean. LocalDB 17.0.1000.7, levels 100-170, `last-forms/*` probes and the committed verdicts
verify arguments, suffix ordering, nested forms and invalid neighbours. Every existing clean
raw tree is preserved; ESCAPE adds an existing binary_expression with the LIKE predicate on
its left and the escape expression on its right. JSON pairs and clauses reuse existing nodes.

PARSEONLY accepts INTO on any set branch, including several branches in one set operation.
Do not impose a single-INTO limit. Subquery/CTE, INSERT-source and view-body INTO are rejected
by `query-select-into-scope`. JSON_ARRAYAGG supports ORDER BY, NULL handling, RETURNING json
and windows subject to the measured combinations; JSON_OBJECTAGG takes one key:value pair,
no argument ORDER BY or DISTINCT, and permits NULL handling, RETURNING json and windows.
Bare JSON_ARRAYAGG(DISTINCT x) and WITHIN GROUP are accepted by PARSEONLY, so they remain allowed.

Tree-check rejects the five measured ESCAPE operand negatives with `expression-like-escape`:
`escape-boolean`, `escape-and_operand`, `escape-is-null`, `escape-in` and
`escape-operand-comparison-group`. It rejects `escape-keyword-column` and `escape-alias` with
`gap-reserved-name`. These cases still have clean raw trees. Quoted `[escape]` passes.
No name, type or boolean/scalar resolution is inferred from a syntax tree.
