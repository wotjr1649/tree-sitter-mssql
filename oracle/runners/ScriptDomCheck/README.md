# ScriptDOM second opinion

[ScriptDOM](https://github.com/microsoft/SqlScriptDOM) is Microsoft's T-SQL parser library (MIT), the one SSMS and
DacFx use. It is not the engine. SQL Server's `SET PARSEONLY ON` verdicts in `oracle/data/verdicts` stay the truth
(ADR-0002). ScriptDOM is a second opinion used to read the cases where the two disagree with the grammar.

- `ScriptDomCheck/` is a .NET 10 tool, pinned by `packages.lock.json` (Microsoft.SqlServer.TransactSql.ScriptDom
  180.154.0). It parses every case of a set at each level with the matching parser (`TSql100Parser` …
  `TSql170Parser`, selected initial QUOTED_IDENTIFIER setting) and writes the parse error numbers in the shape of the engine verdict files.
  These files carry `"mode": "SCRIPTDOM"`, so the replay never reads them as engine verdicts.
- Verdicts live in `oracle/data/scriptdom-verdicts/`. ON uses `<set>.json` except
  `quoted-identifier.qi-on.json`. OFF uses `<set>.qi-off.json`. `--quoted-identifier on|off` selects
  the initial session (default ON). The runner records numeric `quoted_identifier` and the
  source-set hash.
- `node scripts/scriptdom-compare.mjs` compares the cases present in the SQL Server, ScriptDOM
  and tree-check results, separately for ON and OFF. It fails on an unclassified case that both
  SQL Server and ScriptDOM reject and tree-check accepts. When refreshing a case set, verify
  `meta.cases_sha256` against the current source file and check that all case IDs are present in
  both sessions. The comparison joins by ID and skips missing observations. It does not validate
  source hashes.

Refresh both sessions after the case sets change. From the repository root:

```sh
dotnet restore --locked-mode oracle/runners/ScriptDomCheck
dotnet build -c Release --no-restore oracle/runners/ScriptDomCheck
runner=oracle/runners/ScriptDomCheck/bin/Release/net10.0/ScriptDomCheck.dll
for input in oracle/data/kit/cases.jsonl oracle/data/probes/*.jsonl; do
  set_name=$(basename "$input" .jsonl)
  if [ "$set_name" = cases ]; then set_name=kit; fi
  on_suffix=
  if [ "$set_name" = quoted-identifier ]; then on_suffix=.qi-on; fi
  out="oracle/data/scriptdom-verdicts/$set_name"
  dotnet "$runner" --cases "$input" --out "$out$on_suffix.json" --quoted-identifier on || exit
  dotnet "$runner" --cases "$input" --out "$out.qi-off.json" --quoted-identifier off || exit
done
node scripts/scriptdom-compare.mjs
```

Result at its introduction (10,300 cases, with the Linux engine verdicts of #46):

| cell | cases |
| --- | ---: |
| engine accepts, ScriptDOM accepts, tree clean | 7,116 |
| engine accepts, ScriptDOM rejects, tree clean (ScriptDOM gaps) | 388 |
| engine rejects, ScriptDOM accepts, tree error (tree-check rejections) | 724 |
| engine rejects, ScriptDOM accepts, tree clean (context-dependent, classified) | 168 |
| engine rejects, ScriptDOM rejects, tree clean | 21 |
| engine rejects, ScriptDOM rejects, tree error | 1,883 |

All 21 both-reject-but-clean cases are already classified. They are Azure Synapse and Data Warehouse syntax that the
grammar keeps from 1.0.0 (CTAS distribution, data-warehouse transactions, Azure-only database options), platform
limits (MEMORY_OPTIMIZED server settings), features these engine builds lack, or an example missing its variable declarations. ScriptDOM pointed at no new grammar
defect.
