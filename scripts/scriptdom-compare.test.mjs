import assert from 'node:assert/strict';
import { test } from 'node:test';
import { compare, observations } from './scriptdom-compare.mjs';

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

test('engine and ScriptDOM acceptance stay bound to their initial session', () => {
  const engine = observations([
    { meta: { session_set: { quoted_identifier: 1 } }, verdicts: { a: { 170: [] } } },
    { meta: { session_set: { quoted_identifier: 0 } }, verdicts: { a: { 170: [102] } } },
  ]);
  const scriptdom = observations(
    [
      { meta: { quoted_identifier: 1 }, verdicts: { a: { 170: [] } } },
      { meta: { quoted_identifier: 0 }, verdicts: { a: { 170: [46010] } } },
    ],
    true,
  );
  assert.deepEqual(engine, { a: true, 'a@qi-off': false });
  assert.deepEqual(scriptdom, engine);
  assert.deepEqual(
    compare(engine, scriptdom, { a: 'clean', 'a@qi-off': 'clean' }).bothRejectClean,
    ['a@qi-off'],
  );
});
