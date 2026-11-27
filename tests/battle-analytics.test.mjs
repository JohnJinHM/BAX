import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import ts from 'typescript';

async function moduleFrom(file) {
  const source = await readFile(new URL('../src/main/' + file, import.meta.url), 'utf8');
  const { outputText } = ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}});
  return import('data:text/javascript;base64,' + Buffer.from(outputText).toString('base64'));
}
const { treemap } = await moduleFrom('treemap-layout.ts');
const { playerAverages, preferredModels } = await moduleFrom('player-analysis.ts');

test('treemap partitions wide and narrow plots with exact, disjoint tile areas', () => {
  const items = [21, 7, 1, 4, 0, 6].map(value => ({value}));
  for (const [width,height] of [[770,300],[170,320]]) {
    const boxes = treemap(items,width,height);
    assert.equal(boxes.length,5);
    for (const [i, {item,rect:r}] of boxes.entries()) {
      assert.ok(Math.abs(r.width*r.height - width*height*item.value/39) < 1e-7);
      for (const {rect:b} of boxes.slice(i+1)) {
        assert.ok(Math.min(r.x+r.width,b.x+b.width) - Math.max(r.x,b.x) < 1e-8 ||
          Math.min(r.y+r.height,b.y+b.height) - Math.max(r.y,b.y) < 1e-8);
      }
    }
  }
});

test('radars average player-games, count only supplies given to allies and omit ineligible games', () => {
  const model = {id:5,name:'Scout',class:'recon'};
  const a = {team:'alpha', performance:{kills:8,losses:2,objectives:1,suppliedAllies:200},
    composition:[{class:'recon',count:2},{class:'supply',count:2}],opening:[],deployedUnits:[model]};
  const b = {...a,team:'bravo',performance:{kills:4,losses:0,objectives:3,suppliedAllies:0},
    composition:[{class:'infantry',count:8}],opening:[model]};
  const fights = [a,b,{...a,team:'spectators'}, {...a,composition:[],opening:null,deployedUnits:[]}];
  const result = playerAverages(fights);
  assert.equal(result.games,2);
  assert.deepEqual(result.combat,[6,2,75,100]);
  assert.deepEqual(result.preferences,[50,50,0,0,0,0,0,0]);
  assert.equal(preferredModels(fights,true).samples,2);
  assert.equal(preferredModels(fights,true).top[0].games,1);
  assert.equal(preferredModels(fights,false).top[0].games,2);
});
