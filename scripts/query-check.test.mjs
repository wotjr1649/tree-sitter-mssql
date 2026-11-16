import assert from 'node:assert/strict';
import { test } from 'node:test';
import { applySteps, parseCaptures } from './query-check.mjs';

test('parses capture lines of tree-sitter query --captures', () => {
  const out = [
    'in.sql',
    '    pattern:  0, capture: 0 - table, start: (0, 17), end: (0, 22), text: `Items`',
    '    pattern:  1, capture: 1 - column, start: (0, 24), end: (0, 26), text: `Id`',
  ].join('\n');
  assert.deepEqual(parseCaptures(out), [
    { name: 'table', text: 'Items' },
    { name: 'column', text: 'Id' },
  ]);
});

test('applies the first `step` edits in order and refuses a missing target', () => {
  const edits = [
    { find: 'a', replace: 'b' },
    { find: 'b', replace: 'c' },
  ];
  assert.equal(applySteps('a a', edits, 0), 'a a');
  assert.equal(applySteps('a a', edits, 1), 'b a');
  assert.equal(applySteps('a a', edits, 2), 'c a');
  assert.throws(() => applySteps('x', edits, 1));
});
