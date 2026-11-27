import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile(new URL('../src/shared/roster.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { authorPlayerId, currentRelations, playerLabels, rosterGroups } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);
const candidate = (friendly, side, id = 999123) => ({ observed: { name: 'Raven', level: 79, friendly, side, row: 0 }, ambiguous: false,
  candidates: [{ id, name: 'Raven', profile: { rating: 2200, level: 70 } }] });
const match = { rosterKnown: false, selfTeam: null, players: [], pregame: { players: [candidate(false, 'left'), candidate(true, 'right')], status: '' } };
test('only confirmed active teammates and opponents receive a relation', () => {
  const active = { rosterKnown: true, selfTeam: 'bravo', phase: 'inBattle', players: [
    { id: 1, team: 'bravo', isSelf: true }, { id: 2, team: 'bravo' },
    { id: 3, team: 'alpha' }, { id: 4, team: 'spectators' },
  ] };
  assert.deepEqual([...currentRelations(active)], [[2, '队友'], [3, '对手']]);
  for (const change of [{ phase: 'ended' }, { rosterKnown: false }, { selfTeam: 'spectators' }, { selfTeam: null }]) {
    assert.equal(currentRelations({ ...active, ...change }).size, 0);
  }
});
test('pregame uses normal groups with friendly order independent of screen side', () => {
  const groups = rosterGroups(match);
  assert.deepEqual(groups.map(g => [g.title, g.tone]), [['我方','mine'], ['敌方','enemy']]);
  assert.equal(groups[0].players[0].displayLevel,79);
  assert.equal(groups[0].players[0].id,null);
  assert.equal(groups[0].averageValue,2200);
  assert.equal(JSON.stringify(groups).includes('999123'),false);
});
test('confirmed IDs replace OCR hypotheses and missing names never render as IDs', () => {
  const m = { ...match, rosterKnown: true, selfTeam: 'alpha', players: [{ id: 345678, team: 'alpha', name: null, logName: null, profile: null }] };
  let row = rosterGroups(m)[0].players[0];
  assert.equal(row.name,'资料读取中'); assert.equal(row.provisional,false);
  m.players[0].name='Confirmed Raven';
  row = rosterGroups(m)[0].players[0]; assert.equal(row.name,'Confirmed Raven');
  assert.equal(rosterGroups(m).flatMap(g => g.players).length,1);
});
test('ambiguous candidates do not acquire authoritative profile statistics', () => {
  const p = { ...candidate(false,'right'), ambiguous: true };
  assert.equal(rosterGroups({ ...match, pregame: { players: [p] } })[0].players[0].profile,null);
});

test('OCR candidate specialization analysis is reused without exposing provisional IDs', () => {
  const p = candidate(false, 'left');
  p.candidates[0].profile.lastSpecs = [{ ids: [5], name: '空降部队', games: 1 }];
  const row = rosterGroups({ ...match, pregame: { players: [p] } })[0].players[0];
  assert.deepEqual(row.profile.lastSpecs, p.candidates[0].profile.lastSpecs);
  assert.equal(row.id, null);
});

test('the author badge follows the account across official and unambiguous OCR rosters', () => {
  const other = { symbol: '车', detail: '示例' };
  const author = { symbol: '作', detail: '是作者哟=w=' };
  const labels = { [authorPlayerId]: [other] };
  const players = [
    { id: authorPlayerId, name: 'Renamed', team: 'bravo' },
    { id: 301, name: 'Renamed', isSelf: true, team: 'bravo' },
  ];
  const official = rosterGroups({ ...match, rosterKnown: true, players, labels })[0].players;
  assert.deepEqual(official.map(p => p.labels), [[author, other], []]);
  assert.deepEqual(playerLabels(authorPlayerId, undefined), [author]);
  const p = candidate(false, 'left', authorPlayerId);
  const ocr = () => rosterGroups({ ...match, labels, pregame: { players: [p] } })[0].players[0];
  assert.deepEqual(ocr().labels, [author, other]);
  p.ambiguous = true;
  assert.deepEqual(ocr().labels, []);
});
