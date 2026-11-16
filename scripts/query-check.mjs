// Query regression check (ADR-0004): grammar-kit's T-SQL queries in test/queries/kit/*.scm, run on the oracle
// sources named in test/queries/kit-cases.json (after `step` edits from oracle/data/kit/route-edits.jsonl), must give
// the recorded captures. The recorded captures were measured on this grammar, not copied from grammar-kit.
//
// Usage: node scripts/query-check.mjs --tree-sitter <cli> [--update]
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const CASES = 'test/queries/kit-cases.json';

// The captures of `tree-sitter query --captures` output: one {name, text} per capture line, in order.
export function parseCaptures(output) {
  return [
    ...output.matchAll(
      /capture: \d+ - ([^,]+), start: \(\d+, \d+\), end: \(\d+, \d+\), text: `([^`]*)`/g,
    ),
  ].map((m) => ({ name: m[1], text: m[2] }));
}

// The source after applying the first `step` edits (each replaces the first occurrence of `find`).
export function applySteps(source, edits, step) {
  let s = source;
  for (const { find, replace } of edits.slice(0, step)) {
    const i = s.indexOf(find);
    if (i < 0) throw new Error(`edit target not found: ${find}`);
    s = s.slice(0, i) + replace + s.slice(i + find.length);
  }
  return s;
}

const jsonl = (file) =>
  readFileSync(file, 'utf8')
    .split('\n')
    .filter(Boolean)
    .map((l) => JSON.parse(l));

function main() {
  const argv = process.argv.slice(2);
  const cli = argv[argv.indexOf('--tree-sitter') + 1];
  if (!argv.includes('--tree-sitter') || !cli)
    throw new Error('usage: query-check.mjs --tree-sitter <cli> [--update]');
  const update = argv.includes('--update');
  const cases = JSON.parse(readFileSync(CASES, 'utf8'));
  const sources = Object.fromEntries(
    jsonl('oracle/data/probes/kit-routes.jsonl').map((c) => [c.id, c.source]),
  );
  const edits = Object.fromEntries(
    jsonl('oracle/data/kit/route-edits.jsonl').map((h) => [h.id, h.edits]),
  );
  const dir = mkdtempSync(join(tmpdir(), 'query-check-'));
  let failed = 0;
  for (const c of cases) {
    const source = applySteps(sources[c.source_case], edits[c.source_case] ?? [], c.step);
    const file = join(dir, 'in.sql');
    writeFileSync(file, source);
    const r = spawnSync(
      cli,
      ['query', '--captures', join('test/queries/kit', `${c.query}.scm`), file],
      {
        encoding: 'utf8',
        maxBuffer: 1 << 26,
      },
    );
    if (r.status !== 0)
      throw new Error(`${c.id} step ${c.step}: tree-sitter query failed: ${r.stderr}`);
    const captures = parseCaptures(r.stdout);
    if (update) {
      c.captures = captures;
    } else if (JSON.stringify(captures) !== JSON.stringify(c.captures)) {
      failed++;
      console.error(
        `${c.id} step ${c.step} (${c.query}): captures differ\n  expected ${JSON.stringify(c.captures)}\n  actual   ${JSON.stringify(captures)}`,
      );
    }
  }
  if (update) writeFileSync(CASES, `${JSON.stringify(cases, null, 2)}\n`);
  console.log(
    `${cases.length} query cases${update ? ' recorded' : failed ? `, ${failed} differ` : ' match'}`,
  );
  if (failed) process.exit(1);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) main();
