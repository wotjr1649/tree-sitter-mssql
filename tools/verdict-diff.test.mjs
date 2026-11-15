import assert from 'node:assert/strict';
import test from 'node:test';
import { diff } from './verdict-diff.mjs';

const file = (levels, verdicts, sha = 'x') => ({
  meta: { mode: 'PARSEONLY', cases_sha256: sha, levels },
  verdicts,
});

test('reports differing cases per shared level only', () => {
  const rows = diff(
    file([110, 170], { a: { 110: [], 170: [] }, b: { 110: [102], 170: [] } }),
    file([110], { a: { 110: [156] }, b: { 110: [102] } }),
  );
  assert.deepEqual(rows, [{ level: '110', differing: [{ id: 'a', base: [], other: [156] }] }]);
});

test('refuses to compare verdicts from different cases or modes', () => {
  assert.throws(() => diff(file([110], {}, 'x'), file([110], {}, 'y')), /different cases/);
  assert.throws(() => diff({ meta: { mode: 'NOEXEC' } }, file([110], {})), /PARSEONLY/);
});
