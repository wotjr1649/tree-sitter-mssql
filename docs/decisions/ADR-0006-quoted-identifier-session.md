# ADR-0006 — Per-session QUOTED_IDENTIFIER checking

- Status: accepted
- Date: 2026-11-16
- Evidence: LocalDB 17.0.1000.7, compatibility levels 100–170,
  `oracle/probes/quoted-identifier.jsonl` and its `.qi-on.json` / `.qi-off.json` verdicts

## Context

The same `"text"` token is a name under QUOTED_IDENTIFIER ON and a string under OFF. The public grammar represents it
as an identifier. `RAISERROR("message",16,1)`, `WAITFOR DELAY "00:00:01"`, `EXEC ("SELECT 1")` and similar forms are
accepted under OFF, but they sit in string-only positions, so the existing tree can contain an ERROR. Conversely,
`FROM "t"` and `CREATE TABLE "t"` are rejected under OFF. Adding only constraints, or ignoring the existing ERROR,
cannot judge both directions correctly.

## Decision

The public grammar is not split by session. It keeps its node kinds, fields and the original parse tree. `tree-check`
interprets the initial session state and the SET switches inside the source. At OFF positions it replaces each closed
double-quoted token with a single-quoted string of the same value, and parses that internal check source again with the
same grammar. A `""` escape becomes `"`, and an inner apostrophe becomes `''`. If there is no token to convert, it
checks the original tree. It does not ignore original ERRORs wholesale; it checks errors, missing nodes, ranges and
constraints of the whole internal source.

This internal reparse is needed to represent string-only positions. We did not choose to add a double-quoted literal
alternative to the public grammar, because it could change the identifier/literal choice and conflicts in existing ON
trees. A consumer's tree and incremental parsing state are not modified.

Separately, this fixes a gap in the existing ON grammar that the probes found. The engine accepts
`BULK INSERT ... FROM "path"` under both ON and OFF, so a double-quoted path alternative is added at this position and
aliased to the existing `literal`. The `ON`, `FILESTREAM_ON` and `MOVE TO` values of filegroup placement also accept
single- and double-quoted strings under both settings. It is not an exception only for `"default"` and `"NULL"`. A probe
of 6 placement forms × 8 values confirmed that ordinary names such as `"fg"` and `"PRIMARY"`, and case and
single-quote variants, are all accepted, so a single-quoted string alternative is added at that position. The OFF
projection uses the same grammar. This local change creates no new node kind or field.

The following syntax constraints were also confirmed on the same engine and levels. The rejection of a name reference
in PRINT (128) is classified as a contextual case that binds a name. We do not add a broad PRINT rule to support string
sessions.

| Rule | Measured boundary |
| --- | --- |
| `gap-call-shape` | The first argument of CONVERT does not accept a string type name (102). |
| `quoted-identifier-string` | The first string argument of a RAISERROR message, WAITFOR DELAY/TIME and an EXEC dynamic body does not accept a double-quoted name under ON (102). Ordinary EXEC procedure arguments are not covered by this rule. |
| `collation-name` | A COLLATE name is written without quotes or brackets (102). |
| `bulk-path-empty` | An empty BULK INSERT path is rejected (1038). |
| `filegroup-name-empty` | The ON filegroup, MOVE/FILESTREAM_ON in DROP, and constraint MOVE reject an empty name (1038). An empty FILESTREAM_ON string in CREATE INDEX and column INDEX is accepted. |

Every converted byte keeps its original offset, and `first_problem`, constraint positions, `root_end` and `len` are
returned in original UTF-8 byte coordinates. So the error count and node kinds in the JSON describe the internal check
tree of the selected session, while positions and lengths point into the original source the caller passed. The ranges
of `--keyword-ranges` are always taken from the public original tree, because they are used for mutation.

## Session and boundaries

The CLI takes `--quoted-identifier on|off`, and ON is the default if it is omitted. The existing Rust `inspect` also
keeps ON as its default, and `inspect_with_quoted_identifier` sets the initial state. A `SET QUOTED_IDENTIFIER ON/OFF`
in the source applies to later tokens and persists after GO. SET text inside a comment, a bracketed name or a string
does not switch the setting. SQLCMD client lines also keep their original text.

Procedure, function and trigger definitions capture the setting at definition time. Internal SET switches are not
applied from the module definition in the public tree to the end of that batch. In the engine probe, both the ON and OFF
cases inside a procedure are accepted with an initial ON, and with an initial OFF `FROM "t"` is rejected with 102. A SET
inside a dynamic SQL string does not change the outer state. As before, the contents of a dynamic string are not executed
or checked recursively. Names, permissions, types and the execution environment of stored modules are contextual.

The ON requirements of indexed views, filtered/computed indexes and XML methods apply at execution and binding time.
These probes are accepted by PARSEONLY under both settings, so OFF alone is not a reason to reject them. An unclosed
quote is not converted and remains a syntax error.

## WASM ABI and gate

The ABI version in the identity is 2. The existing `tc_check(ptr,len)` keeps the initial ON. The third field of
`tc_check_v2(ptr,len,quoted_identifier)` is `0=ON`, `1=OFF`, and any other value returns handle 0 without an SQL verdict.
The ownership rules for input and results are the same. The session is passed on each call and does not carry over to
the next call. For the detailed contract, follow [constraint-wasm.md](../constraint-wasm.md).

The replay reads `session_set.quoted_identifier` from the oracle meta and interprets existing files as ON. Existing case
IDs stay for ON, and the OFF baseline is distinguished by `@qi-off`. WASM parity checks every oracle, fixture and edit
history input in both ON and OFF and compares the whole JSON except the path.
