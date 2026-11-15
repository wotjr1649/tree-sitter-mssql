# Identifier characters

SQL Server's documentation says that a regular identifier starts with "a letter as defined by the Unicode Standard 3.2", and that the exact set is not spelled out. This directory replaces that wording with a measurement. Every Basic Multilingual Plane code point was put through the engine's own parser.

- `probes.mjs` writes 126,912 probes: `s-XXXX` (`SELECT 1 AS <c>;`, start position) and `c-XXXX` (`SELECT 1 AS a<c>b;`, continuation) for every code point from U+0020 to U+FFFF except surrogates. Run as `node oracle/unicode/probes.mjs probes.jsonl`. The output's sha256 is recorded in `identifier-chars.json`.
- `identifier-chars.json` holds the accepted code points as ranges for each position, and `meta` with:
  - engine, levels, runner hash and probe hash;
  - the code points excluded because the probes accepted them for structural reasons.
- The parser runner (`../runner/ParseOnly`) produced the verdicts:

  ```sh
  dotnet <ParseOnly.dll> --levels 170 --cases probes.jsonl --out verdicts.json
  ```

  The raw verdicts (about 3 MB per level) are not committed. They are reproducible from the probes, the runner and the engine recorded in `meta`.

## Result (SQL Server 2025 LocalDB 17.0.1000.7, levels 100 and 170 identical)

- **Start position:** 45,695 code points accepted. In Unicode terms these are letters (Lu, Ll, Lt, Lm, Lo, Nl) of the Unicode 3.2 era. 3,342 letters added to Unicode later are rejected, for example U+0221. `_` and `#` are also accepted.
- **Continuation:** additionally decimal digits, combining marks (Mn, Mc), some format characters (Cf, including U+FEFF), connector punctuation, `$`, `@` and `#`.
- Characters outside the BMP are rejected. They are not part of the probes, but a spot check returned 102 at both levels.
- Two exclusions, both recorded in `meta.excluded`:
  - `#` at the start: names that start with `#` are temporary objects, which the grammar models separately.
  - `,` in the continuation: `SELECT 1 AS a,b` is a select list, not one name.

`grammar/helpers.js` holds `IDENTIFIER_START` and `IDENTIFIER_CONTINUE` generated from this file (`node tools/identifier-charclass.mjs --print`). `Verify` runs `--check` so the two cannot drift apart. With these classes, tree-check's verdict agrees with the engine on 126,911 of the 126,912 probes. The exception is `SELECT 1 AS #;` (a bare `#` name), which belongs to the separate sigil-name defect.
