import assert from 'node:assert/strict';
import test from 'node:test';
import { classBody, classes, grammarConstant } from './identifier-charclass.mjs';

test('ranges become a class body, with excluded code points cut out', () => {
  assert.equal(
    classBody([
      [0x41, 0x43],
      [0x5f, 0x5f],
    ]),
    '\\u0041-\\u0043\\u005F',
  );
  assert.equal(classBody([[0x41, 0x43]], [0x42]), '\\u0041\\u0043');
  assert.equal(classBody([[0x23, 0x24]], [0x23]), '\\u0024');
});

test('classes apply the recorded exclusions per position', () => {
  const data = {
    meta: { excluded: { start: [0x23], continue: [0x2c] } },
    start: [
      [0x23, 0x23],
      [0x41, 0x41],
    ],
    continue: [
      [0x2c, 0x2c],
      [0x30, 0x30],
    ],
  };
  assert.deepEqual(classes(data), { start: '\\u0041', continue: '\\u0030' });
});

test('reads the grammar constant with its escapes decoded', () => {
  const source = "export const IDENTIFIER_START =\n  '\\\\u0041-\\\\u005A';\n";
  assert.equal(grammarConstant(source, 'IDENTIFIER_START'), '\\u0041-\\u005A');
  assert.throws(() => grammarConstant('', 'IDENTIFIER_START'), /no IDENTIFIER_START/);
});
