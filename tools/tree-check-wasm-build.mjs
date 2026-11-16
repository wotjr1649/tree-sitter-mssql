// No downloads: supply an extracted, checksum-verified WASI_SDK_PATH (see wasm-pins.json).
// Optional WASM_OPT must be Binaryen 133; omitting it always produces the unoptimized recipe.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pins = JSON.parse(readFileSync(join(root, 'tools/tree-check/wasm-pins.json'), 'utf8'));

export function validateVersions(rust, sdk, clang, optimizer) {
  if (!rust.startsWith(`rustc ${pins.rust} `))
    throw new Error(`requires Rust ${pins.rust}: ${rust}`);
  if (sdk.split(/\r?\n/)[0].trim() !== pins['wasi-sdk']) {
    throw new Error(`requires wasi-sdk ${pins['wasi-sdk']}`);
  }
  if (!clang.includes(`clang version ${pins.clang}`) || !clang.includes(pins.llvmCommit)) {
    throw new Error('clang version/LLVM commit does not match wasm-pins.json');
  }
  if (optimizer !== undefined && !new RegExp(`\\bversion ${pins.binaryen}\\b`).test(optimizer)) {
    throw new Error(`requires Binaryen ${pins.binaryen}: ${optimizer}`);
  }
}

export function pathFlags(repoRoot, cargoHome) {
  const paths = [
    [repoRoot, '.'],
    [cargoHome, '/cargo'],
  ];
  const quote = (arg) => `'${arg.replace(/'/g, "'\\''")}'`;
  return {
    rust: paths.map(([from, to]) => `--remap-path-prefix=${from}=${to}`).join('\x1f'),
    c: ['-O3', ...paths.map(([from, to]) => `-ffile-prefix-map=${from}=${to}`)]
      .map(quote)
      .join(' '),
  };
}

export function checkLocalPaths(bytes, paths) {
  for (const path of paths) {
    if ([path, path.replaceAll('\\', '/')].some((value) => bytes.includes(Buffer.from(value)))) {
      throw new Error(`module embeds a local build path: ${path}`);
    }
  }
}

function run(command, args, options = {}) {
  return execFileSync(command, args, { cwd: root, encoding: 'utf8', ...options });
}

export function build() {
  if (!process.env.WASI_SDK_PATH)
    throw new Error('set WASI_SDK_PATH to the verified wasi-sdk 34.0 directory');
  const sdk = resolve(process.env.WASI_SDK_PATH);
  const suffix = process.platform === 'win32' ? '.exe' : '';
  const cc = join(sdk, 'bin', `clang${suffix}`);
  const ar = join(sdk, 'bin', `llvm-ar${suffix}`);
  const optimizer = process.env.WASM_OPT;
  validateVersions(
    run('rustc', ['--version']).trim(),
    readFileSync(join(sdk, 'VERSION'), 'utf8'),
    run(cc, ['--version']),
    optimizer ? run(optimizer, ['--version']).trim() : undefined,
  );
  if (run('git', ['rev-parse', '--is-shallow-repository']).trim() === 'true') {
    throw new Error('fetch full Git history before building the grammar commit identity');
  }
  const grammarPaths = [
    'grammar.js',
    'grammar',
    'src/parser.c',
    'src/scanner.c',
    'src/grammar.json',
    'src/node-types.json',
  ];
  if (run('git', ['status', '--porcelain', '--', ...grammarPaths]).trim()) {
    throw new Error('commit grammar changes before building its published identity');
  }
  const grammarCommit = run('git', ['log', '-1', '--format=%H', '--', ...grammarPaths]).trim();
  if (!/^[a-f0-9]{40}$/.test(grammarCommit)) throw new Error('cannot identify grammar commit');
  const rulesHash = createHash('sha256');
  for (const file of [
    'tools/tree-check/src/constraints.rs',
    'tools/tree-check/src/lib.rs',
    'tools/tree-check/src/wasm.rs',
  ]) {
    rulesHash
      .update(file)
      .update('\0')
      .update(readFileSync(join(root, file), 'utf8').replace(/\r\n/g, '\n'))
      .update('\0');
  }
  const cargoHome = resolve(process.env.CARGO_HOME || join(homedir(), '.cargo'));
  const flags = pathFlags(root, cargoHome);
  const env = {
    ...process.env,
    TREE_CHECK_GRAMMAR_COMMIT: grammarCommit,
    TREE_CHECK_RULES_SHA256: rulesHash.digest('hex'),
    CC_wasm32_wasip1: cc,
    AR_wasm32_wasip1: ar,
    WASI_SYSROOT: join(sdk, 'share/wasi-sysroot'),
    CFLAGS_wasm32_wasip1: flags.c,
    CC_SHELL_ESCAPED_FLAGS: '1',
    CARGO_PROFILE_RELEASE_OPT_LEVEL: '3',
    CARGO_PROFILE_RELEASE_LTO: 'fat',
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS: '1',
    CARGO_PROFILE_RELEASE_PANIC: 'abort',
    CARGO_PROFILE_RELEASE_STRIP: 'symbols',
    CARGO_PROFILE_RELEASE_DEBUG: '0',
    CARGO_PROFILE_RELEASE_INCREMENTAL: 'false',
    CARGO_ENCODED_RUSTFLAGS: flags.rust,
  };
  const started = performance.now();
  run(
    'cargo',
    [
      'build',
      '--release',
      '--locked',
      '--lib',
      '--target',
      pins.target,
      '--manifest-path',
      'tools/tree-check/Cargo.toml',
    ],
    {
      env,
      stdio: 'inherit',
    },
  );
  const target = process.env.CARGO_TARGET_DIR
    ? resolve(root, process.env.CARGO_TARGET_DIR)
    : join(root, 'tools/tree-check/target');
  const raw = join(target, pins.target, 'release/tree_check.wasm');
  const output = join(target, pins.target, 'release/tree_check.reactor.wasm');
  if (optimizer) run(optimizer, [raw, '-O3', '-o', output]);
  else copyFileSync(raw, output);
  const bytes = readFileSync(output);
  checkLocalPaths(bytes, [root, cargoHome, sdk, run('rustc', ['--print', 'sysroot']).trim()]);
  const module = new WebAssembly.Module(bytes);
  const exports = WebAssembly.Module.exports(module);
  if (exports.some(({ name }) => name === '_start'))
    throw new Error('expected a reactor, found _start');
  const report = {
    output,
    recipe: optimizer ? 'rust-O3-lto-binaryen133-O3' : 'rust-O3-lto',
    optimizer: optimizer
      ? `Binaryen ${pins.binaryen}`
      : `not used; set WASM_OPT to verified Binaryen ${pins.binaryen}`,
    bytes: bytes.length,
    sha256: createHash('sha256').update(bytes).digest('hex'),
    build_ms: performance.now() - started,
    imports: WebAssembly.Module.imports(module),
    exports,
  };
  console.log(JSON.stringify(report, null, 2));
  return report;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) build();
