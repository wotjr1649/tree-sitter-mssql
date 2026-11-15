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
sub-forms of deprecated or console statements (`WRITETEXT BULK ...`, `BACKUP ... MIRROR TO`,
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

## Table hints: structure and deferred checks

The R9 table-hint probes (`oracle/probes/table-hints.jsonl`, 165 cases) were measured with
LocalDB 17.0.1000.7, PARSEONLY at compatibility levels 100–170. INDEX takes a name or
0..2147483647 integer; FORCESEEK additionally takes a non-empty column list. Their numeric
tokens allow up to 11 digits (including leading zeros); SPATIAL_WINDOW_MAX_CELLS allows
up to 128 digits, still bounded by 2147483647. Value, sign, decimal/exponent/hex and length
boundaries are corpus-tested. WITH accepts comma, space and mixed hint separators;
TABLE HINT in OPTION requires commas and may omit its inner hint list.

The grammar models hint structure; these rejected cases still have a clean tree. No new
hint constraint rule is installed by this change.

| Oracle cases | Engine verdict | Disposition |
| --- | --- | --- |
| `force-seek-and-index` | 10747 | ADR-0003: parameterized FORCESEEK and INDEX are mutually exclusive; pending constraint rule |
| `query-hint-control-004`, `table-hints/conflict` | 1047 | ADR-0003: conflicting isolation hints; pending constraint rule |
| `force-seek-zero`, `query-hint-control-006`, `table-hints/seek-zero` | 10749 | Context/value check: FORCESEEK cannot use heap index 0, although INDEX can; keep the shared integer shape |
| `query-hint-control-005`, `table-hints/unknown`, `table-hints/spatial-double-zero-hex`, `table-hints/spatial-one-hex` | 321 | Context hint-name check; 00x1 and 1x1 are a decimal token followed by the unknown hint x1, unlike rejected hexadecimal 0x1 |
| `table-hints/bare-index`, `table-hints/bare-index-alone` | 1018 | Unresolved deprecated-form restriction: INDEX needs WITH here. The existing bare INDEX structure is retained; not a name-binding error |
| `table-hints/update-index`, `table-hints/delete-index` | 1069 | Pending statement/hint applicability check: the target cannot use INDEX |
| `table-hints/update-seek`, `table-hints/delete-seek` | 10724 | Pending statement/hint applicability check: the target cannot use FORCESEEK |
| `table-hints/update-scan`, `table-hints/delete-scan` | 10745 | Existing statement/hint applicability gap for FORCESCAN |
| `table-hints/maxdop-32768`, `table-hints/maxdop-2147483647` | 304 | Existing query-hint value gap: MAXDOP range is 0..32767; the generic query-hint rule is unchanged |
| `table-hints/maxdop-2147483648`, `table-hints/maxdop-1e0`, `table-hints/maxdop-0x1` | 102 | Existing query-hint syntax over-acceptance, outside the table-hint rule; not explained by ADR-0002 context |

Five existing oracle negatives turn error to clean: `force-seek-zero`, `force-seek-and-index`,
and `query-hint-control-004/005/006`. Eight new probes do so against the same main parser:
`table-hints/seek-zero`, `bare-index`, `update-index`, `update-seek`, `delete-index`,
`delete-seek`, `spatial-double-zero-hex` and `spatial-one-hex` (all share the `table-hints/`
prefix). The other ten clean negative probes were already clean on main. Each probe is pinned
in `test/corpus/table-hints.txt`; these are documented residual gaps, not syntax success claims.

## Adding an entry

When a review or a probe finds a new gap, add a row here in the same pass as the corpus case (or
fixture) that pins it, rather than leaving the behavior undocumented until the next audit finds it
again.
