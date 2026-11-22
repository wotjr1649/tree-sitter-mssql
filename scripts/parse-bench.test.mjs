import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { budgetCheck, corpusSources, packSources, parseMeasurement } from './parse-bench.mjs';

test('reads corpus SQL with its whitespace and no headers, attributes, or expected trees', () => {
  const corpus = [
    '====',
    'a comment and Korean identifier',
    '====',
    '',
    '---',
    'SELECT 이름  ',
    '',
    '--------',
    '(tree)',
    '',
    '====',
    'expected error',
    ':error',
    '====',
    'SELECT',
    '---',
    '',
  ].join('\r\n');
  assert.deepEqual(corpusSources(corpus), ['\r\n---\r\nSELECT 이름  \r\n', 'SELECT']);
  assert.deepEqual(corpusSources('===\ntie\n===\n---\nSELECT 1\n---\n(tree)'), ['---\nSELECT 1']);
  assert.throws(() => corpusSources('===\nmissing divider\n===\nSELECT 1'), /input divider/);
});

test('packs length-prefixed bytes including Unicode, NULs, and empty sources', () => {
  const { pack, cases, bytes } = packSources(['', '한\0', Buffer.from([0xff])]);
  assert.equal(cases, 3);
  assert.equal(bytes, 5);
  assert.equal(pack.toString('hex'), '0000000004000000ed959c0001000000ff');
});

test('requires a positive Ir count and matching input tallies', () => {
  const input = { cases: 3, bytes: 50 };
  const profile = 'events: Ir\nsummary: 12345\n';
  assert.deepEqual(parseMeasurement(profile, '3 50 1\n', input), {
    instructions: 12345,
    cases: 3,
    bytes: 50,
    errors: 1,
  });
  assert.throws(() => parseMeasurement('events: Dr\nsummary: 123\n', '3 50 1', input), /Ir/);
  assert.throws(() => parseMeasurement(profile, '3 51 1', input), /tally/);
  assert.throws(() => parseMeasurement(profile, '3 50 4', input), /tally/);
});

test('gates increases strictly above a configured threshold', () => {
  assert.equal(budgetCheck('')(200, 100), false);
  assert.equal(budgetCheck('0')(100, 100), false);
  assert.equal(budgetCheck('0')(101, 100), true);
  assert.equal(budgetCheck('10')(110, 100), false);
  assert.equal(budgetCheck('0.125')(801, 800), false);
  assert.equal(budgetCheck('0.124')(801, 800), true);
  assert.equal(budgetCheck('1')(80, 100), false);
  assert.throws(() => budgetCheck('-1'), /nonnegative/);
});

test('reports all workloads against the first ref and writes evidence when the budget fails', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'mssql-parse-bench-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const write = (file, content) => writeFileSync(join(dir, file), content);
  const read = (file) => readFileSync(join(dir, file), 'utf8');
  const refs = ['release/comparison-base', 'tag', 'HEAD'];
  write('refs.tsv', refs.map((ref, i) => ref + '\t' + String(i).repeat(40) + '\n').join(''));
  const sets = ['fixtures', 'corpus', 'oracle'];
  const modes = ['fresh', 'incremental'];
  write(
    'inputs.json',
    JSON.stringify(Object.fromEntries(sets.map((set) => [set, { cases: 3, bytes: 50 }]))),
  );
  for (const [i, set] of sets.entries()) {
    for (const mode of modes) {
      const baseline = (i + 1) * (mode === 'fresh' ? 100 : 200);
      for (const [index, instructions] of [baseline, baseline * 1.1, baseline * 0.9].entries()) {
        const profile = [index, set, mode].join('-');
        write(profile + '.callgrind', 'events: Ir\nsummary: ' + Math.round(instructions) + '\n');
        write(profile + '.tally', '3 50 1\n');
      }
    }
  }
  const summary = fileURLToPath(new URL('./parse-bench.mjs', import.meta.url));
  const run = (threshold, stepSummary) =>
    spawnSync(process.execPath, [summary, 'summary', dir, threshold], {
      encoding: 'utf8',
      env: { ...process.env, GITHUB_STEP_SUMMARY: join(dir, stepSummary) },
    });
  const reportOnly = run('', 'report.md');
  assert.equal(reportOnly.status, 0, reportOnly.stderr);
  assert.match(read('report.md'), /Report only/);
  assert.equal(
    JSON.parse(read('results.json')).some((row) => row.overBudget),
    false,
  );

  const gated = run('9.99', 'gated.md');
  assert.equal(gated.status, 1, gated.stderr);
  const rows = JSON.parse(read('results.json'));
  assert.equal(rows.length, 18);
  assert.match(read('gated.md'), /change versus release\/comparison-base/);
  assert.deepEqual(
    rows.filter((row) => row.overBudget).map((row) => row.ref),
    Array(6).fill('tag'),
  );
  assert.equal(rows.find((row) => row.ref === 'HEAD').increasePct, -10);
});
