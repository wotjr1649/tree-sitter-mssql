// Helpers the scripts share: oracle case sets and edit histories, verdict acceptance, running tree-check over
// many files, and CI step summaries. Kept next to the scripts (not in a subfolder) so the format and test globs
// `scripts/*.mjs` cover it.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { appendFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const sha256 = (data) => createHash('sha256').update(data).digest('hex');

export const readJsonl = (text) =>
  text
    .split('\n')
    .filter((line) => line.trim())
    .map((line) => JSON.parse(line));

// Oracle case sets carry the sha256 of each source; a mismatch means the file was edited by hand.
export function readCases(text) {
  return readJsonl(text).map((c) => {
    assert.equal(sha256(Buffer.from(c.source)), c.sha256, `source hash mismatch for ${c.id}`);
    return c;
  });
}

// A case is accepted when any judged level has no error; UNSUPPORTED_CONTROL never counts as accepted.
export const isAccepted = (levels) =>
  Object.values(levels).some((errors) => Array.isArray(errors) && errors.length === 0);

// grammar-kit's cases, then the probe sets in name order.
export function caseSetFiles() {
  return [
    'oracle/data/kit/cases.jsonl',
    ...readdirSync('oracle/data/probes')
      .filter((f) => f.endsWith('.jsonl'))
      .sort()
      .map((f) => join('oracle/data/probes', f)),
  ];
}

export function editHistoryFiles() {
  return readdirSync('oracle/data/kit')
    .filter((f) => f.endsWith('edits.jsonl'))
    .sort()
    .map((f) => join('oracle/data/kit', f));
}

// One edit-history step: replace the first occurrence of `find`.
export function applyEdit(source, { find, replace }) {
  const at = source.indexOf(find);
  if (at < 0) throw new Error(`edit target not found: ${find}`);
  return source.slice(0, at) + replace + source.slice(at + find.length);
}

// Run tree-check over `paths` (written to `list`) and return one validated row per path, in order.
export function checkPaths(exe, paths, list, args = [], { timeout } = {}) {
  writeFileSync(list, paths.join('\n'));
  const result = spawnSync(exe, [...args, '--paths', list], {
    encoding: 'utf8',
    maxBuffer: 1 << 28,
    timeout,
  });
  return readRows(result, paths);
}

// The rows of a tree-check run over `paths`. Exit 1 only means some input has a problem; anything else
// (another status, a signal, a spawn error) is a failure without verdicts.
export function readRows(result, paths) {
  assert(
    !result.error && !result.signal && [0, 1].includes(result.status),
    `tree-check failed without verdicts: ${result.error?.message || result.stderr || result.signal || result.status}`,
  );
  const rows = readJsonl(result.stdout);
  assert.equal(rows.length, paths.length, 'tree-check returned a different number of results');
  for (const [i, row] of rows.entries()) {
    assert.equal(row.path, paths[i], 'tree-check returned an unexpected path');
    assert.equal(typeof row.root_has_error, 'boolean', 'invalid root_has_error');
    for (const field of ['error_nodes', 'missing_nodes', 'zero_width_nodes', 'root_end', 'len'])
      assert(Number.isSafeInteger(row[field]) && row[field] >= 0, `invalid ${field}`);
  }
  return rows;
}

export function writeSummary(text) {
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${text}\n`);
}

// True when the module at `url` is the script node was started with.
export const isMain = (url) =>
  Boolean(process.argv[1]) && url === pathToFileURL(resolve(process.argv[1])).href;
