import assert from 'node:assert/strict';
import test from 'node:test';
import { Checker, collectInputs, compareRows, loadChecker } from './tree-check-wasm-parity.mjs';

test('parity compares every JSON field except the host-specific path', () => {
  const inputs = [{ id: 'constraint-detail' }];
  const report = {
    path: '/native/input.sql',
    ok: false,
    constraint_violations: [{ rule: 'r', detail: 'same', row: 2, column: 3 }],
  };
  const wasm = { ...report, path: '' };
  assert.doesNotThrow(() => compareRows(inputs, [report], [wasm]));
  assert.throws(
    () =>
      compareRows(
        inputs,
        [report],
        [
          {
            ...wasm,
            constraint_violations: [{ ...report.constraint_violations[0], detail: 'different' }],
          },
        ],
      ),
    /constraint-detail: native\/WASM JSON differs/,
  );
  assert.throws(() => compareRows(inputs, [], [wasm]), /native result count differs/);
  assert.throws(
    () => compareRows(inputs, [report], [{ ...wasm, path: '/unexpected' }]),
    /WASM path must be empty/,
  );
});

test('the host rejects malformed modules, missing exports and foreign imports', async () => {
  await assert.rejects(loadChecker(Uint8Array.of(0)), WebAssembly.CompileError);
  const empty = [0, 97, 115, 109, 1, 0, 0, 0];
  await assert.rejects(loadChecker(Uint8Array.from(empty)), /missing ABI export/);
  const foreignImport = [...empty, 1, 4, 1, 96, 0, 0, 2, 7, 1, 1, 120, 1, 102, 0, 0];
  await assert.rejects(loadChecker(Uint8Array.from(foreignImport)), /unexpected non-WASI import/);
});

test('the host refreshes memory views after growth and releases input/result buffers', () => {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const freed = [];
  const result = Buffer.from('{"path":"","ok":true}');
  const checker = new Checker({
    memory,
    tc_alloc: () => 8,
    tc_free: (ptr, len) => freed.push(['input', ptr, len]),
    tc_check: () => {
      memory.grow(1);
      new Uint8Array(memory.buffer, 100, result.length).set(result);
      return 7;
    },
    tc_result_ptr: () => 100,
    tc_result_len: () => result.length,
    tc_result_free: (handle) => freed.push(['result', handle]),
    tc_identity: () => 7,
  });
  assert.deepEqual(checker.check(Buffer.from('SELECT 1;')), { path: '', ok: true });
  assert.deepEqual(freed, [
    ['input', 8, 9],
    ['result', 7],
  ]);
  new Uint8Array(memory.buffer, 100, result.length).fill(0);
  assert.throws(() => checker.result(7), SyntaxError);
  assert.deepEqual(freed.at(-1), ['result', 7]);
});

test('the gate covers the full recorded oracle and all fixture/history states', () => {
  const { inputs, oracle, fixtures, historyStates } = collectInputs();
  assert(oracle > 0 && fixtures > 0 && historyStates > 0);
  assert.equal(inputs.length, oracle + fixtures + historyStates + 4);
  assert.equal(new Set(inputs.map((input) => input.id)).size, inputs.length);
  assert(inputs.every((input) => Buffer.isBuffer(input.source)));
});
