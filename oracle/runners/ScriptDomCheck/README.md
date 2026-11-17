# ScriptDOM second opinion

[ScriptDOM](https://github.com/microsoft/SqlScriptDOM) is Microsoft's T-SQL parser library (MIT), the one SSMS and
DacFx use. It is not the engine. SQL Server's `SET PARSEONLY ON` verdicts in `oracle/data/verdicts` stay the truth
(ADR-0002). ScriptDOM is a second opinion used to read the cases where the two disagree with the grammar.

- `ScriptDomCheck/` is a .NET 10 tool, pinned by `packages.lock.json` (Microsoft.SqlServer.TransactSql.ScriptDom
  180.154.0). It parses every case of a set at each level with the matching parser (`TSql100Parser` …
  `TSql170Parser`, selected initial QUOTED_IDENTIFIER setting) and writes the parse error numbers in the shape of the engine verdict files.
  These files carry `"mode": "SCRIPTDOM"`, so the replay never reads them as engine verdicts.
- `verdicts/<set>.json` retain the historical ON observations. The new quoted-identifier set uses
  `verdicts/quoted-identifier.qi-on.json`; `verdicts/<set>.qi-off.json` hold OFF observations for every set.
  `--quoted-identifier on|off` selects the session (default ON); metadata records numeric `quoted_identifier`.
- `node scripts/scriptdom-compare.mjs` counts each engine/ScriptDOM/tree cell. It lists the cases where both parsers
  reject but the tree is clean, and fails if one of them has no class in `oracle/data/rejected-clean.json`.
  It keeps ON and OFF acceptance separate and prints a table per session, using the replay's `@qi-off` IDs.

To refresh after the case sets change:

```sh
dotnet restore --locked-mode oracle/runners/ScriptDomCheck
dotnet build -c Release --no-restore oracle/runners/ScriptDomCheck
dotnet oracle/runners/ScriptDomCheck/bin/Release/net10.0/ScriptDomCheck.dll --cases oracle/data/kit/cases.jsonl --out oracle/data/scriptdom-verdicts/kit.json
# and once per oracle/data/probes/<set>.jsonl -> oracle/data/scriptdom-verdicts/<set>.json
```

Result at its introduction (10,300 cases, with the Linux engine verdicts of #46):

| cell | cases |
| --- | ---: |
| engine accepts, ScriptDOM accepts, tree clean | 7,116 |
| engine accepts, ScriptDOM rejects, tree clean (ScriptDOM gaps) | 388 |
| engine rejects, ScriptDOM accepts, tree error (constraint-layer rejections ScriptDOM does not make) | 724 |
| engine rejects, ScriptDOM accepts, tree clean (context-dependent, classified) | 168 |
| engine rejects, ScriptDOM rejects, tree clean | 21 |
| engine rejects, ScriptDOM rejects, tree error | 1,883 |

All 21 both-reject-but-clean cases are already classified. They are Azure Synapse and Data Warehouse syntax that the
grammar keeps from 1.0.0 (CTAS distribution, data-warehouse transactions, Azure-only database options), platform
limits (MEMORY_OPTIMIZED server settings) or features these engine builds lack. ScriptDOM pointed at no new grammar
defect.
