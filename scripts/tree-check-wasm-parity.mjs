// Dependency-free WASI reactor host and full native/WASM JSON parity gate.
// Usage: node scripts/tree-check-wasm-parity.mjs --wasm <module> --tree-check <native>
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  appendFileSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { pathToFileURL } from 'node:url';
import { WASI } from 'node:wasi';
import { readCases } from './oracle-replay.mjs';

const FUNCTIONS = [
  'tc_alloc',
  'tc_free',
  'tc_check',
  'tc_check_v2',
  'tc_result_ptr',
  'tc_result_len',
  'tc_result_free',
  'tc_identity',
];

export class Checker {
  constructor(exports) {
    for (const name of FUNCTIONS)
      assert.equal(typeof exports[name], 'function', `missing ABI export ${name}`);
    assert(exports.memory instanceof WebAssembly.Memory, 'missing exported memory');
    assert.equal(exports._start, undefined, 'expected a reactor without _start');
    this.exports = exports;
  }

  result(handle) {
    assert.notEqual(handle, 0, 'checker returned an invalid result handle');
    const e = this.exports;
    try {
      const ptr = e.tc_result_ptr(handle) >>> 0;
      const len = e.tc_result_len(handle) >>> 0;
      assert(ptr > 0 && len > 0 && ptr + len <= e.memory.buffer.byteLength, 'invalid result range');
      // Calls can grow memory, so take a new view only after obtaining the result range.
      return JSON.parse(Buffer.from(new Uint8Array(e.memory.buffer, ptr, len)).toString('utf8'));
    } finally {
      e.tc_result_free(handle);
    }
  }

  handle(source, quotedIdentifier = 'on') {
    assert(['on', 'off'].includes(quotedIdentifier), 'quoted_identifier must be on or off');
    const bytes = Buffer.from(source);
    assert(bytes.length <= 0xffffffff, 'source exceeds the wasm32 length limit');
    const e = this.exports;
    const ptr = e.tc_alloc(bytes.length) >>> 0;
    assert.notEqual(ptr, 0, 'source allocation failed');
    try {
      assert(ptr + bytes.length <= e.memory.buffer.byteLength, 'invalid allocation range');
      new Uint8Array(e.memory.buffer, ptr, bytes.length).set(bytes);
      return e.tc_check_v2(ptr, bytes.length, quotedIdentifier === 'off' ? 1 : 0);
    } finally {
      e.tc_free(ptr, bytes.length);
    }
  }

  check(source, quotedIdentifier = 'on') {
    return this.result(this.handle(source, quotedIdentifier));
  }
}

export async function loadChecker(bytes) {
  const module = await WebAssembly.compile(bytes);
  const imports = WebAssembly.Module.imports(module);
  assert(
    imports.every((i) => i.module === 'wasi_snapshot_preview1' && i.kind === 'function'),
    'unexpected non-WASI import',
  );
  const wasi = new WASI({
    version: 'preview1',
    args: [],
    env: {},
    preopens: {},
    returnOnExit: true,
  });
  const instance = await WebAssembly.instantiate(module, {
    wasi_snapshot_preview1: wasi.wasiImport,
  });
  const checker = new Checker(instance.exports);
  wasi.initialize(instance);
  const identity = checker.result(instance.exports.tc_identity());
  assert.equal(identity.abi_version, 2, 'unsupported ABI version');
  assert.match(identity.grammar_commit, /^[0-9a-f]{40}$/, 'invalid grammar commit');
  assert.match(identity.rules_sha256, /^[0-9a-f]{64}$/, 'invalid rules hash');
  return { checker, imports, identity };
}

export function checkLifecycle(checker) {
  const e = checker.exports;
  assert.equal(e.tc_check(0, 1), 0, 'unowned input must fail');
  assert.equal(e.tc_check_v2(0, 1, 1), 0, 'unowned version 2 input must fail');
  assert.equal(e.tc_result_ptr(0), 0, 'invalid result pointer must be zero');
  assert.equal(e.tc_result_len(0), 0, 'invalid result length must be zero');
  const small = e.tc_alloc(1);
  try {
    assert.equal(e.tc_check(small, 2), 0, 'input beyond its allocation must fail');
    assert.equal(e.tc_check_v2(small, 1, 2), 0, 'unknown session setting must fail');
  } finally {
    e.tc_free(small, 1);
  }
  const first = checker.handle(Buffer.from("SELECT N'한글😀';"));
  const second = checker.handle(Buffer.from('SELECT FROM;'));
  assert.notEqual(first, second, 'live result handles must be independent');
  // Force a detached old ArrayBuffer while both result buffers are still alive.
  const before = e.memory.buffer;
  e.memory.grow(1);
  assert.notEqual(e.memory.buffer, before);
  assert.equal(checker.result(second).ok, false);
  assert.equal(checker.result(first).ok, true);
  assert.equal(e.tc_result_ptr(first), 0, 'freed handle must be invalid');
  assert.equal(e.tc_result_len(first), 0, 'freed handle must be invalid');
  e.tc_result_free(first);
  assert.equal(checker.check(Buffer.alloc(0)).len, 0);
  assert.equal(checker.check(Buffer.from([0xff, 0x00, 0xfe])).len, 3);
}

export function collectInputs() {
  const files = [
    'oracle/data/kit/cases.jsonl',
    ...readdirSync('oracle/data/probes')
      .filter((f) => f.endsWith('.jsonl'))
      .sort()
      .map((f) => join('oracle/data/probes', f)),
  ];
  const cases = files.flatMap((file) => readCases(readFileSync(file, 'utf8')));
  assert(cases.length > 0, 'no oracle cases');
  assert.equal(new Set(cases.map((c) => c.id)).size, cases.length, 'duplicate oracle case IDs');
  const baseline = JSON.parse(readFileSync('oracle/data/expected.json', 'utf8')).cases;
  assert.deepEqual(
    cases.map((c) => c.id).sort(),
    [...new Set(Object.keys(baseline).map((id) => id.replace(/@qi-off$/, '')))].sort(),
    'oracle case coverage differs from the baseline',
  );
  const inputs = cases.map((c) => ({ id: c.id, source: Buffer.from(c.source) }));
  const histories = readdirSync('oracle/data/kit')
    .filter((f) => f.endsWith('edits.jsonl'))
    .sort();
  for (const file of histories) {
    for (const line of readFileSync(join('oracle/data/kit', file), 'utf8')
      .split('\n')
      .filter((l) => l.trim())) {
      const history = JSON.parse(line);
      let source = history.source;
      inputs.push({ id: `${file}:${history.id}:0`, source: Buffer.from(source) });
      for (const [i, edit] of history.edits.entries()) {
        const at = source.indexOf(edit.find);
        assert(at >= 0, `${file}:${history.id}:${i + 1}: edit target missing`);
        source = source.slice(0, at) + edit.replace + source.slice(at + edit.find.length);
        inputs.push({ id: `${file}:${history.id}:${i + 1}`, source: Buffer.from(source) });
      }
    }
  }
  const fixtureFiles = readdirSync('test/fixtures')
    .filter((f) => f.endsWith('.sql'))
    .sort();
  assert(fixtureFiles.length > 0, 'no fixtures');
  inputs.push(
    ...fixtureFiles.map((file) => ({
      id: `fixture:${file}`,
      source: readFileSync(join('test/fixtures', file)),
    })),
  );
  inputs.push(
    { id: 'abi:empty', source: Buffer.alloc(0) },
    { id: 'abi:unicode', source: Buffer.from("SELECT N'한글😀';") },
    { id: 'abi:malformed', source: Buffer.from('SELECT FROM;') },
    { id: 'abi:invalid-utf8', source: Buffer.from([0xff, 0x00, 0xfe]) },
  );
  return {
    inputs,
    oracle: cases.length,
    fixtures: fixtureFiles.length,
    historyStates: inputs.length - cases.length - fixtureFiles.length - 4,
  };
}

export function compareRows(inputs, native, wasm) {
  assert.equal(native.length, inputs.length, 'native result count differs');
  assert.equal(wasm.length, inputs.length, 'WASM result count differs');
  for (let i = 0; i < inputs.length; i++) {
    const { path: nativePath, ...expected } = native[i];
    const { path: wasmPath, ...actual } = wasm[i];
    assert.equal(typeof nativePath, 'string', `${inputs[i].id}: missing native path`);
    assert.equal(wasmPath, '', `${inputs[i].id}: WASM path must be empty`);
    assert.deepEqual(actual, expected, `${inputs[i].id}: native/WASM JSON differs`);
  }
}

async function main(argv) {
  const value = (flag) => {
    const i = argv.indexOf(flag);
    return i < 0 ? undefined : argv[i + 1];
  };
  const wasmFile = value('--wasm');
  const nativeFile = value('--tree-check');
  assert(
    wasmFile && nativeFile,
    'usage: tree-check-wasm-parity.mjs --wasm <module> --tree-check <native>',
  );
  const { checker, imports, identity } = await loadChecker(readFileSync(wasmFile));
  checkLifecycle(checker);
  const { inputs, oracle, fixtures, historyStates } = collectInputs();
  const dir = mkdtempSync(join(tmpdir(), 'tree-check-wasm-parity-'));
  let nativeMs = 0;
  let wasmMs = 0;
  try {
    const paths = inputs.map((input, i) => {
      const file = join(dir, `${i}.sql`);
      writeFileSync(file, input.source);
      return file;
    });
    const list = join(dir, 'paths.txt');
    writeFileSync(list, paths.join('\n'));
    for (const quotedIdentifier of ['on', 'off']) {
      let start = performance.now();
      const native = spawnSync(
        nativeFile,
        ['--quoted-identifier', quotedIdentifier, '--paths', list],
        {
          encoding: 'utf8',
          maxBuffer: 1 << 28,
        },
      );
      nativeMs += performance.now() - start;
      if (native.error || ![0, 1].includes(native.status))
        throw new Error(
          `native checker failed without verdicts: ${native.error?.message ?? native.stderr}`,
        );
      const nativeRows = native.stdout
        .trim()
        .split('\n')
        .filter(Boolean)
        .map((line) => JSON.parse(line));
      assert.equal(nativeRows.length, paths.length, 'native result count differs');
      nativeRows.forEach((row, i) =>
        assert.equal(row.path, paths[i], 'native output order differs'),
      );
      start = performance.now();
      const wasmRows = inputs.map((input) => checker.check(input.source, quotedIdentifier));
      wasmMs += performance.now() - start;
      compareRows(inputs, nativeRows, wasmRows);
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  const summary = [
    '### tree-check native/WASM parity',
    `- ${inputs.length * 2} checks match under QUOTED_IDENTIFIER ON and OFF: ${oracle} oracle cases, ${historyStates} edit-history states, ${fixtures} fixtures, 4 ABI edge cases per setting.`,
    `- Native CLI batch: ${nativeMs.toFixed(2)} ms (${(nativeMs / (inputs.length * 2)).toFixed(4)} ms/check; includes process startup and file I/O).`,
    `- WASM host batch: ${wasmMs.toFixed(2)} ms (${(wasmMs / (inputs.length * 2)).toFixed(4)} ms/check; includes allocation, check and JSON decoding).`,
    `- Identity: ${JSON.stringify(identity)}`,
    `- Imports: ${imports.map((i) => `${i.module}.${i.name}`).join(', ') || '(none)'}`,
    '- Reactor lifecycle, invalid handles, empty/Unicode/invalid UTF-8 inputs and memory growth passed.',
  ].join('\n');
  console.log(summary);
  if (process.env.GITHUB_STEP_SUMMARY)
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary + '\n');
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(`tree-check-wasm-parity: ${error.message}`);
    process.exitCode = 1;
  });
}
