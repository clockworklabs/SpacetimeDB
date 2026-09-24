import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { captureApplicationDiagnostics, controlAppServer, controlBackendRuntime, drainApplicationDatabase,
  parseRuntimeControlSpec }
  from '../src/runtime/backend-control.js';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { controlHostedAppServer, HOSTED_START_TIMEOUT_MS, hostedLaunchCommand, hostedRecordedProcessStopScript,
  hostedStopScript }
  from '../src/stacks/hosted-lifecycle.js';
import type { TextCommandOptions } from '../src/runtime/command-executor.js';
import { attemptDatabaseUrl } from '../src/stacks/hosted-database-identity.js';

interface RecordedCommand {
  argv: string[];
  options: TextCommandOptions;
}

test('crash database drain requires observed zero work and preserves errors or an expired deadline', async () => {
  for (const backend of ['postgres', 'mongodb']) {
    const id = 'a'.repeat(64);
    const lease = createBackendLease({ runId: `drain-${backend}`, backend, track: 'ecommerce', runIndex: 0,
      database: 'app_ecom_run0', container: { name: 'database', id } });
    lease.resources.network = { name: 'attempt', id: 'b'.repeat(64), namespaceContainerId: id,
      hostAddresses: ['172.20.0.1'], services: [], firewallSha256: null, firewallInstalledAt: null };
    const signal = new AbortController().signal;
    const counts = ['1', '0'];
    const receipt = await drainApplicationDatabase(lease, Date.now() + 5000, signal, (_command, args) => {
      if (args[0] === 'inspect') return id;
      assert.equal(args[0], 'exec');
      if (backend === 'postgres') assert.match(args.at(-1)!, /pg_stat_activity.*pid<>pg_backend_pid/);
      else {
        assert(args.some(arg => arg.includes('maxPoolSize=1')));
        assert.match(args.at(-1)!, /idleSessions:true/);
        assert.match(args.at(-1)!, /connectionId:\{\$ne:self\}/);
        // Only idle sessions end; a running commit is still waited for.
        assert.match(args.at(-1)!, /op\.type === 'idleSession' && op\.lsid/);
        assert.match(args.at(-1)!, /killSessions: idle/);
      }
      return counts.shift()!;
    });
    assert.equal(receipt.settled, true);
    assert.deepEqual(receipt.samples.map(sample => sample.pending), [1, 0]);
    const expired = await drainApplicationDatabase(lease, Date.now() - 1, signal, () => id);
    assert.equal(expired.settled, false);
    assert.deepEqual(expired.samples, []);
    const deadline = Date.now() + 20;
    const timedOut = await drainApplicationDatabase(lease, deadline, signal, (_command, args) => {
      if (args[0] === 'inspect') return id;
      while (Date.now() < deadline) { /* Simulate a probe using its remaining deadline. */ }
      throw Object.assign(new Error('private command details'), { code: 'ETIMEDOUT' });
    });
    assert.equal(timedOut.settled, false);
    assert.deepEqual(timedOut.samples, []);
    await assert.rejects(drainApplicationDatabase(lease, Date.now() + 5000, signal, (_command, args) => {
      if (args[0] === 'inspect') return id;
      throw Object.assign(new Error('private command details'), { code: 'ETIMEDOUT' });
    }), /could not observe pending database work/);
    const cancelled = new AbortController();
    await assert.rejects(drainApplicationDatabase(lease, Date.now() + 5000, cancelled.signal, (_command, args) => {
      if (args[0] === 'inspect') return id;
      cancelled.abort();
      return '1';
    }), /abort/i);
    for (const output of ['', '-1', 'NaN', '9007199254740992']) {
      await assert.rejects(drainApplicationDatabase(lease, Date.now() + 5000, signal,
        (_command, args) => args[0] === 'inspect' ? id : output), /invalid database work count/);
    }
    let samples = 0;
    await assert.rejects(drainApplicationDatabase(lease, Date.now() + 5000, signal, (_command, args) => {
      if (args[0] === 'inspect') return id;
      if (samples++ === 0) return '1';
      throw new Error('command failed with private database credentials');
    }), error => {
      assert(error instanceof Error && error.message === 'could not observe pending database work');
      assert('databaseDrain' in error);
      assert.deepEqual((error.databaseDrain as { samples: Array<{ pending: number }> }).samples.map(row => row.pending), [1]);
      return true;
    });
  }
});

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

test('clean-source application start uses the private attempt database URL', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-clean-source-'));
  const leasePath = join(root, 'lease.json');
  const priorPath = process.env.STACK_BENCH_LEASE;
  const priorToken = process.env.STACK_BENCH_LEASE_TOKEN;
  writeFileSync(join(root, 'start.sh'), '#!/bin/sh\nexec app\n');
  try {
    for (const backend of ['postgres', 'mongodb', 'spacetime']) {
      const anchor = 'a'.repeat(64);
      const id = 'c'.repeat(64);
      const database = 'app_ecom_run0';
      const lease = createBackendLease(backend === 'spacetime'
        ? { runId: 'clean-source-spacetime', backend, track: 'ecommerce', runIndex: 0,
          serverUri: 'http://127.0.0.1:3310', module: 'shop-run-1', dataDir: join(root, 'spacetime-data') }
        : { runId: `clean-source-${backend}`, backend, track: 'ecommerce', runIndex: 0, database,
          container: { name: 'database', id: anchor } });
      lease.state = 'active';
      const namespace = backend === 'spacetime' ? null : anchor;
      lease.resources.network = { name: 'attempt', id: 'b'.repeat(64), namespaceContainerId: namespace,
        hostAddresses: ['172.20.0.1'], services: [], firewallSha256: null, firewallInstalledAt: null };
      lease.resources.buildContainer = { name: 'build', id, owned: true, running: true,
        networkMode: namespace ? `container:${namespace}` : 'bridge', image: `sha256:${'d'.repeat(64)}`,
        resourceLimits: { cpuCount: 2, memoryBytes: 1024, memorySwapBytes: 1024, pids: 32 } };
      writeBackendLease(leasePath, lease);
      process.env.STACK_BENCH_LEASE = leasePath;
      process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
      const launchReached = new Error('launch captured without starting Docker');
      let launch: readonly string[] | undefined;
      let launchEnvironment: NodeJS.ProcessEnv | undefined;
      await assert.rejects(controlAppServer({ backend, app: root, port: 65534, probe: '' }, 'start', {
        exec: (_command, args, options) => {
          if (args[0] === 'inspect') return id;
          if (args[0] === 'exec' && args.includes('-d')) {
            launch = args;
            launchEnvironment = options.env;
            throw launchReached;
          }
          return '';
        },
      }), error => error === launchReached);
      assert(launch);
      if (backend === 'spacetime') {
        assert(!launch.includes('DATABASE_URL'));
        assert(launch.includes('VITE_MODULE_NAME') && launch.includes('VITE_SPACETIMEDB_URI'));
        assert.equal(launchEnvironment?.VITE_MODULE_NAME, 'shop-run-1');
        assert.equal(launchEnvironment?.VITE_SPACETIMEDB_URI, 'http://127.0.0.1:3310');
      } else {
        assert(launch.includes('DATABASE_URL'));
        assert.equal(launchEnvironment?.DATABASE_URL, attemptDatabaseUrl({ backend, database,
          ownershipToken: lease.ownershipToken }));
      }
      assert.equal(launchEnvironment?.VITE_PORT, '65534');
      assert.equal(launchEnvironment?.APP_WARM_START, '1');
      assert(!launch.some(value => value.includes('local-app-password')));
    }
  } finally {
    if (priorPath === undefined) delete process.env.STACK_BENCH_LEASE;
    else process.env.STACK_BENCH_LEASE = priorPath;
    if (priorToken === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN;
    else process.env.STACK_BENCH_LEASE_TOKEN = priorToken;
    rmSync(root, { recursive: true, force: true });
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
    return '===== /run/application/restart-mongodb-6723.log =====\nserver failed clearly\n';
  };
  try {
    assert.deepEqual(captureApplicationDiagnostics(output, { exec }), { captured: true, path: output });
    assert.match(readFileSync(output, 'utf8'), /server failed clearly/);
    const capture = calls[1];
    assert(capture);
    assert.deepEqual(capture.argv.slice(0, 4), ['docker', 'exec', buildContainer.id, 'sh']);
    assert.match(capture.argv.at(-1) ?? '', /reference-application\.log/);
    assert.match(capture.argv.at(-1) ?? '', /restart-\*\.log/);
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
  assert.match(command, /self_pgid=/);
  assert.match(command, /init_pgid=/);
  assert.match(command, /"\$pgid" = 1/);
  assert.match(command, /direct="\$direct \$pid"/);
  assert.throws(() => hostedStopScript('6301; rm -rf /'), /invalid hosted application port/);
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
    handoffWorkspace: true,
    exec,
  });

  const recordedStop = calls[1];
  const stop = calls[2];
  assert(recordedStop && stop);
  assert.deepEqual(recordedStop.args.slice(0, 3), ['exec', id, 'sh']);
  assert.match(recordedStop.args.at(-1) ?? '', /restart-mongodb-65534\.pid/);
  assert.deepEqual(stop.args.slice(0, 7), [
    'exec', '--user', '10001:10001', '-e', 'HOME=/home/developer', '-e', 'USER=developer',
  ]);
  assert.equal(stop.args[7], id);
  assert.match(stop.args.at(-1) ?? '', /lsof -ti tcp:65534 -sTCP:LISTEN/);
  assert(calls.some(call => call.args.slice(0, 3).join(' ') === `exec ${id} chown`));
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

test('hosted application start fails as soon as the launch exits without a listener and keeps its log', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-hosted-exit-'));
  const id = 'b'.repeat(64);
  writeFileSync(join(root, 'start.sh'), '#!/bin/sh\nexec app\n');
  const log = ['[start.sh] Installing dependencies', 'npm ERR! network request failed',
    'npm ERR! A complete log of this run can be found in: /home/developer/.npm/_logs/x.log'];
  const launches: string[] = [];
  const exec = (_command: string, args: readonly string[]): string => {
    if (args[0] === 'inspect') return `${id}\n`;
    if (args[0] === 'exec' && args[2] === 'tail') return `${log.join('\n')}\n`;
    const script = String(args.at(-1) ?? '');
    if (args[0] === 'exec' && args.includes('-d')) launches.push(script);
    if (/\/proc\/\$pid/.test(script) || /-sTCP:LISTEN\)"\s*\]/.test(script)) {
      throw Object.assign(new Error('exit 1'), { status: 1 });
    }
    return '';
  };
  try {
    for (const [adapterId, restarts] of [['postgres', 1], ['spacetime', 2]] as const) {
      launches.length = 0;
      for (let restart = 0; restart < restarts; restart++) {
        const startedAt = Date.now();
        await assert.rejects(controlHostedAppServer({
          adapterId,
          lease: { resources: { buildContainer: { name: 'leased-build', id, owned: true } } },
          app: root,
          port: 65533,
          probe: '/',
          mode: 'start',
          exec,
        }), error => error instanceof Error
          && error.message.startsWith(`${adapterId} application exited before it listened on port 65533: `)
          && error.message.includes('npm ERR! network request failed')
          && 'code' in error && error.code === 'generated_app_not_restartable'
          && 'startLog' in error && error.startLog === log.join('\n'));
        assert.ok(Date.now() - startedAt < HOSTED_START_TIMEOUT_MS / 10);
      }
      const logs = launches.map(script => script.match(/restart-[a-z]+-65533-[a-f0-9-]+\.log/)?.[0]);
      assert.equal(logs.length, restarts);
      assert(logs.every(Boolean));
      assert.equal(new Set(logs).size, restarts, 'restarts must not overwrite earlier process logs');
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});
