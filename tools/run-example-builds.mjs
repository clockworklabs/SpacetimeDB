#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { releasePackages } from './release-packages.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pnpmCommand = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
const targets = releasePackages
  .map(packageDir => `${packageDir}/example`)
  .filter(target => {
    const manifestPath = resolve(root, target, 'package.json');
    if (!existsSync(manifestPath)) return false;
    const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
    return Boolean(manifest.scripts?.build);
  });

const failures = [];
for (const target of targets) {
  console.log(`\nBuilding ${target}`);
  const result = spawnSync(pnpmCommand, ['--dir', target, 'run', 'build'], {
    cwd: root,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });
  if (result.status !== 0) failures.push(target);
}

if (failures.length > 0) {
  console.error(`\nExample builds failed: ${failures.join(', ')}`);
  process.exit(1);
}

console.log(`\nExample builds passed for ${targets.length} browser examples.`);
