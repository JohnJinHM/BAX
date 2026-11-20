import { readFile, writeFile, stat } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

export async function releaseManifest(directory, version, notes = '') {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error('发布版本必须是 X.Y.Z');
  const platforms = {};
  for (const edition of ['Full', 'Lite']) {
    const name = `BAX-${edition}_${version}_x64-setup.exe`;
    const file = path.join(directory, name);
    if (!(await stat(file)).size) throw new Error(`安装包为空：${name}`);
    const signature = (await readFile(`${file}.sig`, 'utf8')).trim();
    if (!signature || !/^[A-Za-z0-9+/=\r\n]+$/.test(signature)) throw new Error(`无效签名：${name}`);
    platforms[`windows-x86_64-${edition.toLowerCase()}`] = {
      signature,
      url: `https://github.com/JohnJinHM/BAX/releases/download/v${version}/${name}`,
    };
  }
  return { version, notes, pub_date: new Date().toISOString(), platforms };
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const [directory, version, notesFile] = process.argv.slice(2);
  const notes = notesFile ? await readFile(notesFile, 'utf8') : `BAX ${version}：详见 Release 说明。`;
  const manifest = await releaseManifest(directory, version, notes);
  await writeFile(path.join(directory, 'latest.json'), JSON.stringify(manifest, null, 2) + '\n');
}
