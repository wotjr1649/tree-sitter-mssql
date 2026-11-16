// Identifier character classes from the measured engine verdicts (oracle/data/unicode/identifier-chars.json).
//
// The grammar's IDENTIFIER_START and IDENTIFIER_CONTINUE (grammar/helpers.js) must equal the classes
// derived here: `--check` fails when they drift, `--print` prints the two strings to paste.
// Usage: node scripts/identifier-charclass.mjs --check | --print
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const esc = (cp) => `\\u${cp.toString(16).toUpperCase().padStart(4, '0')}`;

// Ranges [[first, last], ...] minus the excluded code points, as a regex class body.
export function classBody(ranges, exclude = []) {
  const skip = new Set(exclude);
  const out = [];
  for (const [a, b] of ranges) {
    let start = null;
    for (let cp = a; cp <= b + 1; cp++) {
      const keep = cp <= b && !skip.has(cp);
      if (keep && start === null) start = cp;
      if (!keep && start !== null) {
        out.push(start === cp - 1 ? esc(start) : `${esc(start)}-${esc(cp - 1)}`);
        start = null;
      }
    }
  }
  return out.join('');
}

export function classes(data) {
  return {
    start: classBody(data.start, data.meta.excluded.start),
    continue: classBody(data.continue, data.meta.excluded.continue),
  };
}

// The string literal assigned to `name` in grammar/helpers.js, with JavaScript escapes decoded.
export function grammarConstant(source, name) {
  const m = source.match(new RegExp(`export const ${name} =\\s*'([^']*)';`));
  if (!m) throw new Error(`grammar/helpers.js has no ${name} string`);
  return m[1].replace(/\\\\/g, '\\');
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const data = JSON.parse(readFileSync('oracle/data/unicode/identifier-chars.json', 'utf8'));
  const want = classes(data);
  if (process.argv.includes('--print')) {
    console.log(
      `IDENTIFIER_START: ${want.start.length} chars, IDENTIFIER_CONTINUE: ${want.continue.length} chars`,
    );
    console.log(JSON.stringify(want));
  } else {
    const helpers = readFileSync('grammar/helpers.js', 'utf8');
    const drift = ['start', 'continue'].filter(
      (k) => grammarConstant(helpers, `IDENTIFIER_${k.toUpperCase()}`) !== want[k],
    );
    if (drift.length) {
      console.error(
        `grammar/helpers.js differs from oracle/data/unicode/identifier-chars.json: ${drift.join(', ')}`,
      );
      process.exitCode = 1;
    } else {
      console.log('Identifier character classes match the engine-measured sets.');
    }
  }
}
