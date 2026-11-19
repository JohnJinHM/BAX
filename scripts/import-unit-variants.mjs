import { readFile, writeFile, copyFile } from 'node:fs/promises';
import path from 'node:path';

const root = process.argv[2] ?? 'C:/Projects/BA-Units/output';
const assets = process.argv[3] ?? 'C:/Projects/BA-DeckEditor/public/assets/thumbnails';
const read = async (file) => JSON.parse(await readFile(path.join(root, file), 'utf8'));
const units = await read('tables/Units.json');
const mods = new Map((await read('tables/Modifications.json')).map(m => [m.Id, m]));
const loc = new Map(Object.entries(await read('localization/eng.json')).map(([k, v]) => [k.toLowerCase(), v]));
const label = key => key ? (loc.get(key.toLowerCase()) ?? key) : null;
const options = (await read('tables/Options.json')).map(o => {
  const mod = mods.get(o.ModificationId);
  if (!mod) throw new Error(`Missing modification ${o.ModificationId}`);
  return { id: o.Id, unit: mod.UnitId, mod: mod.Id, order: mod.Order,
    replace: o.ReplaceUnitId, name: label(o.ReplaceUnitName), suffix: label(o.ConcatenateWithUnitName), thumbnail: o.ThumbnailOverride };
});
const files = new Set([...units.map(u => u.ThumbnailFileName), ...options.map(o => o.thumbnail)].filter(Boolean));
const available = new Set();
for (const file of files) {
  if (/[\\/]/.test(file)) continue;
  try {
    await copyFile(path.join(assets, `${file}.webp`), `public/unit-icons/${file}.webp`);
    available.add(file);
  } catch (e) { if (e.code !== 'ENOENT') throw e; }
}
const thumbnail = file => available.has(file) ? `${file}.webp` : null;
const data = {
  version: (await read('manifest.json')).game_version,
  units: units.map(u => ({ id: u.Id, name: u.HUDName || u.Name, thumbnail: thumbnail(u.ThumbnailFileName) })),
  options: options.map(o => ({ ...o, thumbnail: thumbnail(o.thumbnail) })),
};
await writeFile('crates/bax-engine/src/unit-variants.json', JSON.stringify(data) + '\n');
console.log(`Imported ${data.units.length} units and ${data.options.length} options (${data.version}).`);
