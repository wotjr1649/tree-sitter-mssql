// Second opinion for the oracle: parse every case with Microsoft's ScriptDOM parser at each compatibility level and
// record its parse errors, in the shape of the engine verdict files. ScriptDOM is not the engine: these files are kept
// apart from oracle/data/verdicts and only used to find cases where ScriptDOM and the engine disagree.
//
//   ScriptDomCheck --cases oracle/data/probes/x.jsonl --out oracle/data/scriptdom-verdicts/x.json [--levels 100,...] [--quoted-identifier on|off]
using System.Security.Cryptography;
using System.Text;
using System.Text.Json.Nodes;
using Microsoft.SqlServer.TransactSql.ScriptDom;

static class ScriptDomCheck
{
    static readonly int[] AllLevels = [100, 110, 120, 130, 140, 150, 160, 170];

    // The parser for a compatibility level and initial QUOTED_IDENTIFIER session setting.
    static TSqlParser Parser(int level, bool quotedIdentifier) => level switch
    {
        100 => new TSql100Parser(quotedIdentifier),
        110 => new TSql110Parser(quotedIdentifier),
        120 => new TSql120Parser(quotedIdentifier),
        130 => new TSql130Parser(quotedIdentifier),
        140 => new TSql140Parser(quotedIdentifier),
        150 => new TSql150Parser(quotedIdentifier),
        160 => new TSql160Parser(quotedIdentifier),
        170 => new TSql170Parser(quotedIdentifier),
        _ => throw new ArgumentException($"no ScriptDOM parser for level {level}"),
    };

    static int Main(string[] args)
    {
        var opts = new Dictionary<string, string>();
        for (var i = 0; i + 1 < args.Length; i += 2) opts[args[i]] = args[i + 1];
        if (!opts.TryGetValue("--cases", out var casesFile) || !opts.TryGetValue("--out", out var outFile))
        {
            Console.Error.WriteLine("usage: ScriptDomCheck --cases FILE --out FILE [--levels L,...] [--quoted-identifier on|off]");
            return 2;
        }
        var mode = opts.GetValueOrDefault("--quoted-identifier", "on").ToLowerInvariant();
        if (mode is not ("on" or "off"))
        {
            Console.Error.WriteLine("--quoted-identifier must be on or off");
            return 2;
        }
        var quotedIdentifier = mode == "on";
        var levels = opts.TryGetValue("--levels", out var l) ? l.Split(',').Select(int.Parse).ToArray() : AllLevels;
        var bytes = File.ReadAllBytes(casesFile);
        var cases = Encoding.UTF8.GetString(bytes).Split('\n', StringSplitOptions.RemoveEmptyEntries)
            .Select(line => JsonNode.Parse(line)!)
            .Select(c => (Id: (string)c["id"]!, Source: (string)c["source"]!))
            .ToList();
        var sb = new StringBuilder();
        var meta = new JsonObject
        {
            ["mode"] = "SCRIPTDOM",
            ["parser"] = "Microsoft.SqlServer.TransactSql.ScriptDom "
                + typeof(TSqlParser).Assembly
                    .GetCustomAttributes(typeof(System.Reflection.AssemblyInformationalVersionAttribute), false)
                    .Cast<System.Reflection.AssemblyInformationalVersionAttribute>()
                    .First().InformationalVersion,
            ["quoted_identifier"] = quotedIdentifier ? 1 : 0,
            ["levels"] = new JsonArray(levels.Select(x => (JsonNode)x).ToArray()),
            ["cases_sha256"] = Convert.ToHexStringLower(SHA256.HashData(bytes)),
        };
        sb.Append("{\n\"meta\": ").Append(meta.ToJsonString()).Append(",\n\"verdicts\": {\n");
        for (var i = 0; i < cases.Count; i++)
        {
            var v = new JsonObject();
            foreach (var level in levels)
            {
                Parser(level, quotedIdentifier).Parse(new StringReader(cases[i].Source), out IList<ParseError> errors);
                v[level.ToString()] = new JsonArray(errors.Select(e => (JsonNode)e.Number).ToArray());
            }
            sb.Append("  ").Append(JsonValue.Create(cases[i].Id)!.ToJsonString()).Append(": ").Append(v.ToJsonString());
            sb.Append(i < cases.Count - 1 ? ",\n" : "\n");
        }
        sb.Append("}\n}\n");
        File.WriteAllText(outFile, sb.ToString());
        return 0;
    }
}
