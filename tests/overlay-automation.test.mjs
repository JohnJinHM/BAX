import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile(new URL('../src/shared/overlay-automation.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { OverlayAutomation } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);
const settings = { autoExpand: true, collapseBeforeBattleSecs: 60, resultDisplaySecs: 10 };
const match = { matchId: 1, discoveredAt: 0, phase: 'waitingRoom', clock: null, endedAt: null };

test('pregame opens once, deployment collapses at sixty seconds, manual expansion survives battle ticks', () => {
  const a = new OverlayAutomation();
  assert.equal(a.update(match, settings, 0, false), true);
  assert.equal(a.update(match, settings, 1000, false), null);
  const deploy = { ...match, phase: 'deployment', clock: { startAt: 90000 } };
  assert.equal(a.update(deploy, settings, 10000, false), null);
  assert.equal(a.update(deploy, settings, 30000, false), false);
  a.manual();
  assert.equal(a.update({ ...deploy, phase: 'inBattle' }, settings, 90000, false), null);
});
test('end opens, result arrival gets a full ten seconds, ticks never reopen it', () => {
  const a = new OverlayAutomation();
  a.update(match, settings, 0, false);
  const ended = { ...match, phase: 'ended', endedAt: 1000 };
  assert.equal(a.update(ended, settings, 1000, false), true);
  assert.equal(a.update(ended, settings, 11000, false), false);
  assert.equal(a.update(ended, settings, 60000, true), true);
  assert.equal(a.update(ended, settings, 69999, true), null);
  assert.equal(a.update(ended, settings, 70000, true), false);
  assert.equal(a.update(ended, settings, 71000, true), null);
});
test('disabled automation, old results on startup, new match and manual result interaction', () => {
  const a = new OverlayAutomation();
  assert.equal(a.update(match, { ...settings, autoExpand: false }, 0, false), null);
  const ended = { ...match, phase: 'ended', endedAt: 1000 };
  assert.equal(a.update(ended, settings, 200000, true), false);
  assert.equal(a.update({ ...match, matchId: 2 }, settings, 200001, false), true);
  assert.equal(a.update({ ...ended, matchId: 2, endedAt: 201000 }, settings, 201000, true), true);
  a.manual();
  assert.equal(a.update({ ...ended, matchId: 2 }, settings, 230000, true), null);
});
test('custom zero-second threshold and shorter result duration are honored', () => {
  const a = new OverlayAutomation();
  const custom = { ...settings, collapseBeforeBattleSecs: 0, resultDisplaySecs: 3 };
  const deploy = { ...match, phase: 'deployment', clock: { startAt: 90000 } };
  assert.equal(a.update(deploy, custom, 1000, false), true);
  assert.equal(a.update(deploy, custom, 89999, false), null);
  assert.equal(a.update(deploy, custom, 90000, false), false);
  const ended = { ...match, phase: 'ended', endedAt: 95000 };
  assert.equal(a.update(ended, custom, 95000, true), true);
  assert.equal(a.update(ended, custom, 98000, true), false);
});
