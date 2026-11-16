import assert from 'node:assert/strict';
import test from 'node:test';
import { growth, measure } from './parser-cost.mjs';

const parserC = [
  '#define STATE_COUNT 10',
  '#define LARGE_STATE_COUNT 4',
  '#define SYMBOL_COUNT 8',
  '#define TOKEN_COUNT 5',
  '#define FIELD_COUNT 2',
  '#define PRODUCTION_ID_COUNT 3',
  'static const TSFieldMapEntry ts_field_map_entries[] = {',
  '  [0] =',
  '    {field_name, 0},',
  '    {field_body, 1, .inherited = true},',
  '};',
].join('\n');

test('reads the table counts and field-map entries', () => {
  assert.deepEqual(measure(parserC, 123), {
    parser_c_bytes: 123,
    state_count: 10,
    large_state_count: 4,
    symbol_count: 8,
    token_count: 5,
    field_count: 2,
    production_id_count: 3,
    field_map_entries: 2,
    inherited_field_map_entries: 1,
  });
  assert.throws(() => measure('#define STATE_COUNT 1', 1), /LARGE_STATE_COUNT/);
});

test('growth is relative to the baseline', () => {
  const g = growth({ a: 100, b: 0, c: 50 }, { a: 111, b: 0, c: 45 });
  assert.equal(g.a.toFixed(2), '0.11');
  assert.equal(g.b, 0);
  assert.equal(g.c, -0.1);
});
