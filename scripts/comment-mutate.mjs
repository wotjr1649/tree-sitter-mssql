// Equivalent comments, keyword case and token-gap whitespace must preserve every verdict, even for invalid input.
// Usage: node scripts/comment-mutate.mjs --tree-check <exe> [--axes comment,case,whitespace] [--quoted-identifier on|off] [inputs.jsonl | sql-directory ...]
// Defaults: every oracle case and SQL fixture. GO/sqlcmd lines are client directives, not token gaps.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  appendFileSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { pathToFileURL } from 'node:url';
import { readCases } from './oracle-replay.mjs';

// LocalDB 17.0.1000.7, levels 100-170: oracle/data/probes/metamorphic-whitespace.jsonl.
// The engine takes control characters U+0000-0020 as separators too, so the class names them on purpose.
const SQL_WHITESPACE =
  // eslint-disable-next-line no-control-regex
  /[\u0000-\u0020\u0085\u00a0\u1680\u2000-\u200b\u2028\u2029\u202f\u205f\u3000]/;

export const AXES = {
  comment: ['block', 'line'],
  case: ['upper', 'lower', 'mixed'],
  whitespace: ['space', 'newline', 'tab-crlf'],
};

function whitespaceRuns(sql) {
  const at = [];
  let i = 0;
  let lineCommentEnd = -1;
  const directive = (p) =>
    /^(?:[^\S\r\n])*(?:GO\b|:)/i.test(
      sql.slice(Math.max(sql.lastIndexOf('\n', p - 1), sql.lastIndexOf('\r', p - 1)) + 1),
    );
  while (i < sql.length) {
    const c = sql[i];
    if (/^0[xX]/.test(sql.slice(i)) && (i === 0 || !/[\p{ID_Continue}@$#]/u.test(sql[i - 1]))) {
      // SQL Server 17.0.1000.7, levels 100-170: a binary literal's backslash-newline is inside its token.
      i += /^0[xX][0-9A-Fa-f]*(?:\\\r?\n[0-9A-Fa-f]+)*/.exec(sql.slice(i))[0].length;
    } else if (c === "'" || c === '"' || c === '[') {
      const close = c === '[' ? ']' : c;
      i++;
      while (i < sql.length) {
        if (sql[i++] === close) {
          if (sql[i] !== close) break;
          i++;
        }
      }
    } else if (sql.startsWith('--', i)) {
      const end = /[\r\n]/.exec(sql.slice(i));
      if (!end) break;
      i += end.index;
      lineCommentEnd = i;
    } else if (sql.startsWith('/*', i)) {
      let depth = 0;
      while (i < sql.length) {
        if (sql.startsWith('/*', i)) {
          depth++;
          i += 2;
        } else if (sql.startsWith('*/', i)) {
          i += 2;
          if (!--depth) break;
        } else i++;
      }
    } else if (SQL_WHITESPACE.test(c)) {
      const start = i;
      while (i < sql.length && SQL_WHITESPACE.test(sql[i])) i++;
      if (start > 0 && i < sql.length && !directive(start) && !directive(i))
        at.push([start, i, start === lineCommentEnd]);
    } else i++;
  }
  return at;
}

export function gaps(sql) {
  return whitespaceRuns(sql).map(([start]) => start);
}

export function variants(sql, keywordRanges = [], tokenRanges = keywordRanges) {
  const runs = whitespaceRuns(sql);
  const positions = runs.map(([start]) => start);
  const insert = (at, text) => {
    let last = 0;
    let result = '';
    for (const p of at) {
      result += sql.slice(last, p) + text;
      last = p;
    }
    return result + sql.slice(last);
  };
  const result = {
    orig: sql,
    block: insert(positions, ' /*c*/'),
    line: insert(
      positions.filter((p) => /^[ \t]*(?:\r\n?|\n)/.test(sql.slice(p))),
      ' --c\n',
    ),
  };
  // tree-check reports byte ranges; Buffer also keeps non-ASCII identifiers unchanged.
  const bytes = Buffer.from(sql);
  for (const kind of AXES.case) {
    const mutated = Buffer.from(bytes);
    for (const [start, end] of keywordRanges) {
      for (let i = start; i < end; i++) {
        const c = bytes[i];
        if ((c >= 65 && c <= 90) || (c >= 97 && c <= 122)) {
          mutated[i] =
            kind === 'upper' || (kind === 'mixed' && (i - start) % 2 === 0) ? c & ~32 : c | 32;
        }
      }
    }
    result[kind] = mutated.toString();
  }
  for (const [kind, text] of [
    ['space', ' '],
    ['newline', '\n'],
    ['tab-crlf', '\t\r\n'],
  ]) {
    let last = 0;
    let mutated = '';
    for (const [start, end, afterLineComment] of runs) {
      const byte = Buffer.byteLength(sql.slice(0, start));
      // Atomic multi-word keywords and scanner operators can contain whitespace.
      if (tokenRanges.some(([a, b]) => a < byte && byte < b)) continue;
      // The newline ending -- is part of the client's lexical boundary.
      const newline = afterLineComment ? /^(?:\r\n?|\n)/.exec(sql.slice(start))[0] : '';
      mutated += sql.slice(last, start) + newline + text;
      last = end;
    }
    result[kind] = mutated + sql.slice(last);
  }
  return result;
}

export function readVerdicts(result, paths) {
  assert(
    !result.error && !result.signal && [0, 1].includes(result.status),
    `tree-check failed without verdicts: ${result.error?.message || result.stderr || result.signal || result.status}`,
  );
  const rows = result.stdout
    .split('\n')
    .filter((line) => line.trim())
    .map((line) => JSON.parse(line));
  assert.equal(rows.length, paths.length, 'tree-check returned a different number of results');
  return rows.map((row, i) => {
    assert.equal(row.path, paths[i], 'tree-check returned an unexpected path');
    assert.equal(typeof row.root_has_error, 'boolean', 'invalid root_has_error');
    for (const field of ['error_nodes', 'missing_nodes', 'zero_width_nodes', 'root_end', 'len'])
      assert(Number.isSafeInteger(row[field]) && row[field] >= 0, `invalid ${field}`);
    assert(Array.isArray(row.constraint_violations), 'invalid constraint_violations');
    const rules = [
      ...new Set(
        row.constraint_violations.map((v) => {
          assert(typeof v.rule === 'string' && v.rule.length, 'invalid constraint rule');
          return v.rule;
        }),
      ),
    ].sort();
    return {
      parse:
        row.root_has_error ||
        row.error_nodes > 0 ||
        row.missing_nodes > 0 ||
        row.zero_width_nodes > 0 ||
        row.root_end !== row.len
          ? 'ERROR'
          : 'clean',
      rules,
    };
  });
}

export function compare(inputs, verdicts, kinds = AXES.comment) {
  const width = kinds.length + 1;
  assert.equal(verdicts.length, inputs.length * width, 'missing mutation verdicts');
  const changes = [];
  const rules = {};
  let parseChanges = 0;
  inputs.forEach((input, i) => {
    const from = verdicts[i * width];
    for (const [index, variant] of kinds.entries()) {
      const to = verdicts[i * width + index + 1];
      if (JSON.stringify(from) === JSON.stringify(to)) continue;
      changes.push({ id: input.id, variant, from, to });
      if (from.parse !== to.parse) parseChanges++;
      for (const rule of new Set([...from.rules, ...to.rules])) {
        if (from.rules.includes(rule) === to.rules.includes(rule)) continue;
        rules[rule] ??= { added: 0, removed: 0 };
        rules[rule][to.rules.includes(rule) ? 'added' : 'removed']++;
      }
    }
  });
  return { changes, rules, parseChanges };
}

export function checkCases(checker, inputs, axes = Object.keys(AXES), quotedIdentifier = 'on') {
  assert(['on', 'off'].includes(quotedIdentifier), 'quoted_identifier must be on or off');
  assert(inputs.length > 0, 'no mutation inputs');
  assert.equal(
    new Set(inputs.map((c) => c.id)).size,
    inputs.length,
    'duplicate mutation input IDs',
  );
  const dir = mkdtempSync(join(tmpdir(), 'comment-mutate-'));
  try {
    const list = join(dir, 'paths.txt');
    const originalPaths = inputs.map((c, i) => {
      const path = join(dir, `${i}.orig.sql`);
      writeFileSync(path, c.source);
      return path;
    });
    const run = (paths, args = []) => {
      writeFileSync(list, paths.join('\n'));
      return spawnSync(
        checker,
        ['--quoted-identifier', quotedIdentifier, ...args, '--paths', list],
        {
          encoding: 'utf8',
          maxBuffer: 1 << 28,
          timeout: 180000,
        },
      );
    };
    const originals = run(originalPaths, ['--keyword-ranges']);
    readVerdicts(originals, originalPaths);
    const ranges = originals.stdout
      .trim()
      .split('\n')
      .map((line) => {
        const row = JSON.parse(line);
        assert(
          Array.isArray(row.keyword_ranges) && Array.isArray(row.token_ranges),
          'tree-check must support --keyword-ranges',
        );
        return row;
      });
    const kinds = axes.flatMap((axis) => {
      assert(AXES[axis], `unknown mutation axis: ${axis}`);
      return AXES[axis];
    });
    const paths = [];
    const seen = new Map();
    const indices = inputs.flatMap((c, i) => {
      const all = variants(c.source, ranges[i].keyword_ranges, ranges[i].token_ranges);
      return ['orig', ...kinds].map((kind) => {
        const source = all[kind];
        if (seen.has(source)) return seen.get(source);
        const path = join(dir, `${i}.${kind}.sql`);
        if (kind !== 'orig') writeFileSync(path, source);
        seen.set(source, paths.length);
        paths.push(path);
        return paths.length - 1;
      });
    });
    const uniqueVerdicts = readVerdicts(run(paths), paths);
    const result = compare(
      inputs,
      indices.map((i) => uniqueVerdicts[i]),
      kinds,
    );
    result.axes = Object.fromEntries(
      axes.map((axis) => [
        axis,
        {
          variants: inputs.length * AXES[axis].length,
          changes: result.changes.filter((c) => AXES[axis].includes(c.variant)).length,
        },
      ]),
    );
    return result;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function main(argv) {
  assert.equal(
    argv[0],
    '--tree-check',
    'usage: comment-mutate.mjs --tree-check <exe> [inputs.jsonl | sql-directory ...]',
  );
  assert(argv[1], '--tree-check needs an executable');
  let axes = Object.keys(AXES);
  let quotedIdentifier = 'on';
  let sources = argv.slice(2);
  while (sources[0]?.startsWith('--')) {
    const [flag, value] = sources;
    assert(value, `${flag} needs a value`);
    if (flag === '--axes') {
      axes = value.split(',');
      assert.equal(new Set(axes).size, axes.length, 'duplicate mutation axes');
    } else if (flag === '--quoted-identifier') {
      quotedIdentifier = value;
    } else throw new Error(`unknown option: ${flag}`);
    sources = sources.slice(2);
  }
  sources = sources.length
    ? sources
    : [
        'oracle/data/kit/cases.jsonl',
        ...readdirSync('oracle/data/probes')
          .filter((f) => f.endsWith('.jsonl'))
          .sort()
          .map((f) => join('oracle/data/probes', f)),
        'test/fixtures',
      ];
  const inputs = sources.flatMap((source) =>
    statSync(source).isDirectory()
      ? readdirSync(source)
          .filter((f) => f.endsWith('.sql'))
          .sort()
          .map((f) => ({ id: join(source, f), source: readFileSync(join(source, f), 'utf8') }))
      : readCases(readFileSync(source, 'utf8')),
  );
  const started = performance.now();
  const result = checkCases(resolve(argv[1]), inputs, axes, quotedIdentifier);
  const summary = `Mutation (QUOTED_IDENTIFIER ${quotedIdentifier.toUpperCase()}): ${inputs.length} inputs, ${result.changes.length} changed verdicts, ${result.parseChanges} parse changes (${((performance.now() - started) / 1000).toFixed(2)} s)`;
  console.log(summary);
  for (const [axis, counts] of Object.entries(result.axes))
    console.log(`${axis}: ${counts.variants} variants, ${counts.changes} changed verdicts`);
  for (const [rule, counts] of Object.entries(result.rules).sort(([a], [b]) => a.localeCompare(b)))
    console.log(`${rule}: +${counts.added} -${counts.removed}`);
  for (const change of result.changes) console.log(JSON.stringify(change));
  if (process.env.GITHUB_STEP_SUMMARY)
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${summary}\n`);
  process.exitCode = result.changes.length ? 1 : 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(`comment-mutate: ${error.message}`);
    process.exitCode = 2;
  }
}
