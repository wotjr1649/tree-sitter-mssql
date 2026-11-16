import assert from 'node:assert/strict';
import { test } from 'node:test';
import { compare } from './scriptdom-compare.mjs';

test('counts each engine/ScriptDOM/tree cell and lists the both-reject-but-clean cases', () => {
  const { cells, bothRejectClean } = compare(
    { a: true, b: false, c: false, d: false },
    { a: true, b: false, c: true, d: false },
    { a: 'clean', b: 'clean', c: 'error', d: 'error', unjudged: 'clean' },
  );
  assert.deepEqual(cells, {
    'engine accepts, ScriptDOM accepts, tree clean': 1,
    'engine rejects, ScriptDOM rejects, tree clean': 1,
    'engine rejects, ScriptDOM accepts, tree error': 1,
    'engine rejects, ScriptDOM rejects, tree error': 1,
  });
  assert.deepEqual(bothRejectClean, ['b']);
});
