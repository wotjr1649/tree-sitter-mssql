import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import {
  classify,
  compare,
  isPositive,
  matrix,
  mergeVerdicts,
  quotedIdentifier,
  readCases,
} from './oracle-replay.mjs';

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

test('every rejected-but-clean case needs a known class, and no entry may outlive its case', () => {
  const classification = { a: { class: 'gap' }, b: { class: 'guess' }, gone: { class: 'context' } };
  assert.deepEqual(classify(['a', 'b', 'c'], classification), {
    unclassified: ['b', 'c'],
    stale: ['gone'],
  });
  assert.deepEqual(classify(['a'], { a: { class: 'environment' } }), {
    unclassified: [],
    stale: [],
  });
});

test('quoted identifier sessions remain separate when observations share a source and engine', () => {
  const sets = [{ file: 'cases', sha: 'hash', cases: [{ id: 'a', source: 'SELECT "a";' }] }];
  const observation = (value, errors) => ({
    file: `qi-${value}`,
    data: {
      meta: {
        mode: 'PARSEONLY',
        session_set: { quoted_identifier: value },
        cases_sha256: 'hash',
        engine: { product_version: '17' },
      },
      verdicts: { a: { 170: errors } },
    },
  });
  assert.throws(() => mergeVerdicts(sets, [observation(1, [])]), /no OFF verdict file/);
  assert.throws(() => mergeVerdicts(sets, [observation(0, [102])]), /no ON verdict file/);
  const { cases, verdicts } = mergeVerdicts(sets, [observation(1, []), observation(0, [102])]);
  assert.deepEqual(
    cases.map((c) => [c.id, c.session]),
    [
      ['a', 'on'],
      ['a@qi-off', 'off'],
    ],
  );
  assert.equal(isPositive(verdicts.a), true);
  assert.equal(isPositive(verdicts['a@qi-off']), false);
  assert.equal(quotedIdentifier({}), 'on');
  assert.equal(quotedIdentifier({ session_set: { quoted_identifier: 'OFF' } }), 'off');
  assert.throws(() => quotedIdentifier({ session_set: { quoted_identifier: 'guess' } }), /unknown/);
});

test('the acceptance matrix counts an accepted error separately from rejected clean in another session', () => {
  const result = matrix(
    [{ id: 'a' }, { id: 'a@qi-off' }],
    { a: { 170: [] }, 'a@qi-off': { 170: [102] } },
    { a: 'error', 'a@qi-off': 'clean' },
  );
  assert.equal(result.acceptedError, 1);
  assert.equal(result.rejectedClean, 1);
  assert.equal(result.total, 2);
});
