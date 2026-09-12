import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { chmodSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'node:net';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { controlAppServer } from '../src/runtime/backend-control.js';
import { restoreRepairSource } from '../src/runtime/source-materialization.js';
import { snapshotAppSource } from '../src/runtime/source-snapshot.js';
import { resetMutationDatabase } from '../grader/mutation-test.js';

const docker = (args: string[]): string => execFileSync('docker', args,
  { encoding: 'utf8', stdio: 'pipe', timeout: 120_000, windowsHide: true }).trim();
const freePort = async (): Promise<number> => {
  const server = createServer();
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  assert(address && typeof address !== 'string');
  await new Promise<void>(resolve => server.close(() => resolve()));
  return address.port;
};

test('rejected schema repair restores accepted source and a fresh spacetime dev watcher', {
  skip: process.env.STACK_BENCH_REPAIR_SOURCE_DOCKER !== '1', timeout: 240_000,
}, async () => {
  const image = process.env.STACK_BENCH_REPAIR_BUILD_IMAGE;
  const deps = process.env.STACK_BENCH_REPAIR_DEPS_VOLUME;
  assert(image && deps, 'set immutable build image and release dependency volume');
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-'));
  const app = join(root, 'app');
  const accepted = join(root, 'accepted');
  const name = `stack-bench-repair-proof-${process.pid}-${Date.now()}`;
  const port = await freePort();
  const backendPort = await freePort();
  const source = join(app, 'spacetimedb', 'src', 'index.ts');
  mkdirSync(join(app, 'spacetimedb', 'src'), { recursive: true });
  writeFileSync(join(app, 'spacetimedb', 'package.json'), JSON.stringify({
    type: 'module', dependencies: { spacetimedb: 'file:/deps/spacetimedb.tgz' },
  }));
  writeFileSync(join(app, 'spacetimedb', 'tsconfig.json'), '{}');
  const moduleSource = (type: string, value: string): string =>
    `import { schema, table, t } from 'spacetimedb/server';\n`
    + `const db = schema({ proof: table({ public: true }, { value: t.${type}() }) });\n`
    + `export default db; export const seed = db.reducer(ctx => { if (ctx.db.proof.count() === 0n) ctx.db.proof.insert({ value: ${value} }); });\n`;
  writeFileSync(source, moduleSource('string', "'accepted'"));
  writeFileSync(join(app, 'server.cjs'), `require('http').createServer((q,s)=>s.end('ready')).listen(${port},'0.0.0.0');`);
  writeFileSync(join(app, 'start.sh'), `#!/bin/bash\nset -eu\n`
    + `exec /deps/spacetimedb-cli dev repair-proof --no-config --project-path /app --module-path /app/spacetimedb `
    + `--module-bindings-path client/src/module_bindings --client-lang typescript -s http://127.0.0.1:3299 -y `
    + `--run '/deps/spacetimedb-cli call repair-proof seed -s http://127.0.0.1:3299 -y && node /app/server.cjs'\n`);
  let id: string | undefined;
  const previous = { path: process.env.STACK_BENCH_LEASE, token: process.env.STACK_BENCH_LEASE_TOKEN };
  try {
    id = docker(['run', '-d', '--name', name, '--init', '--pull=never',
      '-p', `127.0.0.1:${port}:${port}`, '-p', `127.0.0.1:${backendPort}:3299`,
      '--mount', `type=bind,source=${app},target=/app`,
      '--mount', `type=volume,source=${deps},target=/deps,readonly`,
      '--entrypoint', 'sh', image, '-c',
      'mkdir -p /run/application /home/developer; chown 10001:10001 /home/developer; '
      + 'openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out /tmp/key.pem; '
      + 'openssl ec -in /tmp/key.pem -pubout -out /tmp/public.pem; '
      + 'exec /deps/spacetimedb-standalone start --listen-addr 0.0.0.0:3299 --data-dir /tmp/backend '
      + '--jwt-priv-key-path /tmp/key.pem --jwt-pub-key-path /tmp/public.pem']);
    for (let attempt = 0; ; attempt++) {
      try { docker(['exec', id, 'curl', '-fsS', 'http://127.0.0.1:3299/v1/ping']); break; }
      catch (error) { if (attempt === 40) throw error; await delay(250); }
    }
    docker(['exec', '-w', '/app/spacetimedb', id, 'npm', 'install', '--ignore-scripts', '--no-audit', '--no-fund']);
    const lease = createBackendLease({ runId: name, backend: 'spacetime', track: 'chat', runIndex: 0,
      serverUri: `http://127.0.0.1:${backendPort}`, module: 'repair-proof', dataDir: join(root, 'data') });
    lease.state = 'active';
    lease.resources.buildContainer = { name, id, image, running: true, owned: true, networkMode: 'bridge',
      resourceLimits: { cpuCount: 2, memoryBytes: 2147483648, memorySwapBytes: 2147483648, pids: 512 } };
    const path = join(root, 'lease.json');
    writeBackendLease(path, lease);
    process.env.STACK_BENCH_LEASE = path;
    process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
    const spec = { backend: 'spacetime', app, port, probe: '/' };
    const cli = (...args: string[]): string => docker(['exec', '--user', '10001:10001',
      '-e', 'HOME=/home/developer', id!, '/deps/spacetimedb-cli', ...args]);
    const publish = (...args: string[]): string => cli('publish', 'repair-proof',
      '--module-path', '/app/spacetimedb', '-s', 'http://127.0.0.1:3299', '-y', ...args);
    const query = (): string => cli('sql', 'repair-proof', '-s', 'http://127.0.0.1:3299', 'select * from proof');
    await controlAppServer(spec, 'start');
    assert.match(query(), /accepted/);
    await resetMutationDatabase({ backend: 'spacetime', app, track: 'chat',
      reseedOnReset: true, restartSpec: spec }, null);
    assert.match(query(), /accepted/, 'isolated reset must run startup seeding outside init');
    await controlAppServer(spec, 'stop');
    chmodSync(source, 0o660);
    chmodSync(join(app, 'spacetimedb', 'package.json'), 0o660);
    snapshotAppSource(app, accepted);
    writeFileSync(source, moduleSource('u32', '7'));
    publish('--delete-data');
    await controlAppServer(spec, 'start');
    const watchers = (): string => docker(['exec', id!, 'sh', '-c', "pgrep -f '^/deps/spacetimedb-cli dev ' || true"]);
    const rejectedWatcher = watchers();
    assert.match(rejectedWatcher, /^\d+$/);
    await controlAppServer(spec, 'stop');
    assert.equal(watchers(), '', 'stop must terminate the rejected dev watcher');
    writeFileSync(source, moduleSource('string', "'accepted'"));
    assert.throws(() => publish(), error => {
      const failure = error as { stderr?: string; stdout?: string };
      return /migration|schema|breaking/i.test(`${failure.stderr}\n${failure.stdout}`);
    }, 'source-only rollback must reproduce the incompatible-schema refusal');
    writeFileSync(source, moduleSource('u32', '7'));
    await controlAppServer(spec, 'start');
    await restoreRepairSource(accepted, app, spec);
    assert.match(query(), /accepted/, 'grading can read the accepted schema and seed after recovery');
    assert.match(watchers(), /^\d+$/);
    assert.notEqual(watchers(), rejectedWatcher);
    assert.equal((await fetch(`http://127.0.0.1:${port}/`)).status, 200);
    await controlAppServer(spec, 'stop');
    assert.equal(watchers(), '', 'final stop must leave no dev watcher');
  } catch (error) {
    if (id) console.error(docker(['logs', '--tail', '12', id]));
    throw error;
  } finally {
    if (previous.path === undefined) delete process.env.STACK_BENCH_LEASE;
    else process.env.STACK_BENCH_LEASE = previous.path;
    if (previous.token === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN;
    else process.env.STACK_BENCH_LEASE_TOKEN = previous.token;
    if (id) docker(['rm', '-f', id]);
    rmSync(root, { recursive: true, force: true });
  }
});
