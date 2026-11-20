import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { releaseManifest } from '../scripts/release-manifest.mjs';

test('release manifest requires both editions and signatures and keeps edition URLs separate', async () => {
  const dir = await mkdtemp(path.join(tmpdir(), 'bax-release-'));
  try {
    await assert.rejects(releaseManifest(dir, '../bad'));
    const full = path.join(dir, 'BAX-Full_0.2.1_x64-setup.exe');
    const lite = path.join(dir, 'BAX-Lite_0.2.1_x64-setup.exe');
    await writeFile(full, 'fixture');
    await assert.rejects(releaseManifest(dir, '0.2.1'));
    await writeFile(full + '.sig', 'ZnVsbA==\n');
    await assert.rejects(releaseManifest(dir, '0.2.1'));
    await writeFile(lite, 'fixture');
    await writeFile(lite + '.sig', 'bGl0ZQ==\n');
    const manifest = await releaseManifest(dir, '0.2.1', '修复说明');
    assert.equal(manifest.notes, '修复说明');
    assert.equal(manifest.platforms['windows-x86_64-full'].signature, 'ZnVsbA==');
    assert.match(manifest.platforms['windows-x86_64-lite'].url, /v0\.2\.1\/BAX-Lite_0\.2\.1_x64-setup\.exe$/);
    await writeFile(lite + '.sig', '');
    await assert.rejects(releaseManifest(dir, '0.2.1'));
  } finally { await rm(dir, { recursive: true, force: true }); }
});
