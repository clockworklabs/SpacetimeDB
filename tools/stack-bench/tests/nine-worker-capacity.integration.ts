import assert from 'node:assert/strict';
import { execFile, execFileSync, spawn } from 'node:child_process';
import type { ChildProcess } from 'node:child_process';
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync,
  rmSync, statfsSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { chromium } from 'playwright';
import { attemptBrowserLaunchOptions } from '../container/browser-pipe.js';
import { createBackendLease, claimBackendResources, backendResourceLockKeys,
  readBackendLease, resourceLockScope } from '../src/runtime/backend-lease.js';
import { dockerNetworkMissing, releaseBackendLease } from '../src/runtime/backend-teardown.js';
import { requireLeasedDatabase, requireLeasedSpacetime } from '../src/stacks/backend-reset-guard.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { dbName, loadTrack, moduleName, portsFor } from '../src/composition/tracks.js';
import { BUILD_CONTAINER_RESOURCE_LIMITS, SIDECAR_CONTAINER_RESOURCE_LIMITS }
  from '../src/composition/product-config.js';
import { codingContainerAgentCommand, codingContainerAgentExecOptions }
  from '../src/runtime/coding-container-policy.js';
import { createDatabaseWriteCapability } from '../src/actions/runtime-action-executors.js';
import { redactCredentials } from '../src/evidence/diagnostic-sanitizer.js';
import { compiledEntrypoint } from '../src/package-root.js';

// Diagnostic capacity measurement, not normal-preflight, paid-agent, broker-load,
// or full-grade proof. Root supplies the existing controller, state/deps mounts,
// exact image IDs, /host/proc and /host/cgroup (read-only). No image builds here.
const BACKENDS = ['spacetime', 'postgres', 'mongodb'] as const;
const WORKERS = 9;
const STARTUP_MS = 300_000;
const HOLD_MS = 20_000;
const CLEANUP_MS = 90_000;
const RESERVE_MEMORY = 8 * 1024 ** 3;
const RESERVE_DISK = 10 * 1024 ** 3;
const execute = promisify(execFile);
const self = fileURLToPath(import.meta.url);
const docker = (args: string[]) => execFileSync('docker', args,
  { encoding: 'utf8', stdio: 'pipe', timeout: 10_000 }).trim();
const json = (path: string) => JSON.parse(readFileSync(path, 'utf8'));
const write = (path: string, value: unknown) => writeFileSync(path, JSON.stringify(value, null, 2));
const leasePath = (root: string, index: number) => join(root, '.private', String(index), 'lease.json');
const appPath = (root: string, index: number) => join(root, 'work', String(index), 'app');
const numbers = (text: string): Record<string, number> => Object.fromEntries(text.trim().split('\n')
  .map(line => { const [key, value] = line.trim().split(/\s+/); return [key!, Number(value)]; }));

async function barrier(root: string, name: string, deadline: number): Promise<void> {
  while (!existsSync(join(root, name))) {
    assert(Date.now() < deadline, `timed out waiting for ${name}`);
    await delay(50);
  }
}

async function worker(root: string, index: number): Promise<void> {
  const backend = BACKENDS[index % BACKENDS.length]!;
  const track = loadTrack('ecommerce');
  const ports = portsFor(track, backend, index);
  const adapter = STACK_ADAPTER_REGISTRY.get(backend);
  const deadline = Number(process.env.STACK_BENCH_CAPACITY_DEADLINE);
  const app = appPath(root, index), path = leasePath(root, index);
  mkdirSync(join(root, '.private', String(index)), { recursive: true, mode: 0o700 });
  const lease = createBackendLease({ backend, runId: `${basename(root)}-${index}`, track: track.name,
    runIndex: index, database: backend === 'spacetime' ? null : dbName(track, index),
    module: backend === 'spacetime' ? moduleName(track, index) : null,
    serverUri: backend === 'spacetime' ? `http://127.0.0.1:${3290 + index}` : null,
    dataDir: backend === 'spacetime' ? join(root, 'work', String(index), 'data') : null });
  claimBackendResources(path, lease, { ...resourceLockScope(),
    keys: backendResourceLockKeys(lease, ports) });
  Object.assign(process.env, { STACK_BENCH_LEASE: path, STACK_BENCH_LEASE_TOKEN: lease.ownershipToken });
  if (lease.resources.serverUri) process.env.STACK_BENCH_STDB_URI = lease.resources.serverUri;
  adapter.lifecycle.activate({ leasePath: path, leaseToken: lease.ownershipToken, lease, ports });
  process.send!({ stage: 'activated', index, at: Date.now() });
  await barrier(root, 'deploy-go', deadline);
  const deploymentStarted = Date.now();
  const deployed = await execute(process.execPath, [compiledEntrypoint('src', 'references', 'reference-agent.js'),
    '--mode', 'build', '--backend', backend, '--track', track.name, '--level', '1',
    '--run-index', String(index), '--app', app], { timeout: Math.max(1, deadline - Date.now()),
    maxBuffer: 16 * 1024 ** 2, env: process.env });
  console.log(redactCredentials(deployed.stdout + deployed.stderr));
  const active = readBackendLease(path, { token: lease.ownershipToken, active: true });
  assert(active.resources.buildContainer?.id);
  const browser = await chromium.launch({ headless: true, ...attemptBrowserLaunchOptions() });
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(10_000);
    await page.goto(`http://127.0.0.1:${ports.vite}`, { timeout: 30_000 });
    const marker = `capacity_${index}_${basename(root).replaceAll('-', '_')}`;
    await page.locator('[data-role="signup-username"]').fill(marker);
    await page.locator('[data-role="signup-password"]').fill('capacity-check-password');
    await page.locator('[data-role="signup-submit"]').click();
    await page.locator('[data-role="current-user"]').filter({ hasText: marker }).waitFor();
    const proof = adapter.id === 'spacetime'
      ? adapter.database.proveUse({ lease: requireLeasedSpacetime(active), marker })
      : adapter.database.proveUse({ lease: requireLeasedDatabase(active), marker });
    assert(proof.ok && proof.verified, 'browser signup must persist in the exact native database');
    const stock = page.locator('[data-role="item-card"]').filter({ hasText: 'Desk Lamp' })
      .locator('[data-role="item-stock"]');
    assert.equal(Number(await stock.textContent()), 100);
    process.send!({ stage: 'ready', index, at: Date.now(), backend, marker,
      deploymentStarted, deploymentCompleted: Date.now() });
    await barrier(root, 'frontend-go', deadline);
    const metadata = json(join(app, 'reference.json')) as { client: { directory: string } };
    const buildScript = `const {spawn}=require('node:child_process');let started;
      const child=spawn('npm',['run','build'],{stdio:'inherit'});
      child.on('spawn',()=>{started=Date.now()});child.on('error',e=>{console.error(e);process.exitCode=1});
      child.on('close',code=>{console.log('CAPACITY_BUILD '+JSON.stringify({started,completed:Date.now(),code}));process.exitCode=code??1});`;
    const build = execute('docker', ['exec', ...codingContainerAgentExecOptions(),
      '-w', `/app/${metadata.client.directory}`, active.resources.buildContainer.id,
      ...codingContainerAgentCommand('node', ['-e', buildScript])],
    { timeout: Math.max(1, deadline + HOLD_MS - Date.now()), maxBuffer: 8 * 1024 ** 2 });
    // Handle an early build rejection while the browser loop is still running.
    void build.catch(() => {});
    const database = createDatabaseWriteCapability({ backend,
      databaseLease: backend === 'spacetime' ? null : requireLeasedDatabase(active),
      spacetime: adapter.grading.context({ requireBuildContainer: true }), expand: value => value });
    const liveStarted = Date.now(), latencies: number[] = [];
    while (Date.now() - liveStarted < HOLD_MS) {
      const started = Date.now(), quantity = latencies.length % 2 === 0 ? 54 : 55;
      await page.locator('[data-role="signout"]').click();
      await page.locator('[data-role="signup-username"]').fill(`${marker}_${latencies.length}`);
      await page.locator('[data-role="signup-password"]').fill('capacity-check-password');
      await page.locator('[data-role="signup-submit"]').click();
      await page.locator('[data-role="current-user"]').filter({ hasText: `${marker}_${latencies.length}` }).waitFor();
      database.setStock({ item: 'Desk Lamp', warehouse: 'East', quantity, settleMs: 0 });
      while (Number(await stock.textContent()) !== quantity + 45) {
        assert(Date.now() - started < 10_000, 'open storefront did not observe native stock write');
        await delay(100);
      }
      latencies.push(Date.now() - started);
      await delay(500);
    }
    const activityCompleted = Date.now();
    const built = await build;
    console.log(redactCredentials(built.stdout + built.stderr));
    const interval = JSON.parse(built.stdout.split('\n').find(line => line.startsWith('CAPACITY_BUILD '))!
      .slice('CAPACITY_BUILD '.length)) as { started: number; completed: number; code: number };
    assert.equal(interval.code, 0);
    assert(latencies.length > 0);
    process.send!({ stage: 'measured', index, at: Date.now(), frontendBuild: interval,
      liveStarted, activityCompleted, liveCompleted: Date.now(), stockWrites: latencies.length, latencyMs: latencies });
    await barrier(root, 'finish', deadline + HOLD_MS + CLEANUP_MS);
  } finally { await browser.close(); }
}

function cleanup(root: string, index: number): void {
  const path = leasePath(root, index);
  if (!existsSync(path)) return;
  const lease = readBackendLease(path);
  assert.equal(releaseBackendLease(path, lease.ownershipToken), true, 'exact cleanup must complete');
  const released = readBackendLease(path);
  assert.equal(released.state, 'released');
  rmSync(join(root, 'work', String(index)), { recursive: true, force: true });
}

interface ContainerSample {
  id: string; name: string; kind: string; cgroup: string;
  limits: Record<string, unknown>; initialCpu: Record<string, number>;
  initialMemoryEvents: Record<string, number>; last?: Record<string, unknown>;
}

async function capacityCheck(): Promise<void> {
  assert.equal(process.platform, 'linux');
  assert.equal(process.env.STACK_BENCH_APPLIANCE, '1');
  assert(!process.env.STACK_BENCH_LEASE && !process.env.STACK_BENCH_LEASE_TOKEN);
  assert.match(process.env.STACK_BENCH_CONTROLLER_IMAGE_ID ?? '', /^sha256:[a-f0-9]{64}$/);
  assert.match(process.env.STACK_BENCH_IMAGE ?? '', /^sha256:[a-f0-9]{64}$/);
  const state = process.env.STACK_BENCH_CAPACITY_STATE;
  assert(state && state.startsWith('/'), 'supply the native shared state mount path');
  const root = mkdtempSync(join(state, 'capacity-'));
  const deadline = Date.now() + STARTUP_MS;
  Object.assign(process.env, { STACK_BENCH_RESOURCE_LOCK_DIR: join(root, 'locks'),
    STACK_BENCH_CAPACITY_ROOT: root,
    STACK_BENCH_CAPACITY_DEADLINE: String(deadline) });
  const cache = docker(['inspect', '--format', '{{.Id}}', 'stack-bench-npm-cache']);
  const cacheMemberships = docker(['inspect', '--format', '{{json .NetworkSettings.Networks}}', cache]);
  const controller = docker(['inspect', '--format', '{{.Id}}', process.env.STACK_BENCH_CAPACITY_CONTROLLER_ID!]);
  const baselineContainers = docker(['ps', '--format', '{{.ID}} {{.Names}} {{.Status}}']);
  const baselineDisk = Number(docker(['exec', controller, 'du', '-sk', root]).split(/\s+/)[0]) * 1024;
  const containers = new Map<string, ContainerSample>();
  const events: Array<Record<string, unknown>> = [], samples: Array<Record<string, unknown>> = [];
  let sampling = true, failure: unknown, measurementFailure: unknown;
  const children: Array<{ child: ChildProcess; done: Promise<number | null> }> = [];
  const start = (mode: 'worker' | 'cleanup', index: number) => {
    const child = spawn(process.execPath, [self], { detached: true,
      stdio: ['ignore', 'pipe', 'pipe', 'ipc'], env: { ...process.env,
        STACK_BENCH_CAPACITY_MODE: mode, STACK_BENCH_CAPACITY_INDEX: String(index) } });
    let logged = 0;
    for (const stream of [child.stdout!, child.stderr!]) stream.on('data', chunk => {
      const text = redactCredentials(chunk.toString());
      if (logged < 2 * 1024 ** 2) appendFileSync(join(root, `${mode}-${index}.log`), text);
      logged += text.length;
    });
    child.on('message', value => { events.push(value as Record<string, unknown>); });
    return { child, done: new Promise<number | null>(resolveDone => {
      child.once('error', error => { failure ??= error; resolveDone(null); });
      child.once('close', resolveDone);
    }) };
  };
  const trackContainer = (id: string, kind: string, namespace?: string | null) => {
    if (containers.has(id)) return;
    const detail = JSON.parse(docker(['inspect', '--format',
      '{"id":{{json .Id}},"name":{{json .Name}},"pid":{{.State.Pid}},"limits":{{json .HostConfig}}}', id]));
    // Activation records the exact ID before start; sample its cgroup next tick.
    if (detail.pid === 0) return;
    const path = readFileSync(`/host/proc/${detail.pid}/cgroup`, 'utf8').trim().split('\n')
      .find(line => line.startsWith('0::'))?.slice(3);
    assert(path?.startsWith('/'), 'cgroup v2 host mount is required');
    const cgroup = resolve('/host/cgroup', `.${path}`);
    assert(cgroup.startsWith('/host/cgroup/'));
    if (kind !== 'controller' && kind !== 'cache') {
      const limits = kind === 'buildContainer' ? BUILD_CONTAINER_RESOURCE_LIMITS : SIDECAR_CONTAINER_RESOURCE_LIMITS;
      assert.equal(detail.limits.Memory, limits.memoryBytes);
      assert.equal(detail.limits.NanoCpus, limits.cpuCount * 1e9);
      assert.equal(detail.limits.PidsLimit, limits.pids);
      assert(detail.limits.CapDrop.includes('ALL'));
      if (kind !== 'container') assert.equal(detail.limits.NetworkMode, `container:${namespace}`);
    }
    containers.set(id, { id, kind, name: detail.name, cgroup,
      limits: { memoryBytes: detail.limits.Memory, memorySwapBytes: detail.limits.MemorySwap,
        nanoCpus: detail.limits.NanoCpus, pids: detail.limits.PidsLimit, capDrop: detail.limits.CapDrop,
        capAdd: detail.limits.CapAdd, networkMode: detail.limits.NetworkMode },
      initialCpu: numbers(readFileSync(join(cgroup, 'cpu.stat'), 'utf8')),
      initialMemoryEvents: numbers(readFileSync(join(cgroup, 'memory.events'), 'utf8')) });
  };
  const sample = () => {
    const mem = readFileSync('/host/proc/meminfo', 'utf8');
    const available = Number(mem.match(/^MemAvailable:\s+(\d+)/m)?.[1]) * 1024;
    const filesystem = statfsSync(root), freeDisk = filesystem.bavail * filesystem.bsize;
    for (let index = 0; index < WORKERS; index++) {
      const path = leasePath(root, index);
      if (!existsSync(path)) continue;
      const lease = readBackendLease(path);
      for (const kind of ['container', 'buildContainer', 'browserContainer'] as const) {
        const container = lease.resources[kind];
        if (container?.running !== false && container?.id) {
          trackContainer(container.id, kind, lease.resources.network?.namespaceContainerId);
        }
      }
    }
    let concurrentMemoryBytes = 0;
    for (const container of containers.values()) {
      const currentBytes = Number(readFileSync(join(container.cgroup, 'memory.current'), 'utf8'));
      concurrentMemoryBytes += currentBytes;
      const cpu = numbers(readFileSync(join(container.cgroup, 'cpu.stat'), 'utf8'));
      container.last = { currentBytes,
        peakBytes: Number(readFileSync(join(container.cgroup, 'memory.peak'), 'utf8')),
        peakScope: container.kind === 'cache' ? 'historical shared-cache lifetime, not trial peak' : 'container lifetime',
        events: numbers(readFileSync(join(container.cgroup, 'memory.events'), 'utf8')),
        cpu, cpuDelta: Object.fromEntries(Object.entries(cpu)
          .map(([key, value]) => [key, value - (container.initialCpu[key] ?? 0)])),
        cpuPressure: readFileSync(join(container.cgroup, 'cpu.pressure'), 'utf8') };
    }
    samples.push({ at: Date.now(), availableMemoryBytes: available, freeDiskBytes: freeDisk,
      concurrentMemoryBytes, containerCount: containers.size,
      cacheMemoryBytes: containers.get(cache)?.last?.currentBytes,
      totalMemoryBytes: Number(mem.match(/^MemTotal:\s+(\d+)/m)?.[1]) * 1024,
      swapTotalBytes: Number(mem.match(/^SwapTotal:\s+(\d+)/m)?.[1]) * 1024,
      swapFreeBytes: Number(mem.match(/^SwapFree:\s+(\d+)/m)?.[1]) * 1024,
      cpuPressure: readFileSync('/host/proc/pressure/cpu', 'utf8'),
      memoryPressure: readFileSync('/host/proc/pressure/memory', 'utf8') });
    assert(available >= RESERVE_MEMORY, 'diagnostic abort: Docker VM MemAvailable below 8 GiB');
    assert(freeDisk >= RESERVE_DISK, 'diagnostic abort: Docker VM free disk below 10 GiB');
  };
  let monitor: Promise<void> | undefined;
  try {
    trackContainer(controller, 'controller'); trackContainer(cache, 'cache'); sample();
    for (let index = 0; index < WORKERS; index++) children.push(start('worker', index));
    monitor = (async () => {
      while (sampling) {
        await delay(1000);
        if (!sampling) break;
        try { sample(); } catch (error) { measurementFailure = error; break; }
      }
    })();
    const until = async (stage: string, limit: number) => {
      while (events.filter(event => event.stage === stage).length < WORKERS) {
        if (measurementFailure) throw measurementFailure;
        assert(!failure, String(failure));
        assert(children.every(({ child }) => child.exitCode === null && child.signalCode === null),
          `worker exited before ${stage}; inspect retained logs`);
        assert(Date.now() < limit, `five-minute startup / bounded hold deadline exceeded at ${stage}`);
        await delay(50);
      }
      if (measurementFailure) throw measurementFailure;
      assert(Date.now() < limit, `deadline exceeded at ${stage}`);
    };
    await until('activated', deadline); write(join(root, 'deploy-go'), { at: Date.now() });
    await until('ready', deadline);
    const active = Array.from({ length: WORKERS }, (_, index) => readBackendLease(leasePath(root, index)));
    assert.equal(new Set(active.map(lease => lease.resources.network?.id)).size, WORKERS);
    for (const [index, lease] of active.entries()) {
      assert(lease.resources.locks.some(lock => lock.key === `capacity:runner:${index}`));
    }
    write(join(root, 'frontend-go'), { at: Date.now() });
    await until('measured', deadline + HOLD_MS);
    sample();
    const measured = events.filter(event => event.stage === 'measured') as Array<{
      frontendBuild: { started: number; completed: number }; liveStarted: number; activityCompleted: number; liveCompleted: number;
    }>;
    const overlap = Math.min(...measured.map(value => value.frontendBuild.completed))
      - Math.max(...measured.map(value => value.frontendBuild.started));
    assert(overlap > 0, 'all nine real frontend builds must overlap');
    const allNineLiveMs = Math.min(...measured.map(value => value.liveCompleted))
      - Math.max(...events.filter(value => value.stage === 'ready').map(value => Number(value.at)));
    assert(allNineLiveMs >= HOLD_MS, 'all nine storefront/browser sessions must remain live for 20 seconds');
    const allNineActivityMs = Math.min(...measured.map(value => value.activityCompleted))
      - Math.max(...measured.map(value => value.liveStarted));
    assert(allNineActivityMs > 0, 'all nine browser activity loops must overlap');
    events.push({ stage: 'frontend-overlap', overlapMs: overlap, allNineLiveMs, allNineActivityMs });
  } catch (error) { failure ??= error; }
  finally {
    sampling = false; await monitor;
    // Capture kernel peaks while cgroups still exist. Stop all controller-side
    // writers before the shared exact-lease owner performs container handback.
    try { sample(); } catch (error) { failure ??= error; }
    for (const container of containers.values()) {
      try {
        const state = JSON.parse(docker(['inspect', '--format', '{{json .State}}', container.id]));
        (container.last ??= {}).oomKilled = state.OOMKilled;
        assert.equal(state.OOMKilled, false);
        if (!['controller', 'cache'].includes(container.kind)) {
          assert.equal((container.last.events as Record<string, number>).oom_kill, 0);
        }
      } catch (error) { failure ??= error; }
    }
    try { write(join(root, 'finish'), { at: Date.now() }); } catch (error) { failure ??= error; }
    if (failure) for (const { child } of children) {
      if (child.pid && child.exitCode === null && child.signalCode === null) {
        try { process.kill(-child.pid, 'SIGKILL'); } catch { /* already exited */ }
      }
    }
    await Promise.race([Promise.all(children.map(value => value.done)), delay(5000, undefined, { ref: false })]);
    for (const { child } of children) if (child.pid) {
      try { process.kill(-child.pid, 'SIGKILL'); } catch { /* process group ended */ }
    }
    let diskBeforeCleanup: number | null = null;
    try { diskBeforeCleanup = Number(docker(['exec', controller, 'du', '-sk', root]).split(/\s+/)[0]) * 1024; }
    catch (error) { failure ??= error; }
    const cleaning = Array.from({ length: WORKERS }, (_, index) => start('cleanup', index));
    const cleanCodes = await Promise.race([Promise.all(cleaning.map(value => value.done)),
      delay(CLEANUP_MS, undefined, { ref: false }).then(() => null)]);
    if (!cleanCodes) for (const { child } of cleaning) if (child.pid) {
      try { process.kill(-child.pid, 'SIGKILL'); } catch { /* already exited */ }
    }
    try {
      assert(cleanCodes?.every(code => code === 0), 'cleanup incomplete; private lease authority retained');
      assert.equal(docker(['inspect', '--format', '{{.Id}}', 'stack-bench-npm-cache']), cache);
      assert.deepEqual(JSON.parse(docker(['inspect', '--format', '{{json .NetworkSettings.Networks}}', cache])),
        JSON.parse(cacheMemberships));
      assert.equal(readdirSync(join(root, 'locks')).filter(name => name.endsWith('.lock.json')).length, 0);
      for (const container of containers.values()) if (!['controller', 'cache'].includes(container.kind)) {
        assert.throws(() => docker(['inspect', container.id]), /No such (object|container)/i);
      }
      for (let index = 0; index < WORKERS; index++) {
        const path = leasePath(root, index);
        if (!existsSync(path)) continue;
        const released = readBackendLease(path);
        assert.equal(released.state, 'released');
        if (released.resources.network) assert.throws(() => docker(['network', 'inspect', released.resources.network!.id]),
          dockerNetworkMissing);
        assert(!existsSync(appPath(root, index)));
      }
    } catch (error) { failure ??= error; }
    write(join(root, 'capacity.json'), { passed: !failure, workers: WORKERS, backendMix: BACKENDS,
      interpretation: 'Direct owned-lifecycle diagnostic; synchronized frontend builds, native databases and browsers. Warm existing shared cache with its unchanged actual caps. Not normal-preflight, paid-agent, broker-load or full-grade proof. Shared-cache lifetime memory.peak is not a trial peak; concurrent peak comes from time-aligned memory.current samples.',
      diagnosticAbortThresholds: { availableMemoryBytes: RESERVE_MEMORY, freeDiskBytes: RESERVE_DISK },
      controllerImage: process.env.STACK_BENCH_CONTROLLER_IMAGE_ID, codingImage: process.env.STACK_BENCH_IMAGE,
      baselineContainers, events, samples, containers: [...containers.values()],
      observedConcurrentMemoryPeakBytes: Math.max(...samples.map(value => Number(value.concurrentMemoryBytes))),
      observedCacheMemoryPeakBytes: Math.max(...samples.map(value => Number(value.cacheMemoryBytes))),
      ownedStateDiskGrowthBytes: diskBeforeCleanup === null ? null : diskBeforeCleanup - baselineDisk,
      cleanupExitCodes: cleanCodes, error: failure ? redactCredentials(String(failure)) : null });
    console.log(`capacity evidence: ${join(root, 'capacity.json')}`);
  }
  if (failure) throw failure;
}

if (process.env.STACK_BENCH_CAPACITY_MODE) {
  const root = process.env.STACK_BENCH_CAPACITY_ROOT!, index = Number(process.env.STACK_BENCH_CAPACITY_INDEX);
  try {
    if (process.env.STACK_BENCH_CAPACITY_MODE === 'cleanup') cleanup(root, index);
    else await worker(root, index);
  } catch (error) { console.error(redactCredentials(error instanceof Error ? error.stack : error)); process.exitCode = 1; }
  finally { process.disconnect?.(); }
} else {
  test('nine owned reference workloads overlap within measured Docker VM capacity', {
    skip: process.env.STACK_BENCH_CAPACITY_TEST !== '1', timeout: STARTUP_MS + HOLD_MS + CLEANUP_MS + 30_000,
  }, capacityCheck);
}
