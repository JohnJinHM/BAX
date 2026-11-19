import { readFile, mkdir, copyFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

const [database = 'C:/Projects/BA-Units/output/tables/Units.json', assets = 'C:/Projects/BA-DeckEditor/public/assets/thumbnails', returnIcon = 'C:/Projects/BATV/Unpack/Assets/Prefabs/GUI/HUD/Images/ActionPanel/Order_BackToBase.png'] = process.argv.slice(2);
const units = JSON.parse(await readFile(database, 'utf8'));
const target = path.resolve('public/unit-icons');
await mkdir(target, { recursive: true });
const manifest = {};
const copied = new Set();
for (const unit of units) {
  const file = unit.ThumbnailFileName;
  if (!file || /[\\/]/.test(file)) continue;
  try {
    if (!copied.has(file)) await copyFile(path.join(assets, `${file}.webp`), path.join(target, `${file}.webp`));
    copied.add(file);
    manifest[unit.Id] = { file: `${file}.webp`, names: [...new Set([unit.Name, unit.HUDName].filter(Boolean))] };
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
}
await writeFile('src/main/unit-icons.json', JSON.stringify(manifest, null, 2) + '\n');
await mkdir('public/ui-icons', { recursive: true });
await copyFile(returnIcon, 'public/ui-icons/back-to-base.png');
console.log(`Imported ${copied.size} images for ${Object.keys(manifest).length} unit IDs.`);
