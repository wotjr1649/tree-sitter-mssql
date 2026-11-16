// Compares two verdict files made from the same cases (for example LocalDB on Windows against a
// Linux container, or two engine builds) and reports, per shared compatibility level, the cases whose
// error numbers differ. Differences are observations to classify, not failures: exit 0 when the files
// are comparable, 2 when they are not (different cases, or not PARSEONLY verdicts).
//
// Usage: node scripts/verdict-diff.mjs <base.json> <other.json>
import { readFileSync } from 'node:fs';
import { isMain, writeSummary } from './common.mjs';
import { quotedIdentifier } from './oracle-replay.mjs';

export function diff(base, other) {
  if (base.meta.mode !== 'PARSEONLY' || other.meta.mode !== 'PARSEONLY')
    throw new Error('only PARSEONLY verdicts compare');
  if (quotedIdentifier(base.meta) !== quotedIdentifier(other.meta))
    throw new Error('the verdicts were made with different QUOTED_IDENTIFIER sessions');
  if (base.meta.cases_sha256 !== other.meta.cases_sha256)
    throw new Error('the verdicts were made from different cases');
  const levels = base.meta.levels.filter((l) => other.meta.levels.includes(l)).map(String);
  return levels.map((level) => ({
    level,
    differing: Object.keys(base.verdicts)
      .filter(
        (id) =>
          JSON.stringify(base.verdicts[id][level]) !== JSON.stringify(other.verdicts[id]?.[level]),
      )
      .map((id) => ({ id, base: base.verdicts[id][level], other: other.verdicts[id]?.[level] })),
  }));
}

const engine = (v) =>
  `${v.meta.engine.product_version} ${v.meta.engine.product_level} ${v.meta.engine.edition} on ${v.meta.engine.platform}`;

if (isMain(import.meta.url)) {
  try {
    const [a, b] = process.argv.slice(2).map((f) => JSON.parse(readFileSync(f, 'utf8')));
    if (!a || !b) throw new Error('usage: verdict-diff.mjs <base.json> <other.json>');
    const rows = diff(a, b);
    const lines = [
      '### Verdict comparison',
      `- base: ${engine(a)}`,
      `- other: ${engine(b)}`,
      ...rows.map((r) => `- level ${r.level}: ${r.differing.length} differing cases`),
      ...rows.flatMap((r) =>
        r.differing
          .slice(0, 50)
          .map(
            (d) =>
              `  - ${r.level} ${d.id}: ${JSON.stringify(d.base)} -> ${JSON.stringify(d.other)}`,
          ),
      ),
    ].join('\n');
    console.log(lines);
    writeSummary(lines);
  } catch (e) {
    console.error(`verdict-diff: ${e.message}`);
    process.exitCode = 2;
  }
}
