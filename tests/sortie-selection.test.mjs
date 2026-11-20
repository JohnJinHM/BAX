import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile(new URL('../src/main/sortie-selection.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { selectionBox, keysInBox, selectKeys } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);

test('box selection works in either direction and includes partially intersecting marks across rows', () => {
  const a = { x: 25, y: 100 }, b = { x: 5, y: 10 };
  const box = selectionBox(a, b);
  assert.deepEqual(box, selectionBox(b, a));
  const marks = [
    { key: 0, x: 0, y: 5, width: 8, height: 30 },
    { key: 1, x: 10, y: 80, width: 100, height: 30 },
    { key: 2, x: 25, y: 40, width: 10, height: 30 },
    { key: 3, x: 10, y: 110, width: 10, height: 30 },
  ];
  assert.deepEqual(keysInBox(box, marks), [0, 1]);
});

test('plain selection replaces, Shift click toggles a whole batch, and Shift box adds without duplicates', () => {
  assert.deepEqual(selectKeys([0, 1], [2, 2], false), [2]);
  assert.deepEqual(selectKeys([0, 1], [1, 2], true, true), [0, 1, 2]);
  assert.deepEqual(selectKeys([0, 1, 2], [1, 2], true, true), [0]);
  assert.deepEqual(selectKeys([0, 1], [1, 2], true), [0, 1, 2]);
  assert.deepEqual(selectKeys([0], [], true), [0]);
  assert.deepEqual(selectKeys([0], [], false), []);
});
