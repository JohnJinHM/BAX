import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile(new URL('../src/shared/overlay-automation.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { OverlayAutomation } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);
const settings = { autoExpand: true, collapseBeforeBattleSecs: 60 };
const match = { matchId: 1, discoveredAt: 0, phase: 'waitingRoom', connection: 'connected', clock: null, endedAt: null };

test('pregame opens once, deployment collapses at sixty seconds, manual expansion survives battle ticks', () => {
  const a = new OverlayAutomation();
  assert.equal(a.update(match, settings, 0), true);
  assert.equal(a.update(match, settings, 1000), null);
  const deploy = { ...match, phase: 'deployment', clock: { startAt: 90000 } };
  assert.equal(a.update(deploy, settings, 10000), null);
  assert.equal(a.update(deploy, settings, 30000), false);
  // 用户此时手动展开；同一自动阶段不再覆盖。
  assert.equal(a.update({ ...deploy, phase: 'inBattle' }, settings, 90000), null);
});

test('end stays expanded beyond ten seconds and collapses once on RoomExited connection change', () => {
  const a = new OverlayAutomation();
  a.update(match, settings, 0);
  const ended = { ...match, phase: 'ended', endedAt: 1000 };
  assert.equal(a.update(ended, settings, 1000), true);
  for (const now of [11000, 60000, 600000]) assert.equal(a.update(ended, settings, now), null);
  const exited = { ...ended, connection: 'disconnected' };
  assert.equal(a.update(exited, settings, 600001), false);
  assert.equal(a.update(exited, settings, 600002), null);
  // 迟到战报／资料快照，或者退出后手动展开，都不重新触发自动操作。
  assert.equal(a.update(structuredClone(exited), settings, 700000), null);
});

test('exit takes priority when match end and RoomExited arrive in the same snapshot', () => {
  const a = new OverlayAutomation();
  a.update({ ...match, phase: 'inBattle' }, settings, 0);
  assert.equal(a.update({ ...match, phase: 'ended', endedAt: 1000, connection: 'disconnected' }, settings, 1000), false);
});

test('disabled automation, old results on startup, new match and manual result interaction', () => {
  const a = new OverlayAutomation();
  assert.equal(a.update(match, { ...settings, autoExpand: false }, 0), null);
  const ended = { ...match, phase: 'ended', endedAt: 1000 };
  assert.equal(a.update(ended, settings, 200000), false);
  assert.equal(a.update({ ...match, matchId: 2 }, settings, 200001), true);
  assert.equal(a.update({ ...ended, matchId: 2, endedAt: 201000 }, settings, 201000), true);
  // 用户手动收起后，资料到达不会重新展开；退出仍正常收起。
  assert.equal(a.update({ ...ended, matchId: 2 }, settings, 230000), null);
  assert.equal(a.update({ ...ended, matchId: 2, connection: 'disconnected' }, settings, 230001), false);
  assert.equal(a.update({ ...match, matchId: 3 }, settings, 230002), true);
});

test('starting after a recent exit stays collapsed; leaving pregame also collapses', () => {
  const a = new OverlayAutomation();
  assert.equal(a.update({ ...match, phase: 'ended', endedAt: 1000, connection: 'disconnected' }, settings, 1001), false);
  assert.equal(a.update({ ...match, matchId: 2 }, settings, 1002), true);
  assert.equal(a.update({ ...match, matchId: 2, connection: 'disconnected' }, settings, 1003), false);
});

test('custom zero-second threshold works and legacy result duration no longer closes results', () => {
  const a = new OverlayAutomation();
  const custom = { ...settings, collapseBeforeBattleSecs: 0, resultDisplaySecs: 3 };
  const deploy = { ...match, phase: 'deployment', clock: { startAt: 90000 } };
  assert.equal(a.update(deploy, custom, 1000), true);
  assert.equal(a.update(deploy, custom, 89999), null);
  assert.equal(a.update(deploy, custom, 90000), false);
  const ended = { ...match, phase: 'ended', endedAt: 95000 };
  assert.equal(a.update(ended, custom, 95000), true);
  assert.equal(a.update(ended, custom, 98000), null);
});
