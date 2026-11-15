// Replays the SQL Server oracle against the current grammar (ADR-0002).
//
// For every case in oracle/kit/cases.jsonl the parse tree is judged by tree-check (clean or error) and
// compared with the committed baseline oracle/expected.json; edit histories are replayed to compare
// incremental and fresh parses. Engine verdicts (oracle/verdicts/*.json) give each case's class: a case
// is positive when SQL Server accepts it at any verified compatibility level.
//
// Usage: node tools/oracle-replay.mjs --tree-check <exe> [--update [--allow-regression=ID,...]]
// Exit 0: results equal the baseline (or the baseline was updated); 1: results differ, or an update
// would turn an engine-accepted case from clean into an error; 2: inputs are inconsistent.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  appendFileSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const sha256 = (data) => createHash('sha256').update(data).digest('hex');

export function readCases(text) {
  return text
    .split('\n')
    .filter((line) => line.trim())
    .map((line) => {
      const c = JSON.parse(line);
      assert.equal(sha256(Buffer.from(c.source)), c.sha256, `source hash mismatch for ${c.id}`);
      return c;
    });
}

// A case is positive when any verified level accepts it; UNSUPPORTED_CONTROL never counts as accepted.
export function isPositive(levels) {
  return Object.values(levels).some((errors) => Array.isArray(errors) && errors.length === 0);
}

// Changes between the baseline and the current results; a regression is an engine-accepted case that
// was clean and now has an error.
export function compare(expected, current, positive) {
  const changes = [];
  for (const id of new Set([...Object.keys(expected), ...Object.keys(current)])) {
    if (expected[id] === current[id]) continue;
    changes.push({
      id,
      from: expected[id] ?? 'absent',
      to: current[id] ?? 'absent',
      regression: Boolean(positive[id]) && expected[id] === 'clean' && current[id] === 'error',
    });
  }
  return changes.sort((a, b) => a.id.localeCompare(b.id));
}

function runTreeCheck(exe, args) {
  try {
    return { status: 0, stdout: execFileSync(exe, args, { encoding: 'utf8', maxBuffer: 1 << 28 }) };
  } catch (e) {
    if (e.status === 1) return { status: 1, stdout: e.stdout };
    throw new Error(`tree-check failed without a verdict: ${e.stderr || e.message}`);
  }
}

function matrix(cases, verdicts, current, level) {
  const m = { acceptedClean: 0, acceptedError: 0, rejectedError: 0, rejectedClean: 0 };
  for (const c of cases) {
    const accepted = level ? verdicts[c.id][level]?.length === 0 : isPositive(verdicts[c.id]);
    const error = current[c.id] === 'error';
    m[
      accepted
        ? error
          ? 'acceptedError'
          : 'acceptedClean'
        : error
          ? 'rejectedError'
          : 'rejectedClean'
    ]++;
  }
  const agree = m.acceptedClean + m.rejectedError;
  return { ...m, agree, total: cases.length, percent: ((agree / cases.length) * 100).toFixed(2) };
}

function main(argv) {
  const arg = (name) => argv.find((a) => a.startsWith(`${name}=`))?.slice(name.length + 1);
  const exeIndex = argv.indexOf('--tree-check');
  const exe = exeIndex >= 0 ? argv[exeIndex + 1] : undefined;
  if (!exe)
    throw new Error(
      'usage: oracle-replay.mjs --tree-check <exe> [--update [--allow-regression=ID,...]]',
    );
  const update = argv.includes('--update');
  const allowed = new Set((arg('--allow-regression') ?? '').split(',').filter(Boolean));

  const casesText = readFileSync('oracle/kit/cases.jsonl');
  const cases = readCases(casesText.toString('utf8'));
  const verdictFiles = readdirSync('oracle/verdicts')
    .filter((f) => f.endsWith('.json'))
    .sort();
  assert(verdictFiles.length > 0, 'no verdict files in oracle/verdicts');
  // Verdicts from every observation file are merged per level; each file must describe these exact cases.
  const verdicts = Object.fromEntries(cases.map((c) => [c.id, {}]));
  for (const file of verdictFiles) {
    const v = JSON.parse(readFileSync(join('oracle/verdicts', file), 'utf8'));
    assert.equal(v.meta.mode, 'PARSEONLY', `${file}: only PARSEONLY verdicts count`);
    assert.equal(
      v.meta.cases_sha256,
      sha256(casesText),
      `${file}: verdicts were made from different cases`,
    );
    for (const c of cases) {
      assert(v.verdicts[c.id], `${file}: no verdict for ${c.id}`);
      for (const [level, errors] of Object.entries(v.verdicts[c.id])) {
        const key = `${v.meta.engine.product_version}@${level}`;
        verdicts[c.id][key] = errors;
      }
    }
  }
  const positive = Object.fromEntries(cases.map((c) => [c.id, isPositive(verdicts[c.id])]));

  const dir = mkdtempSync(join(tmpdir(), 'oracle-replay-'));
  let current, incremental;
  try {
    const paths = cases.map((c, i) => {
      const file = join(dir, `${i}.sql`);
      writeFileSync(file, c.source);
      return file;
    });
    writeFileSync(join(dir, 'paths.txt'), paths.join('\n'));
    const rows = runTreeCheck(exe, ['--paths', join(dir, 'paths.txt')])
      .stdout.trim()
      .split('\n')
      .map((l) => JSON.parse(l));
    assert.equal(rows.length, cases.length, 'tree-check returned a different number of results');
    current = Object.fromEntries(cases.map((c, i) => [c.id, rows[i].ok ? 'clean' : 'error']));
    const edits = runTreeCheck(exe, ['--edits', 'oracle/kit/edits.jsonl'])
      .stdout.trim()
      .split('\n')
      .map((l) => JSON.parse(l));
    incremental = Object.fromEntries(
      edits
        .filter((e) => e.incremental_differences.length)
        .map((e) => [e.id, e.incremental_differences]),
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }

  const expected = JSON.parse(readFileSync('oracle/expected.json', 'utf8'));
  const changes = compare(expected.cases, current, positive);
  const incrementalChanged =
    JSON.stringify(expected.incremental_differences) !== JSON.stringify(incremental);
  const regressions = changes.filter((c) => c.regression && !allowed.has(c.id));

  const levels = [...new Set(Object.values(verdicts).flatMap((v) => Object.keys(v)))].sort();
  const union = matrix(cases, verdicts, current);
  const summary = [
    '### SQL Server oracle replay',
    `- Cases: ${cases.length}; engine-accepted at some level: ${Object.values(positive).filter(Boolean).length}`,
    `- Agreement over all levels: ${union.agree}/${union.total} (${union.percent} %): accepted&clean ${union.acceptedClean}, accepted&error ${union.acceptedError}, rejected&error ${union.rejectedError}, rejected&clean ${union.rejectedClean}`,
    ...levels.map((l) => {
      const m = matrix(cases, verdicts, current, l);
      return `- ${l}: ${m.agree}/${m.total} (${m.percent} %), accepted&error ${m.acceptedError}`;
    }),
    `- Edit histories with incremental differences: ${Object.keys(incremental).length}`,
    `- Changes against oracle/expected.json: ${changes.length}${incrementalChanged ? ' (+ incremental differences changed)' : ''}`,
    ...changes.map((c) => `  - ${c.regression ? 'REGRESSION ' : ''}${c.id}: ${c.from} -> ${c.to}`),
  ].join('\n');
  console.log(summary);
  if (process.env.GITHUB_STEP_SUMMARY)
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary + '\n');

  if (update) {
    if (regressions.length) {
      console.error(
        `Refusing to update: engine-accepted cases would become errors: ${regressions.map((c) => c.id).join(', ')}. Pass --allow-regression=ID,... only with a stated reason.`,
      );
      return 1;
    }
    const sorted = Object.fromEntries(
      Object.entries(current).sort(([a], [b]) => a.localeCompare(b)),
    );
    const lines = Object.entries(sorted).map(
      ([id, v]) => `    ${JSON.stringify(id)}: ${JSON.stringify(v)}`,
    );
    writeFileSync(
      'oracle/expected.json',
      `{\n  "cases": {\n${lines.join(',\n')}\n  },\n  "incremental_differences": ${JSON.stringify(incremental)}\n}\n`,
    );
    console.log('Updated oracle/expected.json');
    return 0;
  }
  if (changes.length || incrementalChanged) {
    console.error(
      'Results differ from oracle/expected.json. Review the changes, then run with --update in the same pull request.',
    );
    return 1;
  }
  return 0;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (e) {
    console.error(`oracle-replay: ${e.message}`);
    process.exitCode = 2;
  }
}
