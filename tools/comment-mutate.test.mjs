import assert from 'node:assert/strict';
import test from 'node:test';
import { compare, gaps, readVerdicts, variants } from './comment-mutate.mjs';

test('mutation skips quoted text, escaped delimiters and nested comments', () => {
  const sql = `SELECT 'a '' b', "a "" b", [a ]] b] /* a /* b */ c */ -- a b\nFROM t;`;
  const mutated = variants(sql).block;
  for (const token of ["'a '' b'", '"a "" b"', '[a ]] b]', '/* a /* b */ c */', '-- a b'])
    assert(mutated.includes(token), token);
  assert(mutated.includes('SELECT /*c*/'));
  assert(mutated.includes('FROM /*c*/ t;'));
});

test('GO and sqlcmd directive lines remain untouched, including their leading whitespace', () => {
  const sql = 'SELECT 1;\n  GO 2\n:r a file.sql\nSELECT 2;';
  for (const mutated of Object.values(variants(sql))) {
    assert(mutated.includes('\n  GO 2\n:r a file.sql\n'));
  }
  assert.equal(variants(sql).block, 'SELECT /*c*/ 1;\n  GO 2\n:r a file.sql\nSELECT /*c*/ 2;');
});

test('binary continuation newlines are part of the literal, other backslashes do not suppress gaps', () => {
  for (const newline of ['\n', '\r\n']) {
    const literal = `0xabc\\${newline}def\\${newline}0123`;
    assert.equal(
      variants(`SELECT ${literal} AS x;`).block,
      `SELECT /*c*/ ${literal} /*c*/ AS /*c*/ x;`,
    );
    assert.equal(variants(`SELECT ${literal};`).line, `SELECT ${literal};`);
    assert.equal(gaps(`SELECT a\\${newline}b;`).length, 2);
  }
});

test('line mutation only uses existing newline gaps and handles CRLF', () => {
  assert.equal(variants('SELECT 1\r\nFROM t;').line, 'SELECT 1 --c\n\r\nFROM t;');
  assert.equal(variants('SELECT 1 FROM t;').line, 'SELECT 1 FROM t;');
  assert.deepEqual(gaps(' \tSELECT 1;\n'), [8]);
});

const report = (path = 'a.sql') => ({
  path,
  root_has_error: false,
  error_nodes: 0,
  missing_nodes: 0,
  zero_width_nodes: 0,
  root_end: 4,
  len: 4,
  constraint_violations: [],
});
const output = (row = report(), status = 0) => ({ stdout: `${JSON.stringify(row)}\n`, status });

test('checker output validates process status, cardinality, paths and report fields', () => {
  assert.deepEqual(readVerdicts(output(), ['a.sql']), [{ parse: 'clean', rules: [] }]);
  assert.throws(() => readVerdicts(output(report(), 2), ['a.sql']), /failed without verdicts/);
  assert.throws(() => readVerdicts({ stdout: '', status: 0 }, ['a.sql']), /number of results/);
  assert.throws(() => readVerdicts(output(), ['b.sql']), /unexpected path/);
  assert.throws(
    () => readVerdicts(output({ ...report(), root_has_error: undefined }), ['a.sql']),
    /root_has_error/,
  );
  assert.throws(() => readVerdicts({ stdout: 'bad', status: 0 }, ['a.sql']), SyntaxError);
  assert.throws(
    () => readVerdicts(output({ ...report(), error_nodes: -1 }), ['a.sql']),
    /error_nodes/,
  );
});

test('parse failures and unique sorted constraint rules are compared independently', () => {
  const row = { ...report(), constraint_violations: [{ rule: 'b' }, { rule: 'a' }, { rule: 'b' }] };
  assert.deepEqual(readVerdicts(output(row, 1), ['a.sql']), [
    { parse: 'clean', rules: ['a', 'b'] },
  ]);
  for (const field of [
    'root_has_error',
    'error_nodes',
    'missing_nodes',
    'zero_width_nodes',
    'root_end',
  ]) {
    const bad = { ...report(), [field]: field === 'root_has_error' ? true : 1 };
    assert.equal(readVerdicts(output(bad, 1), ['a.sql'])[0].parse, 'ERROR', field);
  }
  const from = { parse: 'ERROR', rules: ['old'] };
  const result = compare(
    [{ id: 'case' }],
    [from, { parse: 'ERROR', rules: ['new'] }, { parse: 'clean', rules: ['old'] }],
  );
  assert.equal(result.changes.length, 2);
  assert.equal(result.parseChanges, 1);
  assert.deepEqual(result.rules, { old: { added: 0, removed: 1 }, new: { added: 1, removed: 0 } });
  assert.equal(compare([{ id: 'case' }], [from, from, from]).changes.length, 0);
});
