import assert from 'node:assert/strict';
import test from 'node:test';
import { checkLocalPaths, pathFlags, validateVersions } from './tree-check-wasm-build.mjs';

const rust = 'rustc 1.100.0 (0ef1bc386 2026-11-09)';
const sdk = '34.0\nwasi-libc: 2e6fb9d8ee0c\n';
const clang =
  'clang version 23.1.0-rc3 (https://github.com/llvm/llvm-project 895aa2c896ada719451be2e3673c83da8ddf1141)';

test('the build refuses unpinned compilers and optional optimizer versions', () => {
  validateVersions(rust, sdk, clang);
  validateVersions(rust, sdk, clang, 'wasm-opt version 133');
  assert.throws(() => validateVersions('rustc 1.99.0 (x)', sdk, clang), /requires Rust/);
  assert.throws(() => validateVersions(rust, '33.0', clang), /requires wasi-sdk/);
  assert.throws(
    () => validateVersions(rust, sdk, clang.replace('895aa2', '000000')),
    /LLVM commit/,
  );
  assert.throws(
    () => validateVersions(rust, sdk, clang, 'wasm-opt version 132'),
    /requires Binaryen/,
  );
});

test('remaps repository and Cargo paths without splitting spaces or backslashes', () => {
  const repo = String.raw`C:\work repo`;
  const cargo = String.raw`C:\user's cargo`;
  const flags = pathFlags(repo, cargo);
  assert.deepEqual(flags.rust.split('\x1f'), [
    `--remap-path-prefix=${repo}=.`,
    `--remap-path-prefix=${cargo}=/cargo`,
  ]);
  assert.equal(
    flags.c,
    String.raw`'-O3' '-ffile-prefix-map=C:\work repo=.' '-ffile-prefix-map=C:\user'\''s cargo=/cargo'`,
  );
  assert.throws(() => checkLocalPaths(Buffer.from(repo), [repo]), /local build path/);
  assert.throws(
    () => checkLocalPaths(Buffer.from(repo.replaceAll('\\', '/')), [repo]),
    /local build path/,
  );
  checkLocalPaths(Buffer.from('/cargo/registry/src/lib.rs'), [repo, cargo]);
});
