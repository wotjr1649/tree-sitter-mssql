// Runtime cost of the parser against a milestone (ADR-0002 §6): the compiled library size, the time to
// parse every fixture and oracle case, and the peak memory of that parse where the host can measure it.
//
// The milestone's committed src/parser.c and src/scanner.c and the current ones are compiled with the
// same compiler and flags in the same job, then parse the same inputs with the same CLI. Only ratios
// are compared, so the runner's speed does not matter. A ratio above the milestone's max_ratio fails:
// growing past it needs a recorded decision and a new milestone in test/runtime-cost.json.
//
// Usage: node scripts/runtime-cost.mjs --tree-sitter <cli> [--runs N]
import { execFileSync, spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { caseSetFiles, isMain, readCases, writeSummary } from './common.mjs';

const MILESTONE = 'test/runtime-cost.json';

export function median(values) {
  const s = [...values].sort((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}

// Each metric's current/base ratio and whether it stays within max_ratio; null metrics are skipped.
export function judge(base, current, maxRatio) {
  return Object.keys(current).map((metric) => {
    const ratio = base[metric] && current[metric] ? current[metric] / base[metric] : null;
    return {
      metric,
      base: base[metric],
      current: current[metric],
      ratio,
      ok: ratio === null || ratio <= maxRatio,
    };
  });
}

function sh(cmd, args, opts = {}) {
  return execFileSync(cmd, args, { encoding: 'utf8', maxBuffer: 1 << 28, ...opts });
}

function compile(srcDir, out) {
  const cc = process.env.CC || 'cc';
  sh(cc, [
    '-O2',
    '-shared',
    '-fPIC',
    '-I',
    srcDir,
    join(srcDir, 'parser.c'),
    join(srcDir, 'scanner.c'),
    '-o',
    out,
  ]);
  return statSync(out).size;
}

// The milestone's parser sources, fetched if a shallow clone lacks the commit.
function milestoneSources(ref, dir) {
  if (spawnSync('git', ['cat-file', '-e', `${ref}^{commit}`]).status !== 0) {
    sh('git', ['fetch', '--no-tags', '--depth=1', 'origin', ref]);
  }
  mkdirSync(join(dir, 'tree_sitter'), { recursive: true });
  for (const f of ['parser.c', 'scanner.c'])
    writeFileSync(join(dir, f), sh('git', ['show', `${ref}:src/${f}`]));
  for (const f of readdirSync('src/tree_sitter')) {
    writeFileSync(join(dir, 'tree_sitter', f), sh('git', ['show', `${ref}:src/tree_sitter/${f}`]));
  }
}

// Every fixture and oracle case source, one file each.
function inputs(dir) {
  const files = readdirSync('test/fixtures')
    .filter((f) => f.endsWith('.sql'))
    .map((f) => join('test/fixtures', f));
  let n = 0;
  for (const set of caseSetFiles()) {
    for (const c of readCases(readFileSync(set, 'utf8'))) {
      const f = join(dir, `${n++}.sql`);
      writeFileSync(f, c.source);
      files.push(f);
    }
  }
  return files;
}

// Wall time of one CLI parse over all inputs, and its peak memory in KiB where GNU time exists.
function parseOnce(cli, lib, list) {
  const args = ['parse', '--lib-path', lib, '--lang-name', 'mssql', '--quiet', '--paths', list];
  const gnuTime = existsSync('/usr/bin/time');
  const start = process.hrtime.bigint();
  const r = gnuTime
    ? spawnSync('/usr/bin/time', ['-f', '%M', cli, ...args], {
        encoding: 'utf8',
        maxBuffer: 1 << 28,
      })
    : spawnSync(cli, args, { encoding: 'utf8', maxBuffer: 1 << 28 });
  const ms = Number(process.hrtime.bigint() - start) / 1e6;
  // `parse` exits 1 when an input has errors; the oracle holds rejected cases on purpose.
  if (r.status !== 0 && r.status !== 1) throw new Error(`tree-sitter parse failed: ${r.stderr}`);
  const kib = gnuTime ? Number(r.stderr.trim().split('\n').at(-1)) : null;
  return { ms, kib };
}

function main() {
  const argv = process.argv.slice(2);
  const cli = argv[argv.indexOf('--tree-sitter') + 1];
  const runs = argv.includes('--runs') ? Number(argv[argv.indexOf('--runs') + 1]) : 5;
  if (!argv.includes('--tree-sitter') || !cli)
    throw new Error('usage: runtime-cost.mjs --tree-sitter <cli> [--runs N]');
  const milestone = JSON.parse(readFileSync(MILESTONE, 'utf8'));
  const dir = mkdtempSync(join(tmpdir(), 'runtime-cost-'));
  try {
    measure(cli, runs, milestone, dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function measure(cli, runs, milestone, dir) {
  const ext = process.platform === 'win32' ? 'dll' : 'so';
  milestoneSources(milestone.commit, join(dir, 'base'));
  const baseLib = join(dir, `base.${ext}`);
  const curLib = join(dir, `current.${ext}`);
  const base = { library_bytes: compile(join(dir, 'base'), baseLib) };
  const current = { library_bytes: compile('src', curLib) };
  mkdirSync(join(dir, 'in'));
  const list = join(dir, 'paths.txt');
  writeFileSync(list, inputs(join(dir, 'in')).join('\n'));
  const samples = { base: [], current: [] };
  for (let i = 0; i < runs; i++) {
    samples.base.push(parseOnce(cli, baseLib, list));
    samples.current.push(parseOnce(cli, curLib, list));
  }
  for (const [k, s] of Object.entries(samples)) {
    const target = k === 'base' ? base : current;
    target.parse_ms = median(s.map((x) => x.ms));
    target.peak_kib = s[0].kib === null ? null : median(s.map((x) => x.kib));
  }
  const rows = judge(base, current, milestone.max_ratio);
  const fmt = (v) =>
    v === null || v === undefined ? 'n/a' : Number.isInteger(v) ? String(v) : v.toFixed(1);
  const lines = [
    `### Parser runtime cost against milestone ${milestone.label} (${milestone.commit.slice(0, 8)})`,
    `Limit: ${milestone.max_ratio}x per metric; ${runs} alternating runs, medians.`,
    '| metric | milestone | current | ratio |',
    '| --- | ---: | ---: | ---: |',
    ...rows.map(
      (r) =>
        `| ${r.metric} | ${fmt(r.base)} | ${fmt(r.current)} | ${r.ratio === null ? 'n/a' : `${r.ratio.toFixed(3)}x`}${r.ok ? '' : ' (over)'} |`,
    ),
  ];
  console.log(lines.join('\n'));
  writeSummary(lines.join('\n'));
  const over = rows.filter((r) => !r.ok);
  if (over.length) {
    console.error(
      `Runtime cost over ${milestone.max_ratio}x the milestone: ${over.map((r) => r.metric).join(', ')}. Record a decision and a new milestone (ADR-0002 §6).`,
    );
    process.exit(1);
  }
}

if (isMain(import.meta.url)) main();
