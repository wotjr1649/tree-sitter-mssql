// Parse-only SQL Server verdicts for the oracle cases (ADR-0002 §4).
//
// For each compatibility level a temporary database is created and dropped at the end. Each case gets
// a new unpooled connection; the control batch `SET PARSEONLY ON` is sent on its own, then the case's
// text unchanged as a separate batch, and every error number the engine returns is recorded ([] when
// accepted). A case whose text mentions PARSEONLY is never sent ("UNSUPPORTED_CONTROL"). Connection
// failures, timeouts and fatal errors stop the run (exit 2): they are never recorded as rejections.
//
// Usage:
//   ParseOnly --cases oracle/kit/cases.jsonl --out verdicts.json [--server S] [--levels 100,...]
//   ParseOnly --self-test [--server S]
// Without --server the runner uses (localdb)\MSSQLLocalDB with Windows authentication. With a server,
// it uses SQL authentication: user from MSSQL_USER (default "sa"), password from MSSQL_PASSWORD.

using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using Microsoft.Data.SqlClient;

static class Program
{
    sealed class InfraException(string message) : Exception(message);

    const int CommandTimeoutSeconds = 10;
    static readonly int[] AllLevels = [100, 110, 120, 130, 140, 150, 160, 170];

    static int Main(string[] args)
    {
        try
        {
            var opts = ParseArgs(args);
            if (opts.ContainsKey("--self-test")) return SelfTest(opts);
            if (!opts.TryGetValue("--cases", out var cases) || !opts.TryGetValue("--out", out var output))
                throw new ArgumentException("usage: ParseOnly --cases FILE --out FILE [--server S] [--levels L,...] | --self-test");
            Sweep(opts, cases!, output!);
            return 0;
        }
        catch (Exception e) when (e is InfraException or SqlException or ArgumentException or IOException or JsonException)
        {
            Console.Error.WriteLine($"ParseOnly: {e.Message}");
            return 2;
        }
    }

    static Dictionary<string, string?> ParseArgs(string[] args)
    {
        var opts = new Dictionary<string, string?>();
        for (var i = 0; i < args.Length; i++)
        {
            if (args[i] == "--self-test") { opts[args[i]] = null; continue; }
            if (!args[i].StartsWith("--") || i + 1 >= args.Length) throw new ArgumentException($"unexpected argument {args[i]}");
            opts[args[i]] = args[++i];
        }
        return opts;
    }

    static string ConnectionString(Dictionary<string, string?> opts, string database)
    {
        var b = new SqlConnectionStringBuilder { InitialCatalog = database, Pooling = false, ConnectTimeout = 30 };
        if (opts.TryGetValue("--server", out var server))
        {
            b.DataSource = server;
            b.UserID = Environment.GetEnvironmentVariable("MSSQL_USER") ?? "sa";
            b.Password = Environment.GetEnvironmentVariable("MSSQL_PASSWORD")
                ?? throw new ArgumentException("MSSQL_PASSWORD is not set");
            b.TrustServerCertificate = true; // throwaway test engine in a container
        }
        else
        {
            b.DataSource = @"(localdb)\MSSQLLocalDB";
            b.IntegratedSecurity = true;
            b.Encrypt = SqlConnectionEncryptOption.Optional;
        }
        return b.ConnectionString;
    }

    static SqlConnection Open(Dictionary<string, string?> opts, string database)
    {
        var conn = new SqlConnection(ConnectionString(opts, database));
        try { conn.Open(); }
        catch (SqlException e) { conn.Dispose(); throw new InfraException($"cannot connect to {database}: {e.Message}"); }
        return conn;
    }

    static object? Scalar(SqlConnection conn, string sql)
    {
        using var cmd = new SqlCommand(sql, conn) { CommandTimeout = CommandTimeoutSeconds };
        return cmd.ExecuteScalar();
    }

    static void Control(SqlConnection conn, string sql)
    {
        using var cmd = new SqlCommand(sql, conn) { CommandTimeout = CommandTimeoutSeconds };
        try { cmd.ExecuteNonQuery(); }
        catch (SqlException e) { throw new InfraException($"control batch failed ({sql}): {e.Message}"); }
    }

    // Errors that mean the engine or the transport failed, not that the text was rejected.
    static bool IsInfrastructure(SqlException e) =>
        e.Number == -2 || e.Class >= 20 || e.Errors.Cast<SqlError>().Any(err => err.Number is 233 or 10053 or 10054 or 1205);

    /// <summary>Error numbers for one case at the connection's level; empty when accepted.</summary>
    static JsonNode Judge(Dictionary<string, string?> opts, string database, int level, string source)
    {
        if (source.Contains("PARSEONLY", StringComparison.OrdinalIgnoreCase)) return JsonValue.Create("UNSUPPORTED_CONTROL");
        using var conn = Open(opts, database);
        if (Convert.ToInt32(Scalar(conn, "SELECT compatibility_level FROM sys.databases WHERE name = DB_NAME()")) != level)
            throw new InfraException($"compatibility level of {database} is not {level}");
        Control(conn, "SET PARSEONLY ON;");
        var errors = new JsonArray();
        // A client splits a script at GO lines and sends each batch on its own; so does the runner.
        // Text without a GO line is sent unchanged, as one batch.
        foreach (var batch in Batches(source))
        {
            using var cmd = new SqlCommand(batch, conn) { CommandTimeout = CommandTimeoutSeconds };
            try { cmd.ExecuteNonQuery(); }
            catch (SqlException e) when (!IsInfrastructure(e))
            {
                foreach (SqlError err in e.Errors) errors.Add(err.Number);
            }
            catch (SqlException e) { throw new InfraException($"infrastructure failure while judging a case: {e.Message}"); }
        }
        return errors;
    }

    // A GO line: GO alone on its line, with an optional repeat count and an optional -- comment.
    // shortcut: line-based, so a GO line inside a multi-line string or block comment would also split;
    // the oracle's case sets have none (checked when a set is imported).
    static readonly System.Text.RegularExpressions.Regex GoLine = new(
        @"^[ \t]*GO[ \t]*(?:\d+)?[ \t]*(?:--[^\r\n]*)?\r?$",
        System.Text.RegularExpressions.RegexOptions.IgnoreCase | System.Text.RegularExpressions.RegexOptions.Multiline);

    /// <summary>The batches of a script: the text between GO lines, skipping whitespace-only ones.</summary>
    static IEnumerable<string> Batches(string source)
    {
        if (!GoLine.IsMatch(source)) return [source];
        return GoLine.Split(source).Where(part => !string.IsNullOrWhiteSpace(part));
    }

    static string RunnerSha256()
    {
        var asm = typeof(Program).Assembly;
        using var sha = SHA256.Create();
        foreach (var name in new[] { "runner-source/ParseOnly.csproj", "runner-source/Program.cs" })
        {
            using var s = asm.GetManifestResourceStream(name) ?? throw new InvalidOperationException($"missing resource {name}");
            var bytes = new MemoryStream();
            s.CopyTo(bytes);
            var b = bytes.ToArray();
            sha.TransformBlock(b, 0, b.Length, null, 0);
        }
        sha.TransformFinalBlock([], 0, 0);
        return Convert.ToHexStringLower(sha.Hash!);
    }

    static string NewDatabase(SqlConnection admin, int level)
    {
        var name = $"TSMSSQL_ORACLE_{Guid.NewGuid():N}"[..30] + $"_C{level}";
        Control(admin, $"CREATE DATABASE [{name}]; ALTER DATABASE [{name}] SET COMPATIBILITY_LEVEL = {level};");
        return name;
    }

    static void DropDatabase(SqlConnection admin, string name)
    {
        SqlConnection.ClearAllPools();
        Control(admin, $"ALTER DATABASE [{name}] SET SINGLE_USER WITH ROLLBACK IMMEDIATE; DROP DATABASE [{name}];");
    }

    static void Sweep(Dictionary<string, string?> opts, string casesFile, string outFile)
    {
        var casesBytes = File.ReadAllBytes(casesFile);
        var cases = Encoding.UTF8.GetString(casesBytes).Split('\n', StringSplitOptions.RemoveEmptyEntries)
            .Select(line => JsonNode.Parse(line)!)
            .Select(c => (Id: (string)c["id"]!, Source: (string)c["source"]!))
            .ToList();
        var levels = opts.TryGetValue("--levels", out var l) ? l!.Split(',').Select(int.Parse).ToArray() : AllLevels;
        using var admin = Open(opts, "master");
        var engine = new JsonObject
        {
            ["product_version"] = (string)Scalar(admin, "SELECT CAST(SERVERPROPERTY('ProductVersion') AS nvarchar(64))")!,
            ["product_level"] = (string)Scalar(admin, "SELECT CAST(SERVERPROPERTY('ProductLevel') AS nvarchar(64))")!,
            ["edition"] = (string)Scalar(admin, "SELECT CAST(SERVERPROPERTY('Edition') AS nvarchar(128))")!,
            ["platform"] = (string)Scalar(admin, "SELECT host_platform FROM sys.dm_os_host_info")!,
            ["collation"] = (string)Scalar(admin, "SELECT CAST(SERVERPROPERTY('Collation') AS nvarchar(128))")!,
        };
        // Levels run in parallel: each has its own database, admin connection and per-case connections, and
        // writes only its own result array; results are merged in level order, so the output does not depend
        // on scheduling. Connections are not shared across threads.
        var results = new JsonNode[levels.Length][];
        JsonObject? sessionSet = null;
        try
        {
            Parallel.For(0, levels.Length, i =>
            {
                var level = levels[i];
                using var levelAdmin = Open(opts, "master");
                var db = NewDatabase(levelAdmin, level);
                try
                {
                    if (i == 0)
                    {
                        using var probe = Open(opts, db);
                        var set = new JsonObject();
                        foreach (var p in new[] { "QUOTED_IDENTIFIER", "ANSI_NULLS", "ANSI_PADDING", "ANSI_WARNINGS", "ARITHABORT", "CONCAT_NULL_YIELDS_NULL" })
                            set[p.ToLowerInvariant()] = Convert.ToInt32(Scalar(probe, $"SELECT SESSIONPROPERTY('{p}')"));
                        sessionSet = set;
                    }
                    results[i] = cases.Select(c => Judge(opts, db, level, c.Source)).ToArray();
                }
                finally { DropDatabase(levelAdmin, db); }
                Console.Error.WriteLine($"level {level} done");
            });
        }
        catch (AggregateException e) { throw e.InnerExceptions[0]; }
        var verdicts = cases.ToDictionary(c => c.Id, _ => new JsonObject());
        for (var i = 0; i < levels.Length; i++)
            for (var k = 0; k < cases.Count; k++) verdicts[cases[k].Id][levels[i].ToString()] = results[i][k];
        var meta = new JsonObject
        {
            ["mode"] = "PARSEONLY",
            ["engine"] = engine,
            ["session_set"] = sessionSet,
            ["levels"] = new JsonArray(levels.Select(x => (JsonNode)x).ToArray()),
            ["cases_sha256"] = Convert.ToHexStringLower(SHA256.HashData(casesBytes)),
            ["runner"] = "oracle/runner/ParseOnly (C#, Microsoft.Data.SqlClient)",
            ["runner_sha256"] = RunnerSha256(),
            ["observed_utc"] = DateTime.UtcNow.ToString("yyyy-MM-dd"),
        };
        // One case per line keeps diffs readable.
        var sb = new StringBuilder("{\n\"meta\": ").Append(meta.ToJsonString()).Append(",\n\"verdicts\": {\n");
        for (var i = 0; i < cases.Count; i++)
        {
            sb.Append("  ").Append(JsonValue.Create(cases[i].Id)!.ToJsonString()).Append(": ").Append(verdicts[cases[i].Id].ToJsonString());
            sb.Append(i < cases.Count - 1 ? ",\n" : "\n");
        }
        sb.Append("}\n}\n");
        File.WriteAllText(outFile, sb.ToString(), new UTF8Encoding(false));
    }

    /// <summary>Checks the runner's contract against a live engine; exit 0 when every check passes.</summary>
    static int SelfTest(Dictionary<string, string?> opts)
    {
        using var admin = Open(opts, "master");
        // The engine's own default level: the highest it supports (2017: 140 … 2025: 170).
        var level = Convert.ToInt32(Scalar(admin, "SELECT compatibility_level FROM sys.databases WHERE name = 'master'"));
        var db = NewDatabase(admin, level);
        var failures = new List<string>();
        Console.WriteLine($"self-test at compatibility level {level}");
        try
        {
            void Expect(string name, string source, Func<JsonNode, bool> ok)
            {
                var verdict = Judge(opts, db, level, source);
                Console.WriteLine($"{(ok(verdict) ? "ok  " : "FAIL")} {name}: {verdict.ToJsonString()}");
                if (!ok(verdict)) failures.Add(name);
            }
            bool Accepted(JsonNode v) => v is JsonArray { Count: 0 };
            bool Rejected(JsonNode v) => v is JsonArray { Count: > 0 };
            object? ObjectId(string name)
            {
                using var check = Open(opts, db);
                var id = Scalar(check, $"SELECT OBJECT_ID('{name}')");
                return id is DBNull ? null : id;
            }

            Expect("accepts a valid statement", "SELECT 1;", Accepted);
            Expect("rejects a syntax error with 102", "SELECT 1 FROM;", v => v is JsonArray a && a.Any(n => (int)n! == 102));
            Expect("keeps the batch-first rule for CREATE PROCEDURE", "CREATE PROCEDURE dbo.oracle_p AS SELECT 1;", Accepted);
            Expect("rejects CREATE PROCEDURE after another statement", "SELECT 1;\nCREATE PROCEDURE dbo.oracle_p AS SELECT 1;", Rejected);
            Expect("splits a script at GO lines like a client", "SELECT 1\nGO\nCREATE PROCEDURE dbo.oracle_go AS SELECT 1;", Accepted);
            Expect("keeps a GO-less script as one batch", "SELECT 1\nCREATE PROCEDURE dbo.oracle_go AS SELECT 1;", Rejected);
            Expect("accepts Unicode text", "SELECT N'한글' AS [열];", Accepted);
            Expect("parses without executing", "CREATE TABLE dbo.oracle_probe (a int);", Accepted);
            if (ObjectId("dbo.oracle_probe") is not null) { Console.WriteLine("FAIL a parsed CREATE TABLE was executed"); failures.Add("no execution"); }
            Expect("never sends text that mentions PARSEONLY", "SET PARSEONLY OFF;\nCREATE TABLE dbo.oracle_guard (a int);",
                v => v is JsonValue s && (string)s! == "UNSUPPORTED_CONTROL");
            if (ObjectId("dbo.oracle_guard") is not null) { Console.WriteLine("FAIL guarded text was executed"); failures.Add("guard"); }
        }
        finally { DropDatabase(admin, db); }
        Console.WriteLine(failures.Count == 0 ? "self-test passed" : $"self-test failed: {string.Join(", ", failures)}");
        return failures.Count == 0 ? 0 : 1;
    }
}
