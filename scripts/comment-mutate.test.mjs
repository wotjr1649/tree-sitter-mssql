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

test('CR-only client lines and line-comment boundaries stay separate', () => {
  const sql = 'SELECT 1; -- tail\r SELECT 2;\r GO 2\r:r file.sql\rSELECT 3;';
  const all = variants(sql);
  for (const value of Object.values(all)) assert(value.includes('\r GO 2\r:r file.sql\r'));
  assert(all.space.includes('-- tail\r SELECT 2;'));
  assert(all.newline.includes('-- tail\r\nSELECT\n2;'));
});

test('case axes change only tree keyword byte ranges, preserving Unicode and protected text', () => {
  const sql = `select 한글, [SELECT], "FROM", 'WHERE' from MixedName; -- SELECT`;
  const from = Buffer.byteLength(sql.slice(0, sql.indexOf('from MixedName')));
  const all = variants(sql, [
    [0, 6],
    [from, from + 4],
  ]);
  assert.equal(
    all.upper,
    sql.replace('select', 'SELECT').replace('from MixedName', 'FROM MixedName'),
  );
  assert.equal(all.lower, sql);
  assert.equal(
    all.mixed,
    sql.replace('select', 'SeLeCt').replace('from MixedName', 'FrOm MixedName'),
  );
});

test('JavaScript Unicode whitespace is not assumed to be SQL token-gap whitespace', () => {
  for (const separator of ['\ufeff', '\u180e']) {
    for (const source of Object.values(variants(`SELECT 1 WHERE 1 !${separator}= 2;`)))
      assert(source.includes(`!${separator}=`));
  }
});

test('whitespace axes replace whole runs but preserve tokens, client lines and line-comment terminators', () => {
  for (const newline of ['\n', '\r\n']) {
    const sql = `SELECT\t 1, 'a  b', [a  b], 0x12\\${newline}34; -- tail${newline}  SELECT 2;${newline} GO 2${newline}:r a file.sql${newline}SELECT 3;`;
    const all = variants(sql);
    for (const [kind, gap] of [
      ['space', ' '],
      ['newline', '\n'],
      ['tab-crlf', '\t\r\n'],
    ]) {
      assert(
        all[kind].startsWith(
          `SELECT${gap}1,${gap}'a  b',${gap}[a  b],${gap}0x12\\${newline}34;${gap}-- tail${newline}${gap}SELECT${gap}2;`,
        ),
        kind,
      );
      assert(all[kind].includes(`${newline} GO 2${newline}:r a file.sql${newline}`), kind);
    }
  }
  assert.equal(
    variants('SELECT 1 FOR  SYSTEM_TIME AS OF 1;', [[9, 25]]).space,
    'SELECT 1 FOR  SYSTEM_TIME AS OF 1;',
  );
  assert.equal(variants('SELECT 1 !\t = 2;', [], [[9, 13]]).space, 'SELECT 1 !\t = 2;');
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

test('engine-measured Unicode and control separators are normalized outside tokens', () => {
  for (const separator of ['\u0000', '\u001f', '\u0085', '\u00a0', '\u2002', '\u200b', '\u2028']) {
    assert.equal(variants(`SELECT${separator}1;`).space, 'SELECT 1;');
  }
});
