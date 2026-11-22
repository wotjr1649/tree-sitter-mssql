# Identifier characters

SQL Server's documentation describes the start of a regular identifier using the Unicode Standard
3.2 letter definition. This directory records the measured set. The sweep covered U+0020 through
U+FFFF, excluding surrogates, in the two regular-identifier positions below.

- `probes.mjs` writes 126,912 probes: `s-XXXX` (`SELECT 1 AS <c>;`, start position) and `c-XXXX` (`SELECT 1 AS a<c>b;`, continuation) for every code point from U+0020 to U+FFFF except surrogates. Run as `node oracle/data/unicode/probes.mjs probes.jsonl`. The output's sha256 is recorded in `identifier-chars.json`.
- `identifier-chars.json` holds the accepted code points as ranges for each position, and `meta` with:
  - engine, levels, runner hash and probe hash;
  - the code points excluded because the probes accepted them for structural reasons.
- The [ParseOnly runner](../../runners/ParseOnly/) produced the verdicts. From the repository root,
  build it as described in the [oracle README](../../README.md#producing-verdicts), then run:

  ```sh
  dotnet oracle/runners/ParseOnly/bin/Release/net10.0/ParseOnly.dll --levels 100,170 --cases probes.jsonl --out verdicts.json
  ```

  The raw verdict files (about 3 MB per level) are not committed. The command above remeasures
  both levels with the current runner. `meta` records the engine and the two runner hashes from
  the original measurements.

## Result (SQL Server 2025 LocalDB 17.0.1000.7, levels 100 and 170 identical)

- **Start position:** 45,695 code points accepted. In Unicode terms these are letters (Lu, Ll, Lt, Lm, Lo, Nl) of the Unicode 3.2 era. 3,342 letters added to Unicode later are rejected, for example U+0221. `_` and `#` are also accepted.
- **Continuation:** additionally decimal digits, combining marks (Mn, Mc), some format characters (Cf, including U+FEFF), connector punctuation, `$`, `@` and `#`.
- A supplementary-character spot check returned Msg 102 at both measured levels. The sweep
  itself covers the BMP range stated above.
- Two exclusions, both recorded in `meta.excluded`:
  - `#` at the start: names that start with `#` are temporary objects, which the grammar models separately.
  - `,` in the continuation: `SELECT 1 AS a,b` is a select list, not one name.

`grammar/helpers.js` holds `IDENTIFIER_START` and `IDENTIFIER_CONTINUE` generated from this file.
Run `node scripts/identifier-charclass.mjs --print` to print the classes. Verify runs the script
with `--check`. At the original measurement, tree-check agreed on 126,911 of 126,912 probes.
The bare `#` alias has since been fixed and is covered by `identifiers/alias-hash`.
