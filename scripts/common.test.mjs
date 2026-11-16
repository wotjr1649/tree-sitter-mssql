import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { applyEdit, checkPaths, isAccepted, readCases, sha256 } from './common.mjs';

const line = (id, source, sha = sha256(source)) => JSON.stringify({ id, sha256: sha, source });

test('readCases checks every source hash and skips blank lines', () => {
  assert.equal(readCases(`${line('a', 'SELECT 1;')}\n\n`).length, 1);
  assert.throws(() => readCases(line('a', 'SELECT 1;', '0'.repeat(64))), /hash mismatch for a/);
});

test('a case is accepted when any level has no error', () => {
  assert.equal(isAccepted({ 110: [102], 170: [] }), true);
  assert.equal(isAccepted({ 110: [102], 170: [156] }), false);
  assert.equal(isAccepted({ 110: 'UNSUPPORTED_CONTROL' }), false);
});

test('applyEdit replaces the first occurrence and refuses a missing target', () => {
  assert.equal(applyEdit('a b a', { find: 'a', replace: 'x' }), 'x b a');
  assert.throws(() => applyEdit('a', { find: 'z', replace: 'x' }), /not found/);
});

test('checkPaths rejects a checker that fails without verdicts', () => {
  const dir = mkdtempSync(join(tmpdir(), 'common-test-'));
  try {
    assert.throws(
      () =>
        checkPaths(process.execPath, ['x.sql'], join(dir, 'list.txt'), ['-e', 'process.exit(3)']),
      /without verdicts/,
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
