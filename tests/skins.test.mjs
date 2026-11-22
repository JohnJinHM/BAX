import test from 'node:test';
import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import ts from 'typescript';

const shared = new URL('../src/shared/', import.meta.url);
const read = (path) => readFile(new URL(path, shared), 'utf8');
const compiled = ts.transpileModule(await read('skins.ts'), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { skins, normalizeSkin, defaultSkin } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);

/** 去掉注释后拆成规则块。皮肤与主题文件里只有一层规则，不需要完整的 CSS 解析器。 */
function blocks(css) {
  const clean = css.replace(/\/\*[\s\S]*?\*\//g, '');
  return [...clean.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
    selector: m[1].trim().replace(/\s+/g, ' '),
    declarations: m[2].split(';').map((d) => d.trim()).filter(Boolean).map((d) => {
      const i = d.indexOf(':');
      return [d.slice(0, i).trim(), d.slice(i + 1).trim()];
    }),
  }));
}

const themeTokens = new Map(
  blocks(await read('theme.css'))
    .filter((b) => b.selector.split(',').some((s) => s.trim() === ':root'))
    .flatMap((b) => b.declarations.filter(([name]) => name.startsWith('--bax-'))),
);
const skinFiles = (await readdir(new URL('skins/', shared))).filter((f) => f.endsWith('.css') && f !== 'index.css');
const skinTokens = new Map(
  await Promise.all(skinFiles.map(async (file) => [file.replace(/\.css$/, ''), blocks(await read(`skins/${file}`))])),
);

test('every built-in skin except the default has exactly one CSS file, imported by index.css', async () => {
  const ids = skins.map((s) => s.id).filter((id) => id !== defaultSkin);
  assert.deepEqual([...skinTokens.keys()].sort(), [...ids].sort());
  const index = await read('skins/index.css');
  for (const id of ids) assert.match(index, new RegExp(`@import "\\./${id}\\.css";`));
  assert.equal(new Set(skins.map((s) => s.id)).size, skins.length);
});

test('skin files only override variables that theme.css declares, inside their own [data-skin] block', () => {
  for (const [id, list] of skinTokens) {
    assert.ok(list.length > 0, id);
    for (const block of list) {
      assert.equal(block.selector, `[data-skin="${id}"]`);
      for (const [name] of block.declarations) {
        assert.ok(name.startsWith('--bax-'), `${id}: ${name} 不是皮肤变量`);
        assert.ok(themeTokens.has(name), `${id}: ${name} 在 theme.css 里没有默认值`);
      }
    }
  }
});

test('unknown or missing skin ids fall back to the default skin', () => {
  for (const s of skins) assert.equal(normalizeSkin(s.id), s.id);
  for (const value of [undefined, null, '', 'missing', 3, { id: 'broken-arrow' }]) assert.equal(normalizeSkin(value), defaultSkin);
});

// ---------------------------------------------------------------- 对比度（WCAG AA，正文 4.5:1）

function tokensOf(id) {
  const own = id === defaultSkin ? [] : skinTokens.get(id).flatMap((b) => b.declarations);
  return new Map([...themeTokens, ...own]);
}

function resolve(tokens, name) {
  const value = tokens.get(name);
  const ref = /^var\((--bax-[\w-]+)\)$/.exec(value ?? '');
  return ref ? resolve(tokens, ref[1]) : value;
}

function parseColor(value) {
  let m = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(value);
  if (m) {
    const hex = m[1].length === 3 ? [...m[1]].map((c) => c + c).join('') : m[1];
    const [r, g, b, a = 255] = hex.match(/../g).map((h) => parseInt(h, 16));
    return [r, g, b, a / 255];
  }
  m = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:\s*[,/]\s*([\d.]+))?\s*\)$/.exec(value);
  if (m) return [Number(m[1]), Number(m[2]), Number(m[3]), m[4] == null ? 1 : Number(m[4])];
  m = /^([\d.]+)\s+([\d.]+)\s+([\d.]+)$/.exec(value);
  if (m) return [Number(m[1]), Number(m[2]), Number(m[3]), 1];
  throw new Error(`无法解析颜色：${value}`);
}

const over = (top, bottom) => [0, 1, 2].map((i) => top[i] * top[3] + bottom[i] * (1 - top[3])).concat(1);

function luminance([r, g, b]) {
  const channel = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function contrast(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

// 作为文字出现的颜色。--bax-faint 只用于次要标注和禁用状态，不在此列。
const textTokens = ['--bax-fg', '--bax-soft', '--bax-muted', '--bax-accent', '--bax-value', '--bax-good', '--bax-warn', '--bax-bad', '--bax-friend', '--bax-foe', '--bax-alpha', '--bax-bravo'];

test('text colours of every skin reach 4.5:1 on the background and both surfaces', () => {
  for (const { id } of skins) {
    const tokens = tokensOf(id);
    const color = (name) => parseColor(resolve(tokens, name));
    const bg = color('--bax-bg-rgb');
    const surface = over(color('--bax-surface'), bg);
    const surface2 = over(color('--bax-surface-2'), surface);
    for (const name of textTokens) {
      for (const [label, ground] of [['bg', bg], ['surface', surface], ['surface-2', surface2]]) {
        const ratio = contrast(over(color(name), ground), ground);
        assert.ok(ratio >= 4.5, `${id}: ${name} 在 ${label} 上只有 ${ratio.toFixed(2)}:1`);
      }
    }
    const accent = color('--bax-accent');
    const onAccent = contrast(color('--bax-on-accent'), accent);
    assert.ok(onAccent >= 4.5, `${id}: 主按钮文字只有 ${onAccent.toFixed(2)}:1`);
  }
});
