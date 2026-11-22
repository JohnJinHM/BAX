import test from 'node:test';
import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import ts from 'typescript';

const shared = new URL('../src/shared/', import.meta.url);
const compile = async (file) => ts.transpileModule(await readFile(new URL(file, shared), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText;
const dataUrl = (code) => `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`;

// appearance.ts 只引用 skins.ts：把相对路径换成编译后的 data URL 再导入
const skinsUrl = dataUrl(await compile('skins.ts'));
const appearanceCode = (await compile('appearance.ts')).replace(/from\s+["']\.\/skins["']/, `from "${skinsUrl}"`);
const {
  normalizeAppearance, appearanceVars, colorVars, managedVars, colorSlots, densities, defaultAppearance,
  fontScaleRange, onAccent, parseCssColor, contrastRatio, worstContrast, isDefaultAppearance,
} = await import(dataUrl(appearanceCode));
const { skins } = await import(skinsUrl);

test('missing, old or damaged settings normalize to a usable appearance', () => {
  for (const value of [undefined, null, 'default', 3, [], { id: 'missing' }]) {
    assert.deepEqual(normalizeAppearance(value), defaultAppearance);
  }
  // 0.2.2 只存了皮肤 ID
  assert.deepEqual(normalizeAppearance({ id: 'broken-arrow' }), { ...defaultAppearance, id: 'broken-arrow' });

  const messy = normalizeAppearance({
    id: 'default',
    colors: {
      default: { accent: '#ABC', friend: 'red', foe: '#12345', good: '#00ff00', extra: '#ffffff' },
      'broken-arrow': { bad: 'url(x)' },
      unknown: { accent: '#ffffff' },
    },
    fontScale: 9,
    density: 'huge',
  });
  assert.deepEqual(messy.colors, { default: { accent: '#aabbcc', good: '#00ff00' } });
  assert.equal(messy.fontScale, fontScaleRange.max);
  assert.equal(messy.density, 'standard');
});

test('font scale is clamped to the slider range and rounded', () => {
  const scale = (fontScale) => normalizeAppearance({ fontScale }).fontScale;
  assert.equal(scale(0.1), fontScaleRange.min);
  assert.equal(scale(1.234), 1.23);
  assert.equal(scale('1.2'), 1);
  assert.equal(scale(Number.NaN), 1);
  assert.equal(scale(Infinity), 1);
});

test('only the active skin\'s colours are applied, and paired variables move together', () => {
  const appearance = normalizeAppearance({
    id: 'broken-arrow',
    colors: { default: { accent: '#ff00ff' }, 'broken-arrow': { friend: '#3366ff', foe: '#ff9900' } },
    fontScale: 1.1,
    density: 'compact',
  });
  const vars = appearanceVars(appearance);
  assert.equal(vars['--bax-accent'], undefined);
  assert.equal(vars['--bax-friend'], '#3366ff');
  assert.equal(vars['--bax-alpha'], '#3366ff');
  assert.equal(vars['--bax-foe'], '#ff9900');
  assert.equal(vars['--bax-bravo'], '#ff9900');
  assert.equal(vars['--bax-font-scale'], '1.1');
  assert.equal(vars['--bax-density'], String(densities.find((d) => d.id === 'compact').factor));
  for (const name of Object.keys(vars)) assert.ok(managedVars.includes(name), `${name} 不在 managedVars 里，换皮肤时清不掉`);
  assert.equal(isDefaultAppearance(appearance), false);
  assert.equal(isDefaultAppearance({ ...defaultAppearance, id: 'broken-arrow', colors: { default: { accent: '#ff00ff' } } }), true);
});

test('every colour slot maps to variables that theme.css declares', async () => {
  const theme = await readFile(new URL('theme.css', shared), 'utf8');
  for (const name of managedVars) assert.match(theme, new RegExp(`${name}:`), `${name} 在 theme.css 里没有默认值`);
  assert.equal(new Set(colorSlots.map((s) => s.key)).size, colorSlots.length);
  assert.ok(skins.length >= 2);
});

test('button text on a custom accent always reaches 4.5:1', () => {
  for (let r = 0; r <= 255; r += 17) {
    for (let g = 0; g <= 255; g += 17) {
      for (let b = 0; b <= 255; b += 17) {
        const hex = `#${[r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('')}`;
        const vars = colorVars({ accent: hex });
        const ratio = contrastRatio(parseCssColor(hex), parseCssColor(vars['--bax-on-accent']));
        assert.ok(ratio >= 4.5, `${hex} 上的 ${onAccent(hex)} 只有 ${ratio.toFixed(2)}:1`);
      }
    }
  }
});

test('contrast warning composites translucent surfaces over the background', () => {
  const grounds = { bg: '14 17 22', surface: '#161b22', surface2: '#1d232c' };
  assert.ok(worstContrast('#e6e9ee', grounds) > 10);
  assert.ok(worstContrast('#2a3038', grounds) < 2);
  // 半透明面板叠在底色上：白色 10% 叠在黑底上仍然很暗
  const translucent = worstContrast('#ffffff', { bg: '0 0 0', surface: 'rgb(255 255 255 / 0.1)', surface2: 'rgba(255, 255, 255, 0.1)' });
  assert.ok(translucent > 10 && translucent < 21);
  assert.equal(worstContrast('not-a-colour', grounds), null);
});

// ---------------------------------------------------------------- 主窗口样式

test('main-window font sizes follow the font scale unless marked as a fixed-size box', async () => {
  const dir = new URL('../src/main/', import.meta.url);
  const files = (await readdir(dir)).filter((f) => f.endsWith('.vue') || f.endsWith('.css'));
  const offenders = [];
  for (const file of files) {
    const lines = (await readFile(new URL(file, dir), 'utf8')).split(/\r?\n/);
    lines.forEach((line, i) => {
      for (const rule of line.split('}')) {
        if (/font-size:\s*\d+(\.\d+)?px/.test(rule) && !rule.includes('字号不缩放')) offenders.push(`${file}:${i + 1}`);
      }
    });
  }
  assert.deepEqual(offenders, [], '主窗口的字号写成 calc(12px * var(--bax-font-scale))；固定尺寸框里的文字加注释“字号不缩放”');
});
