import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile(new URL('../src/shared/roster.ts', import.meta.url), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { rosterGroups } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);
const candidate = (friendly, side, id = 999123) => ({ observed: { name: 'Raven', level: 79, friendly, side, row: 0 }, ambiguous: false,
  candidates: [{ id, name: 'Raven', profile: { rating: 2200, level: 70 } }] });
const match = { rosterKnown: false, selfTeam: null, players: [], pregame: { players: [candidate(false, 'left'), candidate(true, 'right')], status: '' } };
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
