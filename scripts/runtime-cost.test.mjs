import assert from 'node:assert/strict';
import { test } from 'node:test';
import { judge, median } from './runtime-cost.mjs';

test('median of odd and even samples', () => {
  assert.equal(median([3, 1, 2]), 2);
  assert.equal(median([4, 1, 3, 2]), 2.5);
});

test('a ratio within the limit passes, above it fails, and an unmeasured metric is skipped', () => {
  const rows = judge(
    { library_bytes: 100, parse_ms: 10, peak_kib: null },
    { library_bytes: 150, parse_ms: 15.1, peak_kib: null },
    1.5,
  );
  assert.deepEqual(
    rows.map((r) => [r.metric, r.ok]),
    [
      ['library_bytes', true],
      ['parse_ms', false],
      ['peak_kib', true],
    ],
  );
  assert.equal(rows[2].ratio, null);
});
