# grammar-kit's T-SQL queries

The six query files here come from tree-sitter-grammar-kit (MIT, see `oracle/data/kit/NOTICE`):
`src/testdata/native/queries/tsql.json`. They are imported under ADR-0004.

`q.r6-structure` is adapted to this grammar in three places, each with a comment in the file:
- a restored database has no field;
- `MOVE ... TO ...` is an `option`;
- a branch `ORDER BY` follows its `query_specification` instead of being its child.

The other five files are unchanged.

`test/queries/kit-cases.json` names the oracle source for each case (`oracle/data/probes/kit-routes.jsonl`) and the edit
step (`oracle/data/kit/route-edits.jsonl`). It also records the captures, measured on this grammar.
`node scripts/query-check.mjs --tree-sitter <cli>` fails when they change, and `--update` records them again.

At import, seven of the eight case-step capture sets matched grammar-kit's expectations. The difference was
`tsql-r6-query-r1` at step 0, using `kit-routes/tsql-r6-feature-r1`. Every recorded engine rejected that source,
and the case note explains why its error-tree captures are retained.
