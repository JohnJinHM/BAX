import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import ts from 'typescript';

async function moduleFrom(file) {
  const source = await readFile(new URL('../src/main/' + file, import.meta.url), 'utf8');
  const { outputText } = ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}});
  return import('data:text/javascript;base64,' + Buffer.from(outputText).toString('base64'));
}
const { treemap, teamColumns } = await moduleFrom('treemap-layout.ts');
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

test('team columns keep the perspective team left even on a narrow plot or with a smaller score', () => {
  const sides = [{id:'bravo',value:12},{id:'alpha',value:36}];
  for (const width of [180,900]) {
    const [own,opponent] = teamColumns(sides,width,300);
    assert.equal(own.item.id,'bravo');
    assert.deepEqual(own.rect,{x:0,y:0,width:width/4,height:300});
    assert.deepEqual(opponent.rect,{x:width/4,y:0,width:3*width/4,height:300});
  }
  assert.deepEqual(teamColumns([{value:0}],200,100),[]);
});

test('radars average each player-game rate and omit ineligible games', () => {
  const model = {id:5,name:'Scout',class:'recon'};
  const a = {mode:'ranked',team:'alpha', durationSecs:600, performance:{kills:8,losses:2,objectives:1,spawnedScore:2000},
    composition:[{class:'recon',count:2},{class:'supply',count:2}],opening:[],deployedUnits:[model]};
  const b = {...a,team:'bravo',durationSecs:1800,performance:{kills:4,losses:0,objectives:3,spawnedScore:10200},
    composition:[{class:'infantry',count:8}],opening:[model]};
  const fights = [...Array(20).fill({...a,mode:'custom'}), {...b,mode:'pve'}, {...a,mode:'unknown'}, a,b,{...a,team:'spectators'}, {...a,composition:[],opening:null,deployedUnits:[]}];
  const result = playerAverages(fights);
  assert.equal(result.games,2);
  assert.deepEqual(result.combat,[6,2,75,270]);
  assert.equal(result.efficiencyGames,2);
  const missing = {...a,performance:{...a.performance,spawnedScore:null}};
  const partial = playerAverages([a,missing,{...b,durationSecs:0}]);
  assert.equal(partial.games,3);
  assert.equal(partial.efficiencyGames,1);
  assert.equal(partial.combat[3],200);
  assert.equal(playerAverages([missing]).combat[3],null);
  assert.equal(playerAverages([{...a,performance:{...a.performance,spawnedScore:0}}]).combat[3],0);
  assert.deepEqual(result.preferences,[50,50,0,0,0,0,0,0]);
  assert.equal(preferredModels(fights,true).samples,2);
  assert.equal(preferredModels(fights,true).top[0].games,1);
  assert.equal(preferredModels(fights,false).top[0].games,2);
});

test('model details merge variants without mutating source records or losing timing', async () => {
  const { deploymentModels } = await moduleFrom('deployment-models.ts');
  const first = {unitId:5,unitName:'Scout',variantKey:'one',count:2,kills:1,dead:1,refunded:0,
    deployedScore:80,value:80,damage:36.5,transferred:1,firstSpawnSecs:null,lastSpawnSecs:null,
    metrics:{supply:17,buildings:1,airdropped:0}};
  const other = {...first, variantKey:'two',count:1,refunded:1,firstSpawnSecs:0,lastSpawnSecs:73};
  const rows = deploymentModels([first,other,{...other,unitName:'Scout B'}]);
  assert.equal(rows.size,2);
  const merged = [...rows.values()][0];
  assert.deepEqual([merged.count,merged.kills,merged.refunded,merged.deployedScore,merged.damage,merged.metrics.supply], [3,2,1,160,73,34]);
  assert.equal(merged.firstSpawnSecs,0);
  assert.equal(merged.lastSpawnSecs,73);
  assert.equal(first.metrics.supply,17);
});

test('effectiveness uses calibrated ranges while preferences keep independent reference shares', async () => {
  const {calibratedAxes,combatAxes,preferenceAxes,axisRatio,referenceValue} = await moduleFrom('player-analysis.ts');
  assert.deepEqual(combatAxes.map(a=>[a.low,a.base]),[[10,50],[1,3],[30,70],[200,340]]);
  assert.deepEqual(preferenceAxes.map(a=>a.base),[20,28,6,10,6,8,10,12]);
  assert.deepEqual(combatAxes.map(referenceValue),[30,2,50,270]);
  assert.deepEqual(preferenceAxes.map(referenceValue),preferenceAxes.map(a=>a.base));
  for(const axis of combatAxes) {
    assert.equal(axisRatio(axis,axis.low-1),0);
    assert.equal(axisRatio(axis,axis.low),0);
    assert.equal(axisRatio(axis,referenceValue(axis)),.5);
    assert.equal(axisRatio(axis,axis.base),1);
    assert.equal(axisRatio(axis,axis.base+(axis.base-axis.low)/2),1.5);
  }
  const saved = {kills:48.25,'kills.low':5,'kills.high':45,'objectives.low':6,'survival.high':101,
    'efficiency.high':Infinity,recon:15.5,jet:0};
  const calibrated = calibratedAxes(combatAxes,saved);
  assert.deepEqual(calibrated.map(a=>[a.low,a.base]),[[5,45],[1,3],[30,70],[200,340]]);
  assert.equal(referenceValue(calibrated[0]),25);
  assert.equal(referenceValue(calibratedAxes(preferenceAxes,saved)[0]),15.5);
  assert.equal(calibratedAxes(preferenceAxes,saved).at(-1).base,12);
  assert.equal(axisRatio(preferenceAxes[0],30),1.5);
});
