import assert from 'node:assert/strict';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { caseSetFiles, isMain, readCases, sha256, writeSummary } from './common.mjs';

const counts = { fixtures: 156, corpus: 2562, oracle: 10712 };
const modes = ['fresh', 'incremental'];
const readText = (file) => readFileSync(file, 'utf8');

export function corpusSources(text) {
  // shortcut: use the pinned corpus header format, extend the reader if the inputs adopt suffixes or multiline titles.
  const headers = [
    ...text.matchAll(/^(={3,})\r?\n[^\r\n]+\r?\n(?:[ \t]*:[^\r\n]*\r?\n)*\1\r?(?:\n|$)/gm),
  ];
  assert(headers.length, 'corpus file has no test headers');
  return headers.map((header, i) => {
    const body = text.slice(header.index + header[0].length, headers[i + 1]?.index);
    // Match the CLI: prefer the longest divider, then the last of equal lengths.
    let divider;
    for (const candidate of body.matchAll(/^(-{3,})\r?$/gm)) {
      if (!divider || candidate[1].length >= divider[1].length) divider = candidate;
    }
    assert(divider, 'corpus test is missing an input divider');
    return body.slice(0, divider.index).replace(/\r?\n$/, '');
  });
}

export function packSources(sources) {
  let bytes = 0;
  const chunks = sources.flatMap((source) => {
    const input = Buffer.from(source);
    assert(input.length < 0xffffffff, 'input has no room for a one-byte edit');
    const length = Buffer.alloc(4);
    length.writeUInt32LE(input.length);
    bytes += input.length;
    return [length, input];
  });
  return { pack: Buffer.concat(chunks), cases: sources.length, bytes };
}

function filesIn(dir, extension) {
  return readdirSync(dir)
    .filter((file) => file.endsWith(extension))
    .sort()
    .map((file) => join(dir, file));
}

function prepareInputs(work, artifacts) {
  const sets = {
    fixtures: filesIn('test/fixtures', '.sql').map((file) => readFileSync(file)),
    corpus: filesIn('test/corpus', '.txt').flatMap((file) => corpusSources(readText(file))),
    oracle: caseSetFiles().flatMap((file) => readCases(readText(file)).map((row) => row.source)),
  };
  const manifest = Object.fromEntries(
    Object.entries(sets).map(([name, sources]) => {
      assert.equal(sources.length, counts[name], 'unexpected ' + name + ' input count');
      const { pack, cases, bytes } = packSources(sources);
      writeFileSync(join(work, name + '.bin'), pack);
      return [name, { cases, bytes, sha256: sha256(pack) }];
    }),
  );
  writeFileSync(join(artifacts, 'inputs.json'), JSON.stringify(manifest, null, 2) + '\n');
}

export function parseMeasurement(profile, tally, input) {
  const instructions = Number(/^summary: (\d+)$/m.exec(profile)?.[1]);
  assert(
    /^events: Ir$/m.test(profile) && Number.isSafeInteger(instructions) && instructions > 0,
    'missing or invalid Callgrind Ir count',
  );
  const fields = /^(\d+) (\d+) (\d+)$/.exec(tally.trim());
  assert(fields, 'invalid benchmark tally');
  const [cases, bytes, errors] = fields.slice(1).map(Number);
  assert(
    cases === input.cases && bytes === input.bytes && errors <= cases,
    'invalid benchmark tally',
  );
  return { instructions, cases, bytes, errors };
}

export function budgetCheck(threshold) {
  if (threshold === '') return () => false;
  assert(/^\d+(?:\.\d+)?$/.test(threshold), 'threshold must be a nonnegative decimal percentage');
  const [whole, fraction = ''] = threshold.split('.');
  const numerator = BigInt(whole + fraction);
  const scale = 100n * 10n ** BigInt(fraction.length);
  return (instructions, baseline) =>
    BigInt(instructions - baseline) * scale > BigInt(baseline) * numerator;
}

function report(artifacts, threshold) {
  const read = (file) => readText(join(artifacts, file));
  const refs = read('refs.tsv')
    .trim()
    .split('\n')
    .map((line) => line.trimEnd().split('\t'));
  const inputs = JSON.parse(read('inputs.json'));
  const exceeds = budgetCheck(threshold);
  const rows = [];
  for (const set of Object.keys(counts)) {
    for (const mode of modes) {
      let baseline;
      for (const [i, [ref, commit]] of refs.entries()) {
        const file = i + '-' + set + '-' + mode;
        const measurement = parseMeasurement(
          read(file + '.callgrind'),
          read(file + '.tally'),
          inputs[set],
        );
        baseline ??= measurement.instructions;
        const increasePct = (100 * (measurement.instructions - baseline)) / baseline;
        rows.push({
          set,
          mode,
          ref,
          commit,
          ...measurement,
          increasePct,
          overBudget: exceeds(measurement.instructions, baseline),
        });
      }
    }
  }

  writeFileSync(join(artifacts, 'results.json'), JSON.stringify(rows, null, 2) + '\n');
  const heading = '## Parse instruction counts';
  const baselineNote =
    'Callgrind Ir change versus ' + refs[0][0] + ' for each input set and parse mode.';
  const gateNote =
    threshold === ''
      ? 'Report only. No instruction budget configured.'
      : 'Maximum allowed increase per workload: ' + threshold + '%.';
  const table = [
    heading,
    '',
    baselineNote,
    '',
    '| Input set | Mode | Ref | Cases | Error trees | Instructions | Change |',
    '| --- | --- | --- | ---: | ---: | ---: | ---: |',
    ...rows.map(
      (row) =>
        '| ' +
        [
          row.set,
          row.mode,
          row.ref + ' (' + row.commit.slice(0, 7) + ')',
          row.cases,
          row.errors,
          row.instructions,
          row.increasePct.toFixed(2) + '%' + (row.overBudget ? ' **over budget**' : ''),
        ].join(' | ') +
        ' |',
    ),
    '',
    gateNote,
  ].join('\n');
  console.log(table);
  writeSummary(table);
  return rows.some((row) => row.overBudget);
}

if (isMain(import.meta.url)) {
  const [command, path, arg = ''] = process.argv.slice(2);
  if (command === 'pack' && path && arg) prepareInputs(path, arg);
  else if (command === 'summary' && path) process.exitCode = Number(report(path, arg));
  else
    throw new Error(
      'usage: parse-bench.mjs pack WORK ARTIFACTS | summary ARTIFACTS [MAX_INCREASE_PCT]',
    );
}
