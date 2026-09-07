#!/usr/bin/env node

import { spawn } from 'node:child_process';
import { Socket } from 'node:net';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { chromium } from 'playwright';
import type { LaunchOptions } from 'playwright';
import { compiledEntrypoint } from '../src/package-root.js';
import { readBackendLease } from '../src/runtime/backend-lease.js';

function browserContainer(): string | null {
  const path = process.env.STACK_BENCH_LEASE;
  if (!path) {
    if (process.env.STACK_BENCH_APPLIANCE === '1') throw new Error('browser requires a private attempt lease');
    return null;
  }
  const token = process.env.STACK_BENCH_LEASE_TOKEN;
  if (!token) throw new Error('browser requires the attempt ownership token');
  const lease = readBackendLease(path, { token });
  if (lease.backend === 'stub') return null;
  const container = lease.resources.browserContainer;
  if (!container?.owned || container.running === false || !/^[a-f0-9]{64}$/.test(container.id)) {
    throw new Error('browser requires the exact running attempt browser container');
  }
  return container.id;
}

export function attemptBrowserLaunchOptions(): LaunchOptions {
  return browserContainer() ? {
    executablePath: compiledEntrypoint('container', 'browser-pipe.js'),
    // The owned browser has private shared memory; do not fill /tmp with IPC buffers.
    ignoreDefaultArgs: ['--disable-dev-shm-usage'],
  } : {};
}

// Playwright uses fd 3/4. Docker carries these bytes over stdin/stdout, so no
// browser control socket is reachable from the generated app's network.
function main(): void {
  const id = browserContainer();
  if (!id) throw new Error('browser pipe requires an isolated attempt');
  const child = spawn('docker', ['exec', '-i', id, 'sh', '-c',
    'exec 3<&0 4>&1 1>&2; exec "$@"', 'sh', chromium.executablePath(), ...process.argv.slice(2)],
  { stdio: ['pipe', 'pipe', 'inherit'] });
  const input = new Socket({ fd: 3, readable: true, writable: false });
  const output = new Socket({ fd: 4, readable: false, writable: true });
  input.pipe(child.stdin);
  child.stdout.pipe(output);
  const close = () => { input.destroy(); child.stdin.end(); };
  process.on('SIGTERM', close);
  process.on('SIGINT', close);
  child.stdin.on('error', error => {
    if ('code' in error && error.code === 'EPIPE') close();
    else throw error;
  });
  child.on('error', error => { console.error(error.message); process.exit(1); });
  child.on('exit', () => { input.destroy(); child.stdin.destroy(); });
  // fd 3 can still have a pending read while Playwright waits for this process
  // to exit. Docker's close event means all browser output has been forwarded.
  child.on('close', code => { process.exit(code ?? 1); });
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
