// Second opinion (ADR-0004 release criterion): compare the engine verdicts, Microsoft's ScriptDOM parser
// (oracle/data/scriptdom-verdicts, made by oracle/runners/ScriptDomCheck) and the grammar's result per case
// (oracle/data/expected.json). Cases where the engine and ScriptDOM both reject but the tree is clean are the ones to read:
// both parsers call them invalid, so each must have a class in oracle/data/rejected-clean.json that explains why the
// grammar may accept them (for example Azure Synapse syntax this grammar models).
//
// Usage: node scripts/scriptdom-compare.mjs
import { readdirSync, readFileSync } from 'node:fs';
import { isAccepted, isMain } from './common.mjs';
import { quotedIdentifier, sessionId } from './oracle-replay.mjs';

// Count cases per (engine, ScriptDOM, tree) cell and list the both-reject-but-clean ones.
export function compare(engine, scriptdom, tree) {
  const cells = {};
  const bothRejectClean = [];
  for (const [id, state] of Object.entries(tree)) {
    if (engine[id] === undefined || scriptdom[id] === undefined) continue;
    const key = `engine ${engine[id] ? 'accepts' : 'rejects'}, ScriptDOM ${scriptdom[id] ? 'accepts' : 'rejects'}, tree ${state}`;
    cells[key] = (cells[key] ?? 0) + 1;
    if (!engine[id] && !scriptdom[id] && state === 'clean') bothRejectClean.push(id);
  }
  return { cells, bothRejectClean };
}

export function observations(files, scriptdom = false) {
  const results = {};
  for (const v of files) {
    const session = quotedIdentifier(
      scriptdom ? { session_set: { quoted_identifier: v.meta.quoted_identifier } } : v.meta,
    );
    for (const [id, verdict] of Object.entries(v.verdicts)) {
      const key = sessionId(id, session);
      results[key] = (results[key] ?? false) || isAccepted(verdict);
    }
  }
  return results;
}

function main() {
  const json = (f) => JSON.parse(readFileSync(f, 'utf8'));
  const engine = observations(
    readdirSync('oracle/data/verdicts')
      .filter((f) => f.endsWith('.json'))
      .map((f) => json(`oracle/data/verdicts/${f}`)),
  );
  const scriptdom = observations(
    readdirSync('oracle/data/scriptdom-verdicts')
      .filter((f) => f.endsWith('.json'))
      .map((f) => json(`oracle/data/scriptdom-verdicts/${f}`)),
    true,
  );
  const classes = json('oracle/data/rejected-clean.json').cases;
  const tree = json('oracle/data/expected.json').cases;
  let unclassified = false;
  for (const session of ['on', 'off']) {
    const sessionTree = Object.fromEntries(
      Object.entries(tree).filter(([id]) => id.endsWith('@qi-off') === (session === 'off')),
    );
    const { cells, bothRejectClean } = compare(engine, scriptdom, sessionTree);
    console.log(`QUOTED_IDENTIFIER ${session.toUpperCase()}`);
    for (const [k, n] of Object.entries(cells).sort())
      console.log(`${String(n).padStart(6)}  ${k}`);
    console.log(`\nEngine and ScriptDOM both reject, tree clean: ${bothRejectClean.length}`);
    for (const id of bothRejectClean)
      console.log(`  ${(classes[id]?.class ?? 'UNCLASSIFIED').padEnd(12)} ${id}`);
    unclassified ||= bothRejectClean.some((id) => !classes[id]);
  }
  if (unclassified) process.exit(1);
}

if (isMain(import.meta.url)) main();
