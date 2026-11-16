# grammar-kit's T-SQL queries

The six query files here come from tree-sitter-grammar-kit (MIT, see `oracle/kit/NOTICE`):
`src/testdata/native/queries/tsql.json`. They are imported under ADR-0004.

`q.r6-structure` is adapted to this grammar in three places, each with a comment in the file:
- a restored database has no field;
- `MOVE ... TO ...` is an `option`;
- a branch `ORDER BY` follows its `query_specification` instead of being its child.

The other five files are unchanged.

`test/queries/kit-cases.json` names the oracle source for each case (`oracle/probes/kit-routes.jsonl`) and the edit
step (`oracle/kit/route-edits.jsonl`). It also records the captures, measured on this grammar.
`node tools/query-check.mjs --tree-sitter <cli>` fails when they change, and `--update` records them again.

grammar-kit's own expected captures were compared once, at import: 7 of the 8 case steps match as sets. The eighth
source is rejected by every engine, and its case carries a note saying so.
