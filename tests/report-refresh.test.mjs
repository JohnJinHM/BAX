import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import vm from 'node:vm';
import ts from 'typescript';
import { compileScript, parse } from '@vue/compiler-sfc';
import { createRenderer, h, nextTick, ref, shallowRef } from 'vue';

const require = createRequire(import.meta.url);
const vue = require('vue');
const format = {};
vm.runInNewContext(ts.transpileModule(readFileSync(new URL('../src/shared/format.ts', import.meta.url), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText, { exports: format });
const renderer = createRenderer({
  createComment: () => ({}), insert() {}, remove() {}, parentNode() {}, nextSibling() {},
});
const flush = async () => { await nextTick(); await Promise.resolve(); await nextTick(); };
function component(file, engine, extra = {}) {
  const source = readFileSync(new URL(`../src/main/${file}.vue`, import.meta.url), 'utf8');
  const script = compileScript(parse(source).descriptor, { id: file }).content;
  const compiled = ts.transpileModule(script, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  const exports = {};
  vm.runInNewContext(compiled, { exports, require: id => id === 'vue' ? vue : id.endsWith('/engine') ? engine : id.endsWith('/format') ? format : (extra[id] ?? {}),
    getComputedStyle: () => ({ getPropertyValue: () => '#ffffff' }), document: { documentElement: {} },
    window: { setTimeout, clearTimeout, matchMedia: () => ({ matches: false }) },
  });
  return { ...exports.default, render: () => null };
}
const snapshotData = () => ({ archive: { seq: 1, pending: 0 }, current: { matchId: 456, phase: 'inGame' }, ui: {} });

test('saving a note preserves edits made in flight and ignores completion on another player', async () => {
  const player = ref(101);
  const pending = [];
  const saved = new Map();
  const panel = component('PlayerDetailView', {
    useSnapshot: () => shallowRef(snapshotData()),
    commands: {
      setNote: (id, body) => new Promise(resolve => pending.push(() => { saved.set(id, body); resolve(); })),
      playerDetail: async id => ({ note: { body: saved.get(id) ?? '' } }),
    },
  }, { '../shared/useNow': { useNow: () => ref(0) } });
  const app = renderer.createApp({ render: () => h(panel, { id: player.value }) });
  app.mount({});
  try {
    await flush();
    const state = app._instance.subTree.component.setupState;
    state.noteDraft = 'first';
    const first = state.saveNote();
    state.noteDraft = 'second';
    pending.shift()(); await first;
    assert.equal(state.noteDraft, 'second');
    assert.equal(state.noteDirty, true);
    assert.equal(state.noteMessage, null);
    const second = state.saveNote();
    player.value = 102; await flush();
    state.noteDraft = 'another player';
    pending.shift()(); await second;
    assert.equal(saved.get(101), 'second');
    assert.equal(state.noteDraft, 'another player');
    assert.equal(state.noteDirty, true);
    assert.equal(state.noteMessage, null);
    player.value = 101; await flush();
    assert.equal(state.noteDraft, 'second');
    assert.equal(state.noteDirty, false);
    player.value = 102; await flush();
    assert.equal(state.noteDraft, 'another player');
    assert.equal(state.noteDirty, true);
  } finally { app.unmount(); }
});

test('saving timeline height or receiving a profile snapshot does not reload match history', async () => {
  const snapshot = shallowRef(snapshotData());
  let loads = 0;
  const history = component('HistoryPanel', { useSnapshot: () => snapshot, commands: {
    matchHistory: async () => { loads++; return [{ matchId: 123, report: { state: 'ready' } }]; },
  } }, { './navigation': { selectedMatch: ref(null), historyPlayer: ref(null) } });
  const app = renderer.createApp(history);
  app.mount({});
  try {
    await flush();
    assert.equal(loads, 1);
    snapshot.value = { ...structuredClone(snapshot.value), ui: { 'ui.sortie': { height: 920 } } };
    await flush();
    snapshot.value = structuredClone(snapshot.value);
    await flush();
    assert.equal(loads, 1);
    snapshot.value = { ...snapshot.value, archive: { seq: 2, pending: 0 } };
    await flush();
    assert.equal(loads, 2);
    snapshot.value = { ...snapshot.value, current: { matchId: 456, phase: 'ended' } };
    await flush();
    assert.equal(loads, 3);
  } finally { app.unmount(); }
});

test('same-match row refresh keeps the report mounted and its selection; a new match resets it', async () => {
  const snapshot = shallowRef(snapshotData());
  const row = ref({ matchId: 123, report: { state: 'ready' } });
  let loads = 0;
  const report = component('ReportView', { useSnapshot: () => snapshot, commands: {
    matchReport: async id => { loads++; return { analysis: { matchId: id } }; },
  } });
  const app = renderer.createApp({ render: () => h(report, { id: row.value.matchId, row: row.value }) });
  app.mount({});
  try {
    await flush();
    const state = app._instance.subTree.component.setupState;
    const initialView = state.view;
    state.selectedRange = [60, 600];
    state.expanded = 999;
    row.value = structuredClone({ matchId: 123, report: { state: 'ready' } });
    await flush();
    assert.equal(loads, 1);
    assert.equal(state.view, initialView);
    assert.deepEqual(state.selectedRange, [60, 600]);
    assert.equal(state.expanded, 999);
    snapshot.value = { ...snapshot.value, archive: { seq: 2 } };
    await flush();
    assert.equal(loads, 2);
    assert.deepEqual(state.selectedRange, [60, 600]);
    row.value = { matchId: 124, report: { state: 'ready' } };
    await flush();
    assert.equal(loads, 3);
    assert.equal(state.selectedRange, null);
    assert.equal(state.expanded, null);
    assert.equal(state.view.analysis.matchId, 124);
  } finally { app.unmount(); }
});

test('history refresh keeps all requested pages when it overtakes an outstanding load', async () => {
  const snapshot = shallowRef(snapshotData());
  const selectedMatch = ref(null);
  const source = Array.from({ length: 650 }, (_, matchId) => ({ matchId, report: { state: 'ready' } }));
  let hold = false;
  let release;
  const history = component('HistoryPanel', { useSnapshot: () => snapshot, commands: {
    matchHistory: async (limit, offset) => {
      assert.ok(limit <= 500);
      const page = source.slice(offset, offset + limit);
      if (hold) { hold = false; await new Promise(resolve => { release = resolve; }); }
      return page;
    },
  } }, { './navigation': { selectedMatch, historyPlayer: ref(null) } });
  const app = renderer.createApp(history);
  app.mount({});
  try {
    await flush();
    const state = app._instance.setupState;
    for (let i = 1; i < 5; ++i) await state.load(true);
    selectedMatch.value = 550;
    hold = true;
    const more = state.load(true);
    source.unshift({ matchId: 999, report: { state: 'ready' } });
    snapshot.value = { ...snapshot.value, archive: { seq: 2 } };
    await flush();
    release(); await more;
    assert.equal(state.rows.length, 600);
    assert.equal(state.rows[0].matchId, 999);
    assert.equal(state.selectedRow.matchId, 550);
    await state.load(true);
    assert.equal(state.rows.length, 651);
    assert.equal(state.exhausted, true);
  } finally { app.unmount(); }
});
