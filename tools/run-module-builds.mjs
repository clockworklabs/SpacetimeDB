#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const pnpmCommand = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
const targets = [
  'spacetime-agents-ts/spacetimedb',
  'spacetime-agents-ts/example/spacetimedb',
  'spacetime-api-keys-ts/example/spacetimedb',
  'spacetime-auth-ts/spacetimedb',
  'spacetime-auth-ts/example/spacetimedb',
  'spacetime-cron-ts/spacetimedb',
  'spacetime-cron-ts/example/spacetimedb',
  'spacetime-files-ts/example/spacetimedb',
  'spacetime-grid-ts/example/spacetimedb',
  'spacetime-lobby-ts',
  'spacetime-lobby-ts/example/spacetimedb',
  'spacetime-posthog-ts',
  'spacetime-posthog-ts/example/spacetimedb',
  'spacetime-presence-ts/spacetimedb',
  'spacetime-presence-ts/example/spacetimedb',
  'spacetime-rate-limit-ts/spacetimedb',
  'spacetime-rate-limit-ts/example/spacetimedb',
  'spacetime-resend-ts',
  'spacetime-resend-ts/example/spacetimedb',
  'spacetime-retry-ts/spacetimedb',
  'spacetime-stripe-ts',
  'spacetime-stripe-ts/example/spacetimedb',
];

const failures = [];
for (const target of targets) {
  let passed = false;
  for (let attempt = 1; attempt <= 2; attempt += 1) {
    const suffix = attempt === 1 ? '' : ' (retry after transient failure)';
    console.log(`\nBuilding ${target}${suffix}`);
    const result = spawnSync(pnpmCommand, ['--dir', target, 'run', 'build'], {
      cwd: root,
      encoding: 'utf8',
      shell: process.platform === 'win32',
    });
    if (result.stdout) process.stdout.write(result.stdout);
    if (result.stderr) process.stderr.write(result.stderr);
    const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
    const reportedRuntimeError = /^Error: Uncaught\b/m.test(output);
    if (result.status === 0 && !result.error && !reportedRuntimeError) {
      passed = true;
      break;
    }
    if (reportedRuntimeError) {
      console.error(`Build reported a runtime error for ${target}.`);
    }
    if (attempt === 1) {
      console.warn(`Build failed for ${target}; retrying once.`);
    }
  }
  if (!passed) failures.push(target);
}

if (failures.length > 0) {
  console.error(`\nModule builds failed: ${failures.join(', ')}`);
  process.exit(1);
}

console.log(`\nModule builds passed for ${targets.length} release fixtures.`);
