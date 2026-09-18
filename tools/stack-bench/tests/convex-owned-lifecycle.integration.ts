import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, mkdtempSync, readFileSync, readdirSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { backendResourceLockKeys, claimBackendResources, createBackendLease, publicBackendLease,
  readBackendLease, resourceLockScope } from '../src/runtime/backend-lease.js';
import { attemptDocker, requireAttemptNetwork } from '../src/runtime/docker-network.js';
import { activateConvex, controlConvex, releaseConvex, CONVEX_PROCESS_RECORD } from '../src/stacks/backends/convex-lifecycle.js';

// Run inside the Linux controller with its normal lock directory and Docker socket.
// The fixture and evidence directory are explicit mounts; no paid calls or registry entry.
test('private Convex owned launch, warm restart, full reset, and exact cleanup', {
  skip: process.env.STACK_BENCH_CONVEX_OWNED_TEST !== '1', timeout: 240_000,
}, async () => {
  assert.equal(process.platform, 'linux');
  const fixture = process.env.STACK_BENCH_CONVEX_FIXTURE!;
  const evidenceDirectory = process.env.STACK_BENCH_CONVEX_EVIDENCE_DIR!;
  assert(fixture && evidenceDirectory);
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-convex-owned-'));
  const path = join(root, 'lease.json');
  const ports = { vite: 14309, express: 14311, dbPort: null };
  const lease = createBackendLease({ runId: root.split('/').at(-1)!, backend: 'convex',
    track: 'ecommerce', runIndex: 0, serverUri: 'http://127.0.0.1:14310' });
  const evidence: Record<string, unknown> = { result: 'running', ports,
    note: 'Direct private lifecycle; explicit claimed test ports, not campaign allocation.' };
  const save = () => writeFileSync(join(evidenceDirectory, 'owned-lifecycle.json'), JSON.stringify(evidence, null, 2));
  save();
  let active;
  let volumes: string[] = [];
  let failure: unknown;
  try {
    claimBackendResources(path, lease, { ...resourceLockScope(), keys: backendResourceLockKeys(lease, ports) });
    activateConvex({ leasePath: path, leaseToken: lease.ownershipToken, ports });
    active = readBackendLease(path, { token: lease.ownershipToken, active: true });
    evidence.activeLease = publicBackendLease(active);
    requireAttemptNetwork(active);
    const backend = active.resources.container!;
    const bindings = attemptDocker(['inspect', '--format', '{{json .HostConfig.PortBindings}}', backend.id]);
    volumes = JSON.parse(attemptDocker(['inspect', '--format', '{{json .Mounts}}', backend.id]))
      .filter((mount: { Type: string }) => mount.Type === 'volume').map((mount: { Name: string }) => mount.Name);
    assert.equal(volumes.length, 1, 'The backend data is one container-owned anonymous volume');
    assert.equal((await fetch(`${active.resources.serverUri}/version`)).status, 200);
    const key = attemptDocker(['exec', backend.id, 'bash', './generate_admin_key.sh']);
    assert(key.includes('|'), 'Native admin key format');
    const environment = { CONVEX_SELF_HOSTED_URL: active.resources.serverUri!, CONVEX_SELF_HOSTED_ADMIN_KEY: key };
    // Deploy trusted fixture code from the controller. The browser's /tmp is
    // intentionally noexec and must not be weakened to run the Convex compiler.
    const workspace = join(root, 'fixture');
    cpSync(fixture, workspace, { recursive: true });
    const sourceHashes = () => Object.fromEntries(readdirSync(join(workspace, 'convex')).filter(name => name.endsWith('.js'))
      .sort().map(name => [name, createHash('sha256').update(readFileSync(join(workspace, 'convex', name))).digest('hex')]));
    const run = (command: string, args: string[]) => execFileSync(command, args, {
      cwd: workspace, env: { ...process.env, ...environment }, encoding: 'utf8', stdio: 'pipe', timeout: 120_000 });
    const cache = active.resources.network!.services[0]!;
    run('npm', ['ci', '--ignore-scripts', '--no-audit', '--no-fund', '--registry', `http://${cache.address}:${cache.port}`]);
    run(process.execPath, ['prepare-auth.mjs']);
    run(process.execPath, ['node_modules/convex/bin/main.js', 'dev', '--once', '--typecheck', 'disable']);
    const deployedSource = sourceHashes();
    const browser = active.resources.browserContainer!.id;
    attemptDocker(['exec', browser, 'mkdir', '-p', '/tmp/convex-owned']);
    const archive = execFileSync('tar', ['-C', workspace, '-cf', '-', '.'], { maxBuffer: 128 * 1024 * 1024 });
    execFileSync('docker', ['exec', '-i', browser, 'tar', '-C', '/tmp/convex-owned', '-xf', '-'], { input: archive });
    attemptDocker(['exec', '-i', browser, 'sh', '-c', 'umask 077; cat > /tmp/convex-owned/owned-private.json'], JSON.stringify(environment));
    const probeEvidence = (phase: string) => JSON.parse(attemptDocker(['exec', browser, 'cat',
      `/tmp/convex-owned/owned-probe${phase === 'initial' ? '' : `-${phase}`}.json`]));
    const probe = (phase: string) => {
      execFileSync('docker', ['exec', '-w', '/tmp/convex-owned', browser, 'node', 'probe-owned.mjs', phase],
        { encoding: 'utf8', stdio: 'pipe', timeout: 60_000 });
      return probeEvidence(phase);
    };
    evidence.native = probe('initial');
    const jwks = await fetch(`http://127.0.0.1:${ports.express}/.well-known/jwks.json`);
    assert.equal(jwks.status, 200);
    assert.equal((await jwks.json() as { keys: unknown[] }).keys.length, 1);
    assert.throws(() => releaseConvex(path, 'wrong-token'), /ownership token does not match/);
    assert.equal(attemptDocker(['inspect', '--format', '{{.State.Running}}', backend.id]), 'true');
    const processRecord = attemptDocker(['exec', backend.id, 'cat', CONVEX_PROCESS_RECORD]);
    const [pid, started] = processRecord.split(' ');
    const writeRecord = (value: string) => attemptDocker(['exec', '-i', backend.id, 'sh', '-c',
      `cat > ${CONVEX_PROCESS_RECORD}`], value);
    try {
      for (const record of ['', `${pid} ${BigInt(started!) + 1n}\n`]) {
        writeRecord(record);
        await assert.rejects(controlConvex({ leasePath: path, leaseToken: lease.ownershipToken, ports, mode: 'reset' }),
          /Command failed|Convex process identity changed/);
        assert.equal((await fetch(`${active.resources.serverUri}/version`)).status, 200);
      }
    } finally { writeRecord(`${processRecord}\n`); }
    evidence.resetGuard = 'empty and stale process records refused without stopping the backend';
    await controlConvex({ leasePath: path, leaseToken: lease.ownershipToken, ports, mode: 'restart' });
    evidence.warm = probe('warm');
    evidence.pending = probe('prepare-reset');
    assert.throws(() => probe('reset'));
    const retainedStateControl = probeEvidence('reset');
    evidence.retainedStateControl = retainedStateControl;
    assert.equal(retainedStateControl.result, 'failed');
    assert.match(retainedStateControl.error, /Reset requires empty stored state/);
    await controlConvex({ leasePath: path, leaseToken: lease.ownershipToken, ports, mode: 'reset' });
    environment.CONVEX_SELF_HOSTED_ADMIN_KEY = attemptDocker(['exec', backend.id, 'bash', './generate_admin_key.sh']);
    run(process.execPath, ['prepare-auth.mjs']);
    run(process.execPath, ['node_modules/convex/bin/main.js', 'dev', '--once', '--typecheck', 'disable']);
    assert.deepEqual(sourceHashes(), deployedSource, 'Reset must redeploy identical application source');
    evidence.deployedSource = deployedSource;
    attemptDocker(['exec', '-i', browser, 'sh', '-c', 'umask 077; cat > /tmp/convex-owned/owned-private.json'], JSON.stringify(environment));
    evidence.reset = probe('reset');
    assert.equal(attemptDocker(['inspect', '--format', '{{json .HostConfig.PortBindings}}', backend.id]), bindings);
    assert.deepEqual(readBackendLease(path).resources.network, active.resources.network);
    requireAttemptNetwork(readBackendLease(path));
    evidence.namespaceAndPortsPreserved = true;
    evidence.result = 'passed';
  } catch (error) {
    evidence.result = 'failed';
    evidence.error = error instanceof Error ? error.message : String(error);
    if (active?.resources.browserContainer) {
      for (const [key, suffix] of [['native', ''], ['warm', '-warm'], ['pending', '-prepare-reset'], ['reset', '-reset']]) {
        try { evidence[key!] = JSON.parse(attemptDocker(['exec', active.resources.browserContainer.id, 'cat',
          `/tmp/convex-owned/owned-probe${suffix}.json`])); } catch { /* Before this probe starts. */ }
      }
    }
    failure = error;
  } finally {
    try {
      if (existsSync(path)) {
        assert.equal(releaseConvex(path, lease.ownershipToken), true, 'Exact owned cleanup must succeed');
        const final = readBackendLease(path);
        evidence.finalLease = publicBackendLease(final);
        assert.equal(final.state, 'released');
        for (const volume of volumes) assert.throws(() => attemptDocker(['volume', 'inspect', volume]), /no such volume/i);
        evidence.anonymousVolumeCleanup = volumes.length;
      }
    } catch (error) {
      evidence.result = 'failed';
      evidence.cleanupError = error instanceof Error ? error.message : String(error);
      failure ??= error;
    } finally { save(); }
  }
  if (failure) throw failure;
});
