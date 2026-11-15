// Generate identifier-character probes for every BMP code point except surrogates and C0 controls.
// s-XXXX: the character starts a regular identifier (alias); c-XXXX: it continues one.
import { writeFileSync } from 'node:fs';

const lines = [];
for (let cp = 0x20; cp <= 0xffff; cp++) {
  if (cp >= 0xd800 && cp <= 0xdfff) continue;
  const ch = String.fromCharCode(cp);
  const hex = cp.toString(16).toUpperCase().padStart(4, '0');
  lines.push(JSON.stringify({ id: `s-${hex}`, source: `SELECT 1 AS ${ch};` }));
  lines.push(JSON.stringify({ id: `c-${hex}`, source: `SELECT 1 AS a${ch}b;` }));
}
writeFileSync(process.argv[2], lines.join('\n') + '\n');
console.log(lines.length);
