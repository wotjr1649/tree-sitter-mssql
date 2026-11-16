// Replays the SQL Server oracle against the current grammar (ADR-0002).
//
// For every case in oracle/data/kit/cases.jsonl and oracle/data/probes/*.jsonl the parse tree is judged by tree-check (clean or error) and
// compared with the committed baseline oracle/data/expected.json; edit histories are replayed to compare
// incremental and fresh parses. Engine verdicts (oracle/data/verdicts/*.json) give each case's class: a case
// is positive when SQL Server accepts it at any verified compatibility level.
//
// Usage: node scripts/oracle-replay.mjs --tree-check <exe> [--update]
// Exit 0: results equal the baseline (or the baseline was updated); 1: results differ, or an update
// would turn an engine-accepted case from clean into an error; 2: inputs are inconsistent.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  caseSetFiles,
  checkPaths,
  editHistoryFiles,
  isAccepted,
  isMain,
  readCases,
  readJsonl,
  sha256,
  writeSummary,
} from './common.mjs';

// Historical verdicts record SESSIONPROPERTY as 1/0; accept explicit names as well.
export function quotedIdentifier(meta) {
  const value = meta.session_set?.quoted_identifier ?? 1;
  if (value === 1 || String(value).toLowerCase() === 'on') return 'on';
  if (value === 0 || String(value).toLowerCase() === 'off') return 'off';
  throw new Error(`unknown quoted_identifier session setting: ${value}`);
}

export const sessionId = (id, mode) => (mode === 'on' ? id : `${id}@qi-off`);

// A positive ON verdict must never hide an OFF rejection of the same source.
export function mergeVerdicts(sets, observations) {
  const verdicts = {};
  const covered = new Set();
  for (const { file, data: v } of observations) {
    assert.equal(v.meta.mode, 'PARSEONLY', `${file}: only PARSEONLY verdicts count`);
    const set = sets.find((s) => s.sha === v.meta.cases_sha256);
    assert(set, `${file}: verdicts were made from cases that are not in the repository`);
    const session = quotedIdentifier(v.meta);
    covered.add(`${set.file}@${session}`);
    for (const c of set.cases) {
      assert(v.verdicts[c.id], `${file}: no verdict for ${c.id}`);
      const id = sessionId(c.id, session);
      verdicts[id] ??= {};
      for (const [level, errors] of Object.entries(v.verdicts[c.id])) {
        const key = `${v.meta.engine.product_version}@${level}`;
        verdicts[id][key] = errors;
      }
    }
  }
  for (const s of sets)
    for (const session of ['on', 'off'])
      assert(
        covered.has(`${s.file}@${session}`),
        `${s.file}: no ${session.toUpperCase()} verdict file`,
      );
  const cases = sets.flatMap((s) =>
    s.cases.flatMap((c) =>
      ['on', 'off']
        .filter((mode) => verdicts[sessionId(c.id, mode)])
        .map((session) => ({ ...c, id: sessionId(c.id, session), session })),
    ),
  );
  return { cases, verdicts };
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

// The classes a rejected-but-clean case may have (oracle/data/rejected-clean.json, ADR-0004): every one of
// these cases is either a recorded gap or a rejection the parse tree cannot decide.
export const CLASSES = ['gap', 'context', 'version', 'environment', 'runner'];

// Rejected-but-clean cases without a valid class, and classified cases that are no longer rejected-but-clean.
export function classify(rejectedClean, classification) {
  const ids = new Set(rejectedClean);
  return {
    unclassified: rejectedClean.filter((id) => !CLASSES.includes(classification[id]?.class)),
    stale: Object.keys(classification).filter((id) => !ids.has(id)),
  };
}

export function matrix(cases, verdicts, current, level) {
  const m = { acceptedClean: 0, acceptedError: 0, rejectedError: 0, rejectedClean: 0 };
  // A per-level table counts only the cases that engine and level judged.
  const judged = level ? cases.filter((c) => Array.isArray(verdicts[c.id][level])) : cases;
  for (const c of judged) {
    const accepted = level ? verdicts[c.id][level].length === 0 : isAccepted(verdicts[c.id]);
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
  return { ...m, agree, total: judged.length, percent: ((agree / judged.length) * 100).toFixed(2) };
}

function main(argv) {
  const exeIndex = argv.indexOf('--tree-check');
  const exe = exeIndex >= 0 ? argv[exeIndex + 1] : undefined;
  if (!exe) throw new Error('usage: oracle-replay.mjs --tree-check <exe> [--update]');
  const update = argv.includes('--update');

  // Case sets: grammar-kit's cases plus the probes measured while fixing defects (oracle/data/probes).
  const setFiles = caseSetFiles();
  const sets = setFiles.map((file) => {
    const text = readFileSync(file);
    return { file, sha: sha256(text), cases: readCases(text.toString('utf8')) };
  });
  const sourceCases = sets.flatMap((s) => s.cases);
  assert.equal(
    new Set(sourceCases.map((c) => c.id)).size,
    sourceCases.length,
    'case IDs must be unique across sets',
  );
  const verdictFiles = readdirSync('oracle/data/verdicts')
    .filter((f) => f.endsWith('.json'))
    .sort();
  // Engine observations merge only within the same session setting.
  const { cases, verdicts } = mergeVerdicts(
    sets,
    verdictFiles.map((file) => ({
      file,
      data: JSON.parse(readFileSync(join('oracle/data/verdicts', file), 'utf8')),
    })),
  );
  const positive = Object.fromEntries(cases.map((c) => [c.id, isAccepted(verdicts[c.id])]));

  const dir = mkdtempSync(join(tmpdir(), 'oracle-replay-'));
  let current, incremental;
  try {
    const paths = cases.map((c, i) => {
      const file = join(dir, `${i}.sql`);
      writeFileSync(file, c.source);
      return file;
    });
    current = {};
    for (const session of ['on', 'off']) {
      const indices = cases.flatMap((c, i) => (c.session === session ? [i] : []));
      if (!indices.length) continue;
      const pathList = join(dir, `paths-${session}.txt`);
      const rows = checkPaths(
        exe,
        indices.map((i) => paths[i]),
        pathList,
        ['--quoted-identifier', session],
      );
      for (const [i, row] of rows.entries())
        current[cases[indices[i]].id] = row.ok ? 'clean' : 'error';
    }
    // Session settings do not change the grammar tree; edit histories compare that shared tree once.
    const edits = editHistoryFiles().flatMap((f) =>
      readJsonl(runTreeCheck(exe, ['--edits', f]).stdout),
    );
    incremental = Object.fromEntries(
      edits
        .filter((e) => e.incremental_differences.length)
        .map((e) => [e.id, e.incremental_differences]),
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }

  const expected = JSON.parse(readFileSync('oracle/data/expected.json', 'utf8'));
  const changes = compare(expected.cases, current, positive);
  const incrementalChanged =
    JSON.stringify(expected.incremental_differences) !== JSON.stringify(incremental);

  const levels = [...new Set(Object.values(verdicts).flatMap((v) => Object.keys(v)))].sort();
  const union = matrix(cases, verdicts, current);
  const classification = JSON.parse(readFileSync('oracle/data/rejected-clean.json', 'utf8')).cases;
  const rejectedClean = cases
    .map((c) => c.id)
    .filter((id) => !positive[id] && current[id] === 'clean');
  const { unclassified, stale } = classify(rejectedClean, classification);
  const byClass = Object.fromEntries(CLASSES.map((k) => [k, 0]));
  for (const id of rejectedClean) if (classification[id]) byClass[classification[id].class]++;
  const summary = [
    '### SQL Server oracle replay',
    `- Cases: ${cases.length} session/case pairs (${sourceCases.length} sources); engine-accepted at some level: ${Object.values(positive).filter(Boolean).length}`,
    `- Agreement over all levels: ${union.agree}/${union.total} (${union.percent} %): accepted&clean ${union.acceptedClean}, accepted&error ${union.acceptedError}, rejected&error ${union.rejectedError}, rejected&clean ${union.rejectedClean}`,
    ...['on', 'off'].map((session) => {
      const m = matrix(
        cases.filter((c) => c.session === session),
        verdicts,
        current,
      );
      return `- QUOTED_IDENTIFIER ${session.toUpperCase()}: ${m.total} cases, accepted&error ${m.acceptedError}, rejected&clean ${m.rejectedClean}`;
    }),
    ...levels.map((l) => {
      const m = matrix(cases, verdicts, current, l);
      return `- ${l}: ${m.agree}/${m.total} (${m.percent} %), accepted&error ${m.acceptedError}`;
    }),
    `- Rejected but clean by class (oracle/data/rejected-clean.json): ${CLASSES.map((k) => `${k} ${byClass[k]}`).join(', ')}`,
    ...unclassified.map(
      (id) =>
        `  - UNCLASSIFIED ${id}: rejected by every engine but clean; classify it in oracle/data/rejected-clean.json`,
    ),
    ...stale.map(
      (id) =>
        `  - STALE ${id}: classified but no longer rejected-and-clean; remove it from oracle/data/rejected-clean.json`,
    ),
    `- Edit histories with incremental differences: ${Object.keys(incremental).length}`,
    `- Changes against oracle/data/expected.json: ${changes.length}${incrementalChanged ? ' (+ incremental differences changed)' : ''}`,
    ...changes.map((c) => `  - ${c.regression ? 'REGRESSION ' : ''}${c.id}: ${c.from} -> ${c.to}`),
  ].join('\n');
  console.log(summary);
  writeSummary(summary);

  if (union.acceptedError) {
    console.error(
      'Engine-accepted cases must be clean under their recorded session; refusing the replay or baseline update.',
    );
    return 1;
  }
  if (unclassified.length || stale.length) {
    console.error(
      'oracle/data/rejected-clean.json does not match the rejected-but-clean cases (see UNCLASSIFIED/STALE above).',
    );
    return 1;
  }
  if (update) {
    const sorted = Object.fromEntries(
      Object.entries(current).sort(([a], [b]) => a.localeCompare(b)),
    );
    const lines = Object.entries(sorted).map(
      ([id, v]) => `    ${JSON.stringify(id)}: ${JSON.stringify(v)}`,
    );
    writeFileSync(
      'oracle/data/expected.json',
      `{\n  "cases": {\n${lines.join(',\n')}\n  },\n  "incremental_differences": ${JSON.stringify(incremental)}\n}\n`,
    );
    console.log('Updated oracle/data/expected.json');
    return 0;
  }
  if (changes.length || incrementalChanged) {
    console.error(
      'Results differ from oracle/data/expected.json. Review the changes, then run with --update in the same pull request.',
    );
    return 1;
  }
  return 0;
}

if (isMain(import.meta.url)) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (e) {
    console.error(`oracle-replay: ${e.message}`);
    process.exitCode = 2;
  }
}
