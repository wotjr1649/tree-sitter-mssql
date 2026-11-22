// Usage: node scripts/check-node-types.mjs BASE_SHA [HEAD_NODE_TYPES]
// Compare against the committed JSON at BASE_SHA. Exit 1 for incompatibility or 2 for input errors.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { isMain } from './common.mjs';

const nodeTypesPath = 'src/node-types.json';
const typeKey = ({ type, named }) => JSON.stringify([type, named]);
const typeName = ({ type, named }) => (named ? type : JSON.stringify(type));

function checkTypes(before, after, label, failures) {
  const current = new Set(after.map(typeKey));
  for (const type of before) {
    if (!current.has(typeKey(type))) failures.push(`removed ${typeName(type)} from ${label}`);
  }
}

function checkEntry(before, after, label, failures) {
  checkTypes(before.types, after?.types ?? [], label, failures);
  if (before.multiple && !after?.multiple)
    failures.push(`${label} no longer allows multiple nodes`);
  if (!before.required && after?.required) failures.push(`${label} is now required`);
}

export function compareNodeTypes(before, after) {
  const current = new Map(after.map((node) => [typeKey(node), node]));
  const failures = [];
  for (const node of before) {
    const name = typeName(node);
    const next = current.get(typeKey(node));
    if (!next) {
      failures.push(`removed kind ${name}`);
      continue;
    }

    const fields = next.fields ?? {};
    for (const [field, entry] of Object.entries(node.fields ?? {})) {
      const label = `field ${name}.${field}`;
      if (Object.hasOwn(fields, field)) checkEntry(entry, fields[field], label, failures);
      else failures.push(`removed ${label}`);
    }
    if (node.children) checkEntry(node.children, next.children, `children of ${name}`, failures);
    checkTypes(node.subtypes ?? [], next.subtypes ?? [], `subtypes of ${name}`, failures);
  }
  return failures;
}

function main(baseSha, headPath = nodeTypesPath) {
  if (!/^[0-9a-f]{40}$/.test(baseSha)) throw new Error('expected a full base commit SHA');
  const type = execFileSync('git', ['cat-file', '-t', baseSha], { encoding: 'utf8' }).trim();
  if (type !== 'commit') throw new Error('base SHA does not identify a commit');

  const baseJson = execFileSync('git', ['show', `${baseSha}:${nodeTypesPath}`], {
    encoding: 'utf8',
    maxBuffer: 1 << 28,
  });
  const failures = compareNodeTypes(
    JSON.parse(baseJson),
    JSON.parse(readFileSync(headPath, 'utf8')),
  );
  for (const failure of failures) console.error(failure);
  console.log(`Node types compared against ${baseSha} with ${failures.length} violation(s)`);
  return failures.length ? 1 : 0;
}

if (isMain(import.meta.url)) {
  try {
    process.exitCode = main(...process.argv.slice(2));
  } catch (error) {
    console.error(`check-node-types: ${error.message}`);
    process.exitCode = 2;
  }
}
