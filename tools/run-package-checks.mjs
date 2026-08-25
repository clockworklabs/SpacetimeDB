#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { releasePackages } from './release-packages.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pnpmCommand = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
const failures = [];

function run(packageDir, script) {
  console.log(`\n${packageDir}: ${script}`);
  const result = spawnSync(pnpmCommand, ['--dir', packageDir, 'run', script], {
    cwd: root,
    encoding: 'utf8',
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });
  if (result.status !== 0) failures.push(`${packageDir}:${script}`);
}

for (const packageDir of releasePackages) {
  const manifest = JSON.parse(
    readFileSync(resolve(root, packageDir, 'package.json'), 'utf8')
  );
  run(packageDir, 'lint');
  run(packageDir, 'typecheck');
  if (!manifest.scripts?.test) {
    failures.push(`${packageDir}:missing-test-script`);
    continue;
  }
  run(packageDir, 'test');
}

if (failures.length > 0) {
  console.error(`\nPackage checks failed: ${failures.join(', ')}`);
  process.exit(1);
}

console.log(`\nPackage checks passed for ${releasePackages.length} packages.`);
