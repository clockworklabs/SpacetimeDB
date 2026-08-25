#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pnpmCommand = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
const targets = [
  'spacetime-agents-ts/example',
  'spacetime-agents-ts/example/spacetimedb',
  'spacetime-api-keys-ts/example',
  'spacetime-files-ts/example',
  'spacetime-grid-ts/example',
  'spacetime-lobby-ts/example',
  'spacetime-posthog-ts/example',
  'spacetime-presence-ts/example',
  'spacetime-rate-limit-ts/example',
  'spacetime-resend-ts/example',
];

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
