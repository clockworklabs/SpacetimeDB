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
    packageDir,
    `${packageDir}/spacetimedb`,
    `${packageDir}/example/spacetimedb`,
  ])
  .filter(target => {
    const manifestPath = resolve(root, target, 'package.json');
    if (!existsSync(manifestPath)) return false;
    const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
    return manifest.scripts?.build === 'spacetime build';
  });

const failures = [];
for (const target of targets) {
  console.log(`\nBuilding ${target}`);
  const result = spawnSync(pnpmCommand, ['--dir', target, 'run', 'build'], {
    cwd: root,
    encoding: 'utf8',
    shell: process.platform === 'win32',
  });
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  const reportedRuntimeError = /^Error: Uncaught\b/m.test(output);
  if (reportedRuntimeError) {
    console.error(`Build reported a runtime error for ${target}.`);
  }
  if (result.status !== 0 || result.error || reportedRuntimeError) {
    failures.push(target);
  }
}

if (failures.length > 0) {
  console.error(`\nModule builds failed: ${failures.join(', ')}`);
  process.exit(1);
}

console.log(`\nModule builds passed for ${targets.length} release fixtures.`);
