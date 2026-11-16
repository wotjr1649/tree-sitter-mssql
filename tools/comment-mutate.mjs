// Comments at token gaps must preserve parse state and constraint-rule sets, including invalid inputs.
// Usage: node tools/comment-mutate.mjs --tree-check <exe> [inputs.jsonl | sql-directory ...]
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

export function gaps(sql) {
  const at = [];
  let i = 0;
  const directive = (p) =>
    /^(?:[^\S\r\n])*(?:GO\b|:)/i.test(sql.slice(sql.lastIndexOf('\n', p - 1) + 1));
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
      i = sql.indexOf('\n', i);
      if (i < 0) break;
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
    } else if (/\s/.test(c)) {
      const start = i;
      while (i < sql.length && /\s/.test(sql[i])) i++;
      if (start > 0 && i < sql.length && !directive(start) && !directive(i)) at.push(start);
    } else i++;
  }
  return at;
}

export function variants(sql) {
  const positions = gaps(sql);
  const insert = (at, text) => {
    let last = 0;
    let result = '';
    for (const p of at) {
      result += sql.slice(last, p) + text;
      last = p;
    }
    return result + sql.slice(last);
  };
  return {
    orig: sql,
    block: insert(positions, ' /*c*/'),
    line: insert(
      positions.filter((p) => /^[ \t]*\r?\n/.test(sql.slice(p))),
      ' --c\n',
    ),
  };
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

export function compare(inputs, verdicts) {
  assert.equal(verdicts.length, inputs.length * 3, 'missing mutation verdicts');
  const changes = [];
  const rules = {};
  let parseChanges = 0;
  inputs.forEach((input, i) => {
    const from = verdicts[i * 3];
    for (const [offset, variant] of [
      [1, 'block'],
      [2, 'line'],
    ]) {
      const to = verdicts[i * 3 + offset];
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

export function checkCases(checker, inputs) {
  assert(inputs.length > 0, 'no mutation inputs');
  assert.equal(
    new Set(inputs.map((c) => c.id)).size,
    inputs.length,
    'duplicate mutation input IDs',
  );
  const dir = mkdtempSync(join(tmpdir(), 'comment-mutate-'));
  try {
    const paths = inputs.flatMap((c, i) =>
      Object.entries(variants(c.source)).map(([kind, source]) => {
        const path = join(dir, `${i}.${kind}.sql`);
        writeFileSync(path, source);
        return path;
      }),
    );
    const list = join(dir, 'paths.txt');
    writeFileSync(list, paths.join('\n'));
    return compare(
      inputs,
      readVerdicts(
        spawnSync(checker, ['--paths', list], {
          encoding: 'utf8',
          maxBuffer: 1 << 28,
          timeout: 180000,
        }),
        paths,
      ),
    );
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
  const sources =
    argv.length > 2
      ? argv.slice(2)
      : [
          'oracle/kit/cases.jsonl',
          ...readdirSync('oracle/probes')
            .filter((f) => f.endsWith('.jsonl'))
            .sort()
            .map((f) => join('oracle/probes', f)),
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
  const result = checkCases(resolve(argv[1]), inputs);
  const summary = `Comment mutation: ${inputs.length} inputs, ${inputs.length * 2} variants, ${result.changes.length} changed verdicts, ${result.parseChanges} parse changes (${((performance.now() - started) / 1000).toFixed(2)} s)`;
  console.log(summary);
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
