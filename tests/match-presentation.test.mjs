import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile(new URL('../src/shared/match-presentation.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext } }).outputText;
const { logStatus, matchSpecs, showOverlaySpecs, rosterSummary, rosterWaitingText } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);

test('log discovery distinguishes retrying, reading an empty folder and waiting for a match', () => {
  const log = { searching: false, dir: null, ready: false, error: null, file: null };
  assert.equal(logStatus({ ...log, searching: true }).needsDirectory, false);
  assert.equal(logStatus(log).needsDirectory, true);
  assert.match(logStatus({ ...log, dir: 'GameLogs', ready: true }).text, /等待游戏写入/);
  assert.match(logStatus({ ...log, dir: 'GameLogs', ready: true, file: 'game.log' }).text, /已就绪/);
  assert.match(logStatus({ ...log, ready: true, error: '权限不足' }).text, /权限不足/);
});

test('an empty OCR roster retains the reason and the log fallback', () => {
  const m = { mode: 'custom', pregame: { players: [], status: 'OCR 模型未就绪' } };
  assert.match(rosterWaitingText(m), /模型未就绪.*游戏日志/);
  assert.equal(rosterWaitingText({ ...m, pregame: { players: [], status: '' } }), '等待游戏日志提供正式名单。');
  assert.match(rosterWaitingText({ ...m, mode: 'pve' }), /PvE.*暂停/);
});

test('overlay shows same-country predictions only during selection, then this match after completion', () => {
  for (const phase of ['waitingRoom', 'loading', 'deployment', 'inBattle', 'ended']) {
    assert.equal(showOverlaySpecs({ phase, mode: 'custom' }), ['waitingRoom', 'ended'].includes(phase));
  }
  assert.equal(showOverlaySpecs({ phase: 'waitingRoom', mode: 'pve' }), false);
  assert.equal(showOverlaySpecs({ phase: 'ended', mode: 'pve' }), true);
  const player = { id: 101, team: 'alpha', profile: { lastSpecs: ['previous'] } };
  const selection = { phase:'waitingRoom' };
  assert.deepEqual(matchSpecs(player, selection, null), []);
  const known = { ...player, country:2, profile:{ lastSpecs:['other'], countrySpecs:{2:['same-country']} } };
  assert.deepEqual(matchSpecs(known, selection, null), ['same-country']);
  assert.deepEqual(matchSpecs({ ...known, country:1 }, selection, null), []);
  const match = { matchId: 1, phase: 'ended' };
  assert.deepEqual(matchSpecs(player, match, null), []);
  assert.deepEqual(matchSpecs(player, match, { analysis: { matchId: 2 }, specs: { 101: ['wrong match'] } }), []);
  assert.deepEqual(matchSpecs(player, match, { analysis: { matchId: 1 }, specs: { 101: ['current'] } }), ['current']);
  assert.deepEqual(matchSpecs({ ...player, team: 'spectators' }, { phase: 'waitingRoom' }, null), []);
});

test('custom summary counts each side separately from spectators', () => {
  const match = { matchId: 1, rosterKnown: true, mode: 'custom', players: [{ team: 'alpha' }, { team: 'bravo' }, { team: 'bravo' }, { team: 'spectators' }] };
  assert.equal(rosterSummary(match), '自定义 · 1 v 2 · 旁观 1 人');
});
