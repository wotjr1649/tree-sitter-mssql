import assert from 'node:assert/strict';
import { test } from 'node:test';
import { compareNodeTypes } from './check-node-types.mjs';

const identifier = { type: 'identifier', named: true };
const namedThis = { type: 'this', named: true };
const anonymousThis = { type: 'this', named: false };
const base = [
  { type: 'expression', named: true, subtypes: [identifier, namedThis] },
  {
    type: 'call',
    named: true,
    fields: {
      callee: { multiple: false, required: true, types: [identifier] },
      children: { multiple: true, required: false, types: [namedThis, anonymousThis] },
    },
    children: { multiple: true, required: false, types: [namedThis, anonymousThis] },
  },
  identifier,
  namedThis,
  anonymousThis,
];

test('identical node types are compatible', () => {
  assert.deepEqual(compareNodeTypes(base, structuredClone(base)), []);
});

test('detects removed named and anonymous kinds, a field, field type, child type and subtype', () => {
  const head = structuredClone(base);
  head[0].subtypes.pop();
  delete head[1].fields.callee;
  head[1].fields.children.types.pop();
  head[1].children.types.pop();
  head.splice(4, 1);
  head.splice(2, 1);

  assert.deepEqual(compareNodeTypes(base, head), [
    'removed this from subtypes of expression',
    'removed field call.callee',
    'removed "this" from field call.children',
    'removed "this" from children of call',
    'removed kind identifier',
    'removed kind "this"',
  ]);
});

for (const [label, select] of [
  ['field call.children', (node) => node.fields.children],
  ['children of call', (node) => node.children],
]) {
  test(`${label} cannot lose multiplicity or become required`, () => {
    const head = structuredClone(base);
    const entry = select(head[1]);
    entry.multiple = false;
    entry.required = true;

    assert.deepEqual(compareNodeTypes(base, head), [
      `${label} no longer allows multiple nodes`,
      `${label} is now required`,
    ]);
    assert.deepEqual(compareNodeTypes(head, base), []);
  });
}

test('detects the removal of unfielded children', () => {
  const head = structuredClone(base);
  delete head[1].children;
  assert.deepEqual(compareNodeTypes(base, head), [
    'removed this from children of call',
    'removed "this" from children of call',
    'children of call no longer allows multiple nodes',
  ]);
});

test('allows added kinds, fields, field types, child types and subtypes', () => {
  const head = structuredClone(base);
  const literal = { type: 'literal', named: true };
  head.push(literal);
  head[0].subtypes.push(literal);
  head[1].fields.children.types.push(literal);
  head[1].fields.argument = { multiple: false, required: true, types: [literal] };
  head[1].children.types.push(literal);

  assert.deepEqual(compareNodeTypes(base, head), []);
});

test('allows new required children alongside a field named children', () => {
  const before = structuredClone(base);
  delete before[1].children;
  const head = structuredClone(before);
  head[1].children = { multiple: false, required: true, types: [identifier] };

  assert.deepEqual(compareNodeTypes(before, head), []);
});
