// Generated-parser cost against the committed baseline test/parser-cost.json (ADR-0002 §6).
//
// Reports the size of src/parser.c, its table counts, and the field-map entries (all and inherited);
// inherited entries are what consumers such as go-treesitter pay for in field lookups.
// Check mode fails on any difference from the baseline, so the baseline always describes `main`.
// --update records the current values: growth above 5 % in any metric warns, and growth above 10 %
// is refused unless --approve-budget is given (the pull request must say which cells it adds).
//
// Usage: node scripts/parser-cost.mjs [--update [--approve-budget]]
import { appendFileSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const DEFINES = [
  'STATE_COUNT',
  'LARGE_STATE_COUNT',
  'SYMBOL_COUNT',
  'TOKEN_COUNT',
  'FIELD_COUNT',
  'PRODUCTION_ID_COUNT',
];
const BASELINE = 'test/parser-cost.json';

export function measure(parserC, bytes) {
  const cost = { parser_c_bytes: bytes };
  for (const name of DEFINES) {
    const m = parserC.match(new RegExp(`^#define ${name} (\\d+)$`, 'm'));
    if (!m) throw new Error(`src/parser.c has no #define ${name}`);
    cost[name.toLowerCase()] = Number(m[1]);
  }
  const start = parserC.indexOf('static const TSFieldMapEntry ts_field_map_entries[] = {');
  const block = start < 0 ? '' : parserC.slice(start, parserC.indexOf('\n};', start));
  cost.field_map_entries = (block.match(/^\s*\{field_/gm) ?? []).length;
  cost.inherited_field_map_entries = (block.match(/\.inherited = true/g) ?? []).length;
  return cost;
}

// Relative growth per metric, as a fraction (0.05 = +5 %).
export function growth(baseline, current) {
  return Object.fromEntries(
    Object.keys(current).map((k) => [
      k,
      baseline[k] ? (current[k] - baseline[k]) / baseline[k] : current[k] ? Infinity : 0,
    ]),
  );
}

function main(argv) {
  const update = argv.includes('--update');
  const approved = argv.includes('--approve-budget');
  const current = measure(readFileSync('src/parser.c', 'utf8'), statSync('src/parser.c').size);
  const baseline = JSON.parse(readFileSync(BASELINE, 'utf8'));
  const g = growth(baseline, current);
  const rows = Object.keys(current).map((k) => {
    const pct = Number.isFinite(g[k]) ? `${(g[k] * 100).toFixed(2)} %` : 'new';
    return `| ${k} | ${baseline[k] ?? '-'} | ${current[k]} | ${pct} |`;
  });
  const summary = [
    '### Generated parser cost',
    '| metric | baseline | current | change |',
    '| --- | ---: | ---: | ---: |',
    ...rows,
  ].join('\n');
  console.log(summary);
  if (process.env.GITHUB_STEP_SUMMARY)
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary + '\n');
  const over = (limit) => Object.keys(g).filter((k) => g[k] > limit);
  for (const k of over(0.05))
    console.log(`::warning::${k} grew ${(g[k] * 100).toFixed(2)} % over the baseline`);
  if (update) {
    if (over(0.1).length && !approved) {
      console.error(
        `Refusing to update: ${over(0.1).join(', ')} grew more than 10 %. Name the cells this adds in the pull request and pass --approve-budget.`,
      );
      return 1;
    }
    writeFileSync(BASELINE, JSON.stringify(current, null, 2) + '\n');
    console.log(`Updated ${BASELINE}`);
    return 0;
  }
  const changed = Object.keys(current).filter((k) => current[k] !== baseline[k]);
  if (changed.length) {
    console.error(
      `Parser cost differs from ${BASELINE} (${changed.join(', ')}). Review it, then run with --update in the same pull request.`,
    );
    return 1;
  }
  return 0;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (e) {
    console.error(`parser-cost: ${e.message}`);
    process.exitCode = 2;
  }
}
