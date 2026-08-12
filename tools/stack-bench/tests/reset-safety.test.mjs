import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { createBackendLease, writeBackendLease } from '../backend-lease.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const SCRIPT_DIR = join(HERE, '..');
const bashUsesMnt = execFileSync('bash', ['-lc', 'pwd'], { cwd: SCRIPT_DIR, encoding: 'utf8' })
  .trim().startsWith('/mnt/');
const posixPath = path => path
  .replace(/^([A-Za-z]):/, (_, drive) => bashUsesMnt ? `/mnt/${drive.toLowerCase()}` : `/${drive.toLowerCase()}`)
  .replaceAll('\\', '/');
const RESET_FOR_BASH = './reset-db.sh';
const bashEnv = overrides => ({
  ...process.env,
  ...overrides,
  WSLENV: [process.env.WSLENV, ...Object.keys(overrides)].filter(Boolean).join(':'),
});

function workspace() {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reset-'));
  const app = join(root, 'app');
  mkdirSync(join(app, 'client', 'src'), { recursive: true });
  mkdirSync(join(app, 'backend', 'spacetimedb'), { recursive: true });
  writeFileSync(join(app, 'client', 'src', 'config.ts'),
    "export const MODULE_NAME = 'victim-module';\nexport const URI = 'https://production.example';\n");
  const capture = join(root, 'capture.txt');
  const fake = join(root, 'spacetime-fake.sh');
  writeFileSync(fake, '#!/usr/bin/env bash\nprintf "%s\\n" "$@" > "$CAPTURE"\n');
  chmodSync(fake, 0o755);
  const leasePath = join(root, 'lease.json');
  const lease = createBackendLease({
    runId: 'ecommerce-spacetime-run7-test', backend: 'spacetime', track: 'ecommerce', runIndex: 7,
    serverUri: 'http://127.0.0.1:43210', module: 'stackbench-ecom-run7',
    dataDir: join(root, 'data'),
  });
  lease.state = 'active';
  lease.resources.listenerPids = [12345];
  writeBackendLease(leasePath, lease);
  return { root, app, capture, fake, leasePath, lease };
}

test('reset refuses without an authenticated backend lease', () => {
  const ws = workspace();
  try {
    assert.throws(() => execFileSync('bash', [RESET_FOR_BASH, 'spacetime', posixPath(ws.app), '7', 'ecom'], {
      env: bashEnv({
        STACK_BENCH_LEASE: '',
        STACK_BENCH_LEASE_TOKEN: '',
        SPACETIME_BIN: posixPath(ws.fake),
      }),
      cwd: SCRIPT_DIR,
      stdio: 'pipe',
    }), /Command failed/);
  } finally {
    rmSync(ws.root, { recursive: true, force: true });
  }
});

test('reset uses the harness module and URI instead of generated config', () => {
  const ws = workspace();
  try {
    execFileSync('bash', [RESET_FOR_BASH, 'spacetime', posixPath(ws.app), '7', 'ecom'], {
      env: bashEnv({
        STACK_BENCH_LEASE: posixPath(ws.leasePath),
        STACK_BENCH_LEASE_TOKEN: ws.lease.ownershipToken,
        SPACETIME_BIN: posixPath(ws.fake),
        CAPTURE: posixPath(ws.capture),
      }),
      cwd: SCRIPT_DIR,
      stdio: 'pipe',
    });
    const args = readFileSync(ws.capture, 'utf8');
    assert.match(args, /^publish\nstackbench-ecom-run7\n/m);
    assert.match(args, /http:\/\/127\.0\.0\.1:43210/);
    assert.doesNotMatch(args, /victim-module|production\.example/);
  } finally {
    rmSync(ws.root, { recursive: true, force: true });
  }
});

test('reset refuses a non-loopback destructive target', () => {
  const ws = workspace();
  try {
    const unsafe = { ...ws.lease, resources: { ...ws.lease.resources,
      serverUri: 'https://production.example:443' } };
    writeFileSync(ws.leasePath, JSON.stringify(unsafe));
    assert.throws(() => execFileSync('bash', [RESET_FOR_BASH, 'spacetime', posixPath(ws.app), '7', 'ecom'], {
      env: bashEnv({
        STACK_BENCH_LEASE: posixPath(ws.leasePath),
        STACK_BENCH_LEASE_TOKEN: ws.lease.ownershipToken,
        SPACETIME_BIN: posixPath(ws.fake),
        CAPTURE: posixPath(ws.capture),
      }),
      cwd: SCRIPT_DIR,
      stdio: 'pipe',
    }), /Command failed/);
  } finally {
    rmSync(ws.root, { recursive: true, force: true });
  }
});
