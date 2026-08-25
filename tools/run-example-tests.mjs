#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { releasePackages } from './release-packages.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pnpmCommand = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
const targets = releasePackages
  .flatMap(packageDir => [
    `${packageDir}/example`,
    `${packageDir}/example/spacetimedb`,
  ])
  .filter(target => {
    const manifestPath = resolve(root, target, 'package.json');
    if (!existsSync(manifestPath)) return false;
    const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
    return Boolean(manifest.scripts?.['test:unit']);
  });

const failures = [];
for (const target of targets) {
  console.log(`\nTesting ${target}`);
  const result = spawnSync(pnpmCommand, ['--dir', target, 'run', 'test:unit'], {
    cwd: root,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });
  if (result.status !== 0) failures.push(target);
}

if (failures.length > 0) {
  console.error(`\nExample tests failed: ${failures.join(', ')}`);
  process.exit(1);
}

console.log(`\nExample tests passed for ${targets.length} targeted suites.`);
