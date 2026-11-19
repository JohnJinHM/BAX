import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile, access } from 'node:fs/promises';
import ts from 'typescript';

const source = await readFile(new URL('../src/main/sortie-layout.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { clusterEntries, clusterNearby, unitTiles, lifeMinimum, fitMark, packLanes, timeBins, netValue, inRange } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);
const entry = (key, spawnSecs, unitId = 1, refunded = false) => ({ key, spawnSecs, unitId, refunded, cost: 100 });

test('same-second repeated types merge even when interleaved; individual entries survive', () => {
  const groups = clusterEntries([entry(0, 0), entry(1, 0, 2), entry(2, 0)], 0);
  assert.equal(groups.length, 2);
  assert.deepEqual(groups[0].entries.map((x) => x.key), [0, 2]);
  assert.equal(groups.flatMap((x) => x.entries).length, 3);
});
test('batch duration is anchored to first deployment, not extended indefinitely', () => {
  const groups = clusterEntries([entry(0, 0), entry(1, 5), entry(2, 10)], 5);
  assert.deepEqual(groups.map((x) => x.entries.length), [2, 1]);
});
test('nearby deployment windows follow zoom and do not merge a whole chain', () => {
  const entries = [entry(0, 0), entry(1, 50, 2), entry(2, 100), entry(3, 300)];
  assert.deepEqual(clusterNearby(entries, 700, 700).map((g) => g.entries.length), [2, 1, 1]);
  assert.equal(clusterNearby(entries, 100, 700).length, 4);
  assert.deepEqual(clusterNearby(entries, 700, 700, 15).map((g) => g.entries.length), [1, 1, 1, 1]);
});
test('batch tiles retain each model and separate refunded instances of the same model', () => {
  const tiles = unitTiles([entry(0, 0), entry(1, 0, 2), entry(2, 1), entry(3, 2, 1, true)]);
  assert.deepEqual(tiles.map((t) => [t.unit.unitId, t.unit.refunded, t.count]), [[1, false, 2], [2, false, 1], [1, true, 1]]);
  assert.equal(tiles.reduce((n, tile) => n + tile.count, 0), 4);
});
test('lifetime bars share padding while reserving extra room only for RTB', () => {
  assert.equal(lifeMinimum(44, false), 54);
  assert.equal(lifeMinimum(44, true), 66);
  assert.equal(lifeMinimum(48, false) - 48, 10);
});
test('collisions occupy different lanes and freed lanes are reused', () => {
  const result = packLanes([{ x: 0, width: 30 }, { x: 0, width: 40 }, { x: 34, width: 20 }]);
  assert.deepEqual(result.map((x) => x.lane), [0, 1, 0]);
  for (let i = 0; i < result.length; i++) for (let j = i + 1; j < result.length; j++) {
    if (result[i].lane === result[j].lane) assert.ok(result[i].x + result[i].width + 4 <= result[j].x);
  }
});
test('short marks retain minimum width at both ends and reserve lanes for the whole logo', () => {
  assert.deepEqual(fitMark(100, 2, 700, 76), { x: 100, width: 76 });
  assert.deepEqual(fitMark(699, 2, 700, 76), { x: 624, width: 76 });
  assert.deepEqual(fitMark(-10, 2, 700, 76), { x: 0, width: 76 });
  const marks = packLanes([fitMark(699, 1, 700, 76), fitMark(680, 2, 700, 76)]);
  assert.notEqual(marks[0].lane, marks[1].lane);
  for (const mark of marks) assert.ok(mark.width >= 76 && mark.x + mark.width <= 700);
});
test('bins count exact boundaries once, include range end, and omit out-of-range deployments', () => {
  const bins = timeBins([-1, 0, 59, 60, 120, 121].map((time, i) => entry(i, time)), 0, 120, 60);
  assert.deepEqual(bins.map((b) => b.entries.length), [2, 2]);
  assert.equal(inRange(entry(0, 0), 0, 0), true);
  assert.equal(inRange(entry(0, 1), 0, 0), false);
});
test('refunds keep their event but do not inflate net estimated value', () => {
  assert.equal(netValue([entry(0, 0), entry(1, 0, 1, true)]), 100);
  assert.equal(netValue([]), 0);
});
test('all mapped unit thumbnails are packaged locally', async () => {
  await access(new URL('../public/ui-icons/back-to-base.png', import.meta.url));
  const manifest = JSON.parse(await readFile(new URL('../src/main/unit-icons.json', import.meta.url), 'utf8'));
  await Promise.all([...new Set(Object.values(manifest).map((x) => x.file))].map((file) => access(new URL(`../public/unit-icons/${file}`, import.meta.url))));
});
test('every native type icon is packaged and shared vehicle types have distinct labels', async () => {
  const source = await readFile(new URL('../src/main/unit-types.ts', import.meta.url), 'utf8');
  const output = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext } }).outputText;
  const { unitTypes } = await import(`data:text/javascript;base64,${Buffer.from(output).toString('base64')}`);
  await Promise.all(Object.values(unitTypes).filter((type) => type.file).map((type) => access(new URL(`../public/ui-icons/types/${type.file}`, import.meta.url))));
  assert.equal(unitTypes.ifv.file, unitTypes.transport.file);
  assert.notEqual(unitTypes.ifv.tag, unitTypes.transport.tag);
  assert.notEqual(unitTypes.ifv.color, unitTypes.transport.color);
});

test('different options on one base unit stay separate in batches', () => {
  const a = { ...entry(0, 0), variantKey: '1:[10]' };
  const b = { ...entry(1, 0), variantKey: '1:[20]' };
  const c = { ...entry(2, 0), variantKey: '1:[10]' };
  assert.deepEqual(unitTiles([a, b, c]).map(t => t.count), [2, 1]);
  assert.deepEqual(clusterEntries([a, b, c], 0).map(t => t.entries.length), [2, 1]);
});

test('all variant thumbnails are packaged locally', async () => {
  const data = JSON.parse(await readFile(new URL('../crates/bax-engine/src/unit-variants.json', import.meta.url), 'utf8'));
  for (const thumbnail of new Set([...data.units, ...data.options].map(x => x.thumbnail).filter(Boolean))) {
    await access(new URL(`../public/unit-icons/${thumbnail}`, import.meta.url));
  }
});
