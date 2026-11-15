# Parse-only SQL Server verdicts for oracle cases at every compatibility level the engine supports.
#
# For each level a temporary database is created (and dropped at the end); each case gets a new
# unpooled connection, the control batch `SET PARSEONLY ON` is sent separately from the case's own
# text, and every error number the engine returns is recorded. A case whose text mentions PARSEONLY
# is never sent (recorded as "UNSUPPORTED_CONTROL"). Connection or control failures abort the run:
# they are never recorded as syntax rejections.
#
# Prototype runner (System.Data.SqlClient in PowerShell 7); a C# runner replaces it (ADR-0002).
#
# Usage: pwsh -File parseonly-sweep.ps1 -Cases oracle/kit/cases.jsonl -Out <verdicts.json>
#        [-Server '(localdb)\MSSQLLocalDB'] [-Levels 100,110,...]
param(
  [Parameter(Mandatory)][string]$Cases,
  [Parameter(Mandatory)][string]$Out,
  [string]$Server = '(localdb)\MSSQLLocalDB',
  [int[]]$Levels = @(100, 110, 120, 130, 140, 150, 160, 170)
)
$ErrorActionPreference = 'Stop'

function Invoke-Batch($conn, $sql) {
  $cmd = $conn.CreateCommand(); $cmd.CommandText = $sql; $cmd.CommandTimeout = 10
  [void]$cmd.ExecuteNonQuery()
}
function Get-Scalar($conn, $sql) {
  $cmd = $conn.CreateCommand(); $cmd.CommandText = $sql; $cmd.CommandTimeout = 10
  return $cmd.ExecuteScalar()
}

$caseList = Get-Content -Encoding utf8 $Cases | Where-Object { $_.Trim() } | ForEach-Object { $_ | ConvertFrom-Json }
$casesSha = (Get-FileHash -Algorithm SHA256 $Cases).Hash.ToLowerInvariant()
$runnerSha = (Get-FileHash -Algorithm SHA256 $PSCommandPath).Hash.ToLowerInvariant()
$suffix = [guid]::NewGuid().ToString('N').Substring(0, 8)
$admin = [System.Data.SqlClient.SqlConnection]::new("Server=$Server;Database=master;Integrated Security=true;Connect Timeout=30")
$admin.Open()
$engine = [ordered]@{
  product_version = [string](Get-Scalar $admin "SELECT CAST(SERVERPROPERTY('ProductVersion') AS nvarchar(64))")
  product_level   = [string](Get-Scalar $admin "SELECT CAST(SERVERPROPERTY('ProductLevel') AS nvarchar(64))")
  edition         = [string](Get-Scalar $admin "SELECT CAST(SERVERPROPERTY('Edition') AS nvarchar(128))")
  platform        = [string](Get-Scalar $admin "SELECT host_platform FROM sys.dm_os_host_info")
  collation       = [string](Get-Scalar $admin "SELECT CAST(SERVERPROPERTY('Collation') AS nvarchar(128))")
}
$verdicts = @{}
foreach ($c in $caseList) { $verdicts[$c.id] = [ordered]@{} }
$sessionSet = $null
try {
  foreach ($level in $Levels) {
    $db = "TSMSSQL_ORACLE_${suffix}_C$level"
    Invoke-Batch $admin "CREATE DATABASE [$db]; ALTER DATABASE [$db] SET COMPATIBILITY_LEVEL = $level;"
    try {
      foreach ($c in $caseList) {
        if ($c.source -match '(?i)PARSEONLY') { $verdicts[$c.id]["$level"] = 'UNSUPPORTED_CONTROL'; continue }
        $conn = [System.Data.SqlClient.SqlConnection]::new("Server=$Server;Database=$db;Integrated Security=true;Connect Timeout=30;Pooling=false")
        $conn.Open()
        try {
          if ([int](Get-Scalar $conn 'SELECT compatibility_level FROM sys.databases WHERE name = DB_NAME()') -ne $level) {
            throw "compatibility level mismatch on $db"
          }
          if ($null -eq $sessionSet) {
            $sessionSet = [ordered]@{
              quoted_identifier = [int](Get-Scalar $conn "SELECT SESSIONPROPERTY('QUOTED_IDENTIFIER')")
              ansi_nulls        = [int](Get-Scalar $conn "SELECT SESSIONPROPERTY('ANSI_NULLS')")
              ansi_padding      = [int](Get-Scalar $conn "SELECT SESSIONPROPERTY('ANSI_PADDING')")
              ansi_warnings     = [int](Get-Scalar $conn "SELECT SESSIONPROPERTY('ANSI_WARNINGS')")
              arithabort        = [int](Get-Scalar $conn "SELECT SESSIONPROPERTY('ARITHABORT')")
              concat_null_yields_null = [int](Get-Scalar $conn "SELECT SESSIONPROPERTY('CONCAT_NULL_YIELDS_NULL')")
            }
          }
          Invoke-Batch $conn 'SET PARSEONLY ON;'
          $errors = @()
          try { Invoke-Batch $conn $c.source }
          catch [System.Data.SqlClient.SqlException] { $errors = @($_.Exception.Errors | ForEach-Object { $_.Number }) }
          $verdicts[$c.id]["$level"] = $errors
        } finally { $conn.Dispose() }
      }
    } finally {
      [System.Data.SqlClient.SqlConnection]::ClearAllPools()
      Invoke-Batch $admin "ALTER DATABASE [$db] SET SINGLE_USER WITH ROLLBACK IMMEDIATE; DROP DATABASE [$db];"
    }
    Write-Host "level $level done"
  }
} finally { $admin.Dispose() }

# One case per line keeps diffs readable.
$meta = [ordered]@{
  mode = 'PARSEONLY'; engine = $engine; session_set = $sessionSet; levels = $Levels
  cases_sha256 = $casesSha; runner_sha256 = $runnerSha
  observed_utc = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd')
}
$lines = @('{', ('"meta": ' + ($meta | ConvertTo-Json -Depth 4 -Compress) + ','), '"verdicts": {')
$ids = $caseList | ForEach-Object { $_.id }
for ($i = 0; $i -lt $ids.Count; $i++) {
  $comma = if ($i -lt $ids.Count - 1) { ',' } else { '' }
  $lines += ('  ' + ($ids[$i] | ConvertTo-Json -Compress) + ': ' + ($verdicts[$ids[$i]] | ConvertTo-Json -Depth 3 -Compress) + $comma)
}
$lines += @('}', '}')
[System.IO.File]::WriteAllText($Out, ($lines -join "`n") + "`n", [System.Text.UTF8Encoding]::new($false))
