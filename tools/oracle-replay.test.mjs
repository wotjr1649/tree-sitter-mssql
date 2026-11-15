import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { compare, isPositive, readCases } from './oracle-replay.mjs';

const line = (id, source, sha = createHash('sha256').update(source).digest('hex')) =>
  JSON.stringify({ id, sha256: sha, source });

test('cases must match their recorded source hash', () => {
  assert.equal(readCases(`${line('a', 'SELECT 1;')}\n\n`).length, 1);
  assert.throws(() => readCases(line('a', 'SELECT 1;', '0'.repeat(64))), /hash mismatch for a/);
});

test('a case is positive when any level accepts it, never through UNSUPPORTED_CONTROL', () => {
  assert.equal(isPositive({ 110: [102], 170: [] }), true);
  assert.equal(isPositive({ 110: [102], 170: [156] }), false);
  assert.equal(isPositive({ 110: 'UNSUPPORTED_CONTROL' }), false);
});

test('only an engine-accepted case going from clean to error is a regression', () => {
  const changes = compare(
    { pos: 'clean', neg: 'error', fixed: 'error' },
    { pos: 'error', neg: 'clean', fixed: 'clean', added: 'clean' },
    { pos: true, neg: false, fixed: true, added: true },
  );
  assert.deepEqual(
    changes.map((c) => `${c.id}:${c.from}->${c.to}:${c.regression}`),
    [
      'added:absent->clean:false',
      'fixed:error->clean:false',
      'neg:error->clean:false',
      'pos:clean->error:true',
    ],
  );
});
