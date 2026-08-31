import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { captureApplicationDiagnostics, controlBackendRuntime, hostedStopScript,
  parseRuntimeControlSpec }
  from '../src/runtime/backend-control.js';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { controlHostedAppServer, hostedLaunchCommand, hostedRecordedProcessStopScript }
  from '../src/stacks/stack-lifecycle-operations.js';
import type { TextCommandOptions } from '../src/runtime/command-executor.js';

interface RecordedCommand {
  argv: string[];
  options: TextCommandOptions;
}

test('runtime control input is typed at the serialized boundary', () => {
  const input = { backend: 'mongodb', app: '/app', port: 6301, probe: '/api/items' };
  assert.deepEqual(parseRuntimeControlSpec(input), input);
  assert.throws(() => parseRuntimeControlSpec({ ...input, port: null }),
    /runtime control spec is incomplete/);
  assert.throws(() => parseRuntimeControlSpec({ ...input, port: '6301' }),
    /runtime control spec is incomplete/);
});

test('backend control refuses without an authenticated lease', async () => {
  const priorPath = process.env.STACK_BENCH_LEASE;
  const priorToken = process.env.STACK_BENCH_LEASE_TOKEN;
  delete process.env.STACK_BENCH_LEASE;
  delete process.env.STACK_BENCH_LEASE_TOKEN;
  try {
    await assert.rejects(controlBackendRuntime({ backend: 'mongodb', app: '.', port: 6101, probe: '/api/items' }),
      /STACK_BENCH_LEASE is required/);
  } finally {
    if (priorPath === undefined) delete process.env.STACK_BENCH_LEASE;
    else process.env.STACK_BENCH_LEASE = priorPath;
    if (priorToken === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN;
    else process.env.STACK_BENCH_LEASE_TOKEN = priorToken;
  }
});

test('restart diagnostics are copied only from the exact leased build container', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-backend-log-'));
  const leasePath = join(root, 'lease.json');
  const output = join(root, 'restart.log');
  const lease = createBackendLease({ runId: 'diagnostics-test', backend: 'mongodb',
    track: 'ecommerce', runIndex: 0, database: 'app_ecom_run0',
    container: { name: 'database', id: 'd'.repeat(64) } });
  lease.state = 'active';
  lease.resources.buildContainer = { name: 'leased-build', id: 'a'.repeat(64),
    running: true, owned: true, image: `sha256:${'b'.repeat(64)}`,
    resourceLimits: { cpuCount: 2, memoryBytes: 1024, memorySwapBytes: 1024, pids: 32 } };
  writeBackendLease(leasePath, lease);
  const priorPath = process.env.STACK_BENCH_LEASE;
  const priorToken = process.env.STACK_BENCH_LEASE_TOKEN;
  process.env.STACK_BENCH_LEASE = leasePath;
  process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
  const buildContainer = lease.resources.buildContainer;
  assert(buildContainer);
  const calls: RecordedCommand[] = [];
  const exec = (command: string, args: readonly string[], options: TextCommandOptions): string => {
    calls.push({ argv: [command, ...args], options });
    if (args[0] === 'inspect') return `${buildContainer.id}\n`;
    return '===== /run/application/restart-mongodb-6673.log =====\nserver failed clearly\n';
  };
  try {
    assert.deepEqual(captureApplicationDiagnostics(output, { exec }), { captured: true, path: output });
    assert.match(readFileSync(output, 'utf8'), /server failed clearly/);
    const inspect = calls[0];
    const capture = calls[1];
    assert(inspect);
    assert(capture);
    assert.deepEqual(capture.argv.slice(0, 4), ['docker', 'exec', 'leased-build', 'sh']);
    assert.match(capture.argv.at(-1) ?? '', /reference-application\.log/);
    assert.match(capture.argv.at(-1) ?? '', /restart-\*\.log/);
    assert.equal(inspect.options.timeout, 120_000);
    assert.equal(capture.options.timeout, 120_000);
  } finally {
    if (priorPath === undefined) delete process.env.STACK_BENCH_LEASE;
    else process.env.STACK_BENCH_LEASE = priorPath;
    if (priorToken === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN;
    else process.env.STACK_BENCH_LEASE_TOKEN = priorToken;
    rmSync(root, { recursive: true, force: true });
  }
});

test('hosted application stop targets safe process groups and exact group-1 listeners', () => {
  const command = hostedStopScript(6301);
  assert.ok((command.match(/lsof -ti tcp:6301 -sTCP:LISTEN/g)?.length ?? 0) >= 3,
    'listener ownership must be reacquired before TERM, before KILL, and during final verification');
  assert.match(command, /ps -o pgid=/);
  assert.match(command, /\/bin\/kill -TERM -- "-\$pgid"/);
  assert.match(command, /\/bin\/kill -TERM "\$pid"/);
  assert.match(command, /\/bin\/kill -KILL -- "-\$pgid"/);
  assert.match(command, /\/bin\/kill -KILL "\$pid"/);
  assert.match(command, /quiet.*-ge 10/);
  assert.match(command, /hosted application port 6301 still has a listener/);
  assert.match(command, /self_pgid=/);
  assert.match(command, /init_pgid=/);
  assert.match(command, /"\$pgid" = 1/);
  assert.match(command, /direct="\$direct \$pid"/);
  assert.match(command, /unsafe listener pid/);
  assert.throws(() => hostedStopScript('6301; rm -rf /'), /invalid hosted application port/);
});

test('hosted application stop targets its recorded launch before a port listener exists', () => {
  const command = hostedRecordedProcessStopScript('/run/application/restart-mongodb-6301.pid');
  assert.match(command, /cat "\$record"/);
  assert.match(command, /current_started="\$\{20\}"/);
  assert.match(command, /current_pgid="\$3"/);
  assert.match(command, /current_started" != "\$started".*rm -f "\$record"; exit 0/s);
  assert(command.indexOf('current_started') < command.indexOf('/bin/kill -TERM'));
  assert.match(command, /\/bin\/kill -TERM -- "-\$pid"/);
  assert.match(command, /\/bin\/kill -KILL -- "-\$pid"/);
  assert.match(command, /application process group is still running/);
  assert.match(command, /rm -f "\$record"/);
  assert.throws(() => hostedRecordedProcessStopScript('/app/server.pid'),
    /invalid hosted application process record/);
});

test('hosted application launch supports generated and fixed reference apps', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-hosted-launch-'));
  try {
    writeFileSync(join(root, 'start.sh'), '#!/usr/bin/env bash\nset -euo pipefail\nexec app\n');
    assert.deepEqual(hostedLaunchCommand(root), {
      directory: '.', command: '/bin/bash ./start.sh',
    });
    rmSync(join(root, 'start.sh'));
    writeFileSync(join(root, 'package.json'), JSON.stringify({
      scripts: { start: 'node index.js', dev: 'node --watch index.js' },
    }));
    assert.deepEqual(hostedLaunchCommand(root), {
      directory: '.', command: '/usr/local/bin/npm run start',
    });
    writeFileSync(join(root, 'package.json'), JSON.stringify({ scripts: { dev: 'vite' } }));
    assert.throws(() => hostedLaunchCommand(root), error =>
      error instanceof Error
      && error.message === 'app has no start.sh or npm start script'
      && 'code' in error && error.code === 'generated_app_not_restartable');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('hosted application control inspects and stops listeners as the application user', async () => {
  const id = 'a'.repeat(64);
  const calls: Array<{ command: string; args: readonly string[]; options: TextCommandOptions }> = [];
  const exec = (command: string, args: readonly string[], options: TextCommandOptions): string => {
    calls.push({ command, args, options });
    if (args[0] === 'inspect') return `${id}\n`;
    return '';
  };

  await controlHostedAppServer({
    adapterId: 'mongodb',
    lease: { resources: { buildContainer: { name: 'leased-build', id, owned: true } } },
    app: '.',
    port: 65534,
    probe: '/',
    mode: 'stop',
    exec,
  });

  const recordedStop = calls[1];
  const stop = calls[2];
  assert(recordedStop && stop);
  assert.deepEqual(recordedStop.args.slice(0, 3), ['exec', 'leased-build', 'sh']);
  assert.match(recordedStop.args.at(-1) ?? '', /restart-mongodb-65534\.pid/);
  assert.deepEqual(stop.args.slice(0, 7), [
    'exec', '--user', '10001:10001', '-e', 'HOME=/home/developer', '-e', 'USER=developer',
  ]);
  assert.equal(stop.args[7], 'leased-build');
  assert.match(stop.args.at(-1) ?? '', /lsof -ti tcp:65534 -sTCP:LISTEN/);
});

test('application control rejects unsupported modes before touching a container', async () => {
  let calls = 0;
  await assert.rejects(controlHostedAppServer({
    adapterId: 'mongodb',
    lease: { resources: { buildContainer: null } },
    app: '.',
    port: 6301,
    probe: '/',
    mode: 'invalid' as 'restart',
    exec: () => { calls += 1; return ''; },
  }), /unsupported application control mode invalid/);
  assert.equal(calls, 0);

  assert.throws(() => STACK_ADAPTER_REGISTRY.get('spacetime').lifecycle.control!({
    adapterId: 'spacetime', lease: {} as never, app: '.', port: 6301, probe: '/', mode: 'stop',
  }), /unsupported SpacetimeDB control mode stop/);
});

test('SpacetimeDB application start uses the root contract independently of its database host', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-hosted-failure-'));
  const id = 'a'.repeat(64);
  const abort = new AbortController();
  abort.abort(new Error('stop waiting'));
  writeFileSync(join(root, 'start.sh'), '#!/bin/sh\nexec app\n');
  const calls: string[][] = [];
  const exec = (_command: string, args: readonly string[]): string => {
    calls.push([...args]);
    if (args[0] === 'inspect') return `${id}\n`;
    if (args[0] === 'exec' && args[2] === 'tail') {
      return './start.sh: 2: set: Illegal option -o pipefail\n';
    }
    return '';
  };
  try {
    await assert.rejects(controlHostedAppServer({
      adapterId: 'spacetime',
      lease: { resources: { buildContainer: { name: 'leased-build', id, owned: true } } },
      app: root,
      port: 65534,
      probe: '/',
      mode: 'start',
      signal: abort.signal,
      exec,
    }), error => error instanceof Error
      && error.message.includes('set: Illegal option -o pipefail')
      && 'code' in error && error.code === 'generated_app_not_restartable');
    const launch = calls.find(args => args[0] === 'exec' && args.includes('-d'));
    assert(launch);
    assert.match(launch.at(-1) ?? '', /\/bin\/bash \.\/start\.sh/);
    assert.match(launch.at(-1) ?? '', /restart-spacetime-65534\.log/);
    assert.match(launch.at(-1) ?? '', /\/usr\/bin\/setsid/);
    assert.match(launch.at(-1) ?? '', /restart-spacetime-65534\.pid/);
    assert.match(launch.at(-1) ?? '', /\/proc\/\$\$\/stat/);
    assert.match(launch.at(-1) ?? '', /\(umask 077; printf/);
    assert.match(launch.at(-1) ?? '', /printf "%s %s\\n" "\$\$" "\$\{20\}"/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
