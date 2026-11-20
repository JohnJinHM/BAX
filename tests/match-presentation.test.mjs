import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile(new URL('../src/shared/match-presentation.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext } }).outputText;
const { matchSpecs, showOverlaySpecs, rosterSummary } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);

test('overlay shows last specs only during selection, then this match after completion', () => {
  for (const phase of ['waitingRoom', 'loading', 'deployment', 'inBattle', 'ended']) {
    assert.equal(showOverlaySpecs({ phase, mode: 'custom' }), ['waitingRoom', 'ended'].includes(phase));
  }
  assert.equal(showOverlaySpecs({ phase: 'waitingRoom', mode: 'pve' }), false);
  assert.equal(showOverlaySpecs({ phase: 'ended', mode: 'pve' }), true);
  const player = { id: 101, team: 'alpha', profile: { lastSpecs: ['previous'] } };
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
