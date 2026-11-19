import { mkdir, copyFile } from 'node:fs/promises';
import path from 'node:path';

const root = process.argv[2] ?? 'C:/Projects/BATV/Unpack';
const gui = 'Assets/Prefabs/GUI';
const map = `${gui}/HUD/Images/MapPreviewDraw`;
const sources = {
  'recon.png': `${map}/MapDrawButton_Recon.png`,
  'infantry.png': `${map}/MapDrawButton_Inf.png`,
  'tank.png': `${map}/MapDrawButton_Tank.png`,
  'air-defense.png': `${map}/MapDrawButton_AA.png`,
  'artillery.png': `${map}/MapDrawButton_Arty.png`,
  'helicopter.png': `${map}/MapDrawButton_Heli.png`,
  'jet.png': `${map}/MapDrawButton_Jet.png`,
  'vehicle.png': `${gui}/Arsenal/Images/UnitTypeIcons/vehicle.png`,
  'supply.png': 'Images/Labels/Icons/supply-Label.png',
};
await mkdir('public/ui-icons/types', { recursive: true });
for (const [file, source] of Object.entries(sources)) {
  await copyFile(path.join(root, source), path.join('public/ui-icons/types', file));
}
console.log(`Imported ${Object.keys(sources).length} original game type icons.`);
