import assert from 'node:assert/strict';
import { fork } from 'node:child_process';
import { once } from 'node:events';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { compiledEntrypoint, STACK_BENCH_ROOT } from '../src/package-root.js';
import { loadTrack, portsFor } from '../src/composition/tracks.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { borrowCampaignReservation, closeCampaignDelegation, delegateCampaignReservation,
  releaseCampaignReservation, runCampaignAdmission } from '../src/campaigns/campaign-admission.js';
import { backendResourceLockKeys, claimBackendResources, createBackendLease, readBackendLease,
  releaseResourceLocks, runResourceLockKeys, verifyResourceLocks, writeBackendLease } from '../src/runtime/backend-lease.js';

const linux = { skip: process.platform !== 'linux' ? 'Kernel flock requires Linux' : false };

test('one-use child delegation preserves parent reservations across successive attempts', linux, async () => {
  const root = mkdtempSync(join(tmpdir(), 'campaign-borrow-'));
  try {
    const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign.deterministic.json'));
    const admitted = await runCampaignAdmission(plan, root, {
      env: { STACK_BENCH_RUNNER_CAPACITY: '64', STACK_BENCH_RESOURCE_LOCK_DIR: join(root, 'locks') },
      probePort: () => ({ free: true }),
      preflight: request => ({ schemaVersion: 1, generatedAt: new Date().toISOString(),
        request: { backends: request.backends, track: request.track, levels: request.levelList,
          runIndex: request.runIndex, parallelism: request.parallelism, agentAdapter: request.agentAdapter,
          packs: request.packIds, checks: request.checkKeys, image: request.image,
          resultsDir: request.resultsDir, smoke: request.smoke },
        ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] }),
    });
    assert(admitted.reservation);
    const parent = readBackendLease(admitted.reservation.path, { token: admitted.reservation.token });
    for (const executionId of ['execution-one', 'execution-two']) {
      const output = join(root, executionId);
      const env = delegateCampaignReservation(admitted.reservation, root, {
        campaignSha256: plan.contentSha256, admissionId: admitted.id, executionId, output,
        backend: 'postgres', runIndex: admitted.runIndices[0]!,
      });
      const lease = createBackendLease({ runId: executionId, backend: 'postgres',
        track: plan.definition.track, runIndex: admitted.runIndices[0]!, database: 'test',
        container: { name: 'unused', id: 'unused' } });
      const leasePath = join(root, executionId, 'lease.json');
      const input = { env, campaignSha256: plan.contentSha256, admissionId: admitted.id,
        executionId, output, leasePath, lease, keys: backendResourceLockKeys(lease, portsFor(loadTrack(lease.track), lease.backend, lease.runIndex)) };
      assert.throws(() => borrowCampaignReservation({ ...input, output: join(root, 'wrong') }),
        /identity does not match/);
      assert.equal(borrowCampaignReservation(input), true);
      assert.throws(() => borrowCampaignReservation(input), /already consumed/);
      assert.throws(() => releaseCampaignReservation(admitted.reservation!), /unresolved child/);
      releaseResourceLocks(lease);
      verifyResourceLocks(parent);
      lease.state = 'released';
      writeBackendLease(leasePath, lease);
      closeCampaignDelegation(root, executionId);
      verifyResourceLocks(parent);
    }
    releaseCampaignReservation(admitted.reservation);
    assert.throws(() => verifyResourceLocks(parent), /no longer belongs/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});


test('three concurrent nine-worker campaigns claim 27 disjoint workers within host capacity', linux, async () => {
  const root = mkdtempSync(join(tmpdir(), 'campaign-dynamic-admission-'));
  const locks = join(root, 'locks');
  const helper = compiledEntrypoint('tests', 'fixtures', 'campaign-admission-process.js');
  const directories = ['first', 'second', 'third'].map(name => join(root, name));
  for (const directory of directories) mkdirSync(directory);
  const children = directories.map(directory => fork(helper, [directory, locks, '9', '3'],
    { stdio: ['ignore', 'ignore', 'pipe', 'ipc'] }));
  type Admitted = { status: 'admitted'; runIndices: number[]; keys: string[] };
  try {
    await Promise.all(children.map(child => once(child, 'message', { signal: AbortSignal.timeout(30000) })));
    const results = children.map(child => once(child, 'message', { signal: AbortSignal.timeout(30000) }));
    children.forEach(child => child.send('start'));
    const outcomes = (await Promise.all(results)).map(([message]) => message as Admitted);
    assert(outcomes.every(result => result.status === 'admitted'), directories.map(directory => {
      const path = join(directory, 'admission-error');
      return existsSync(path) ? readFileSync(path, 'utf8') : 'admitted';
    }).join(' | '));
    assert(outcomes.every(result => result.runIndices.length === 9));
    assert.equal(new Set(outcomes.flatMap(result => result.runIndices)).size, 27);
    const keys = outcomes.flatMap(result => result.keys);
    assert.equal(new Set(keys).size, keys.length, 'campaigns cannot share host ports or run slots');
    assert(keys.every(key => !key.startsWith('capacity:')));
    assert.equal(directories.filter(directory => existsSync(join(directory, 'preflight-entered'))).length, 3);
    const released = once(children[0]!, 'message', { signal: AbortSignal.timeout(15000) });
    children[0]!.send('release');
    assert.equal((await released)[0], 'released');
    const reused = once(children[0]!, 'message', { signal: AbortSignal.timeout(30000) });
    children[0]!.send('start');
    const [replacement] = await reused as [Admitted];
    assert.equal(replacement.status, 'admitted');
    assert.deepEqual(replacement.runIndices, outcomes[0]!.runIndices);
    const releases = children.map(child => once(child, 'message', { signal: AbortSignal.timeout(15000) }));
    children.forEach(child => child.send('release'));
    assert((await Promise.all(releases)).every(([message]) => message === 'released'));
    assert.equal(readdirSync(locks).filter(name => name.endsWith('.lock.json')).length, 0);
  } finally {
    await Promise.all(children.map(async child => {
      if (child.exitCode === null) { const exited = once(child, 'exit'); child.kill('SIGKILL'); await exited; }
    }));
    rmSync(root, { recursive: true, force: true });
  }
});

test('dynamic admission skips live legacy capacity and port reservations without reclaiming them', linux, async () => {
  const root = mkdtempSync(join(tmpdir(), 'campaign-legacy-reservation-'));
  const locks = join(root, 'locks');
  const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign.deterministic.json'));
  const track = loadTrack(plan.definition.track);
  const legacy = createBackendLease({ runId: 'live-legacy', backend: 'stub', track: track.name, runIndex: 0 });
  try {
    const keys = Array.from({ length: 9 }, (_, index) => [
      `capacity:runner:${index}`,
      ...plan.stacks.flatMap(stack => runResourceLockKeys({
        backend: stack.id, track: track.name, runIndex: index,
        serverUri: stack.id === 'spacetime' ? `http://127.0.0.1:${3210 + index}` : null,
        ports: portsFor(track, stack.id, index) })),
    ]).flat();
    claimBackendResources(join(root, 'legacy.json'), legacy, { root: locks, keys });
    const before = legacy.resources.locks.map(lock => readFileSync(lock.path, 'utf8'));
    const admitted = await runCampaignAdmission(plan, root, {
      env: { STACK_BENCH_RUNNER_CAPACITY: '64', STACK_BENCH_RESOURCE_LOCK_DIR: locks }, probePort: () => ({ free: true }),
      preflight: request => ({ schemaVersion: 1, generatedAt: new Date().toISOString(),
        request: { backends: request.backends, track: request.track, levels: request.levelList,
          runIndex: request.runIndex, parallelism: request.parallelism, agentAdapter: request.agentAdapter,
          packs: request.packIds, checks: request.checkKeys, image: request.image,
          resultsDir: request.resultsDir, smoke: request.smoke },
        ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] }),
    });
    assert.deepEqual(admitted.runIndices, [9]);
    assert.deepEqual(legacy.resources.locks.map(lock => readFileSync(lock.path, 'utf8')), before);
    releaseCampaignReservation(admitted.reservation!);
    verifyResourceLocks(legacy);
    releaseResourceLocks(legacy);
  } finally { rmSync(root, { recursive: true, force: true }); }
});


test('attempt admissions reserve only their stack and release slots for another campaign', linux, async () => {
  const root = mkdtempSync(join(tmpdir(), 'campaign-attempt-reservation-'));
  const locks = join(root, 'locks');
  const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign.deterministic.json'));
  const pg = plan.attempts.find(attempt => attempt.stack === 'postgres')!;
  const mongo = plan.attempts.find(attempt => attempt.stack === 'mongodb')!;
  const reservations: NonNullable<Awaited<ReturnType<typeof runCampaignAdmission>>['reservation']>[] = [];
  const admit = async (attempt: typeof pg, directory: string) => {
    const result = await runCampaignAdmission(plan, join(root, directory), {
      attempt, env: { STACK_BENCH_RUNNER_CAPACITY: '64', STACK_BENCH_RESOURCE_LOCK_DIR: locks }, probePort: () => ({ free: true }),
      preflight: request => {
        assert.deepEqual(request.backends, [attempt.stack]);
        assert.deepEqual(request.agentSkills, attempt.skills.slice().sort());
        assert.equal(request.parallelism, plan.summary.parallelism);
        return { schemaVersion: 1, generatedAt: new Date().toISOString(),
          request: { backends: request.backends, track: request.track, levels: request.levelList,
            runIndex: request.runIndex, parallelism: request.parallelism, agentAdapter: request.agentAdapter,
            packs: request.packIds, checks: request.checkKeys, image: request.image,
            resultsDir: request.resultsDir, smoke: request.smoke },
          ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] };
      },
    });
    reservations.push(result.reservation!);
    assert.equal(result.payload.attemptId, attempt.id);
    return result;
  };
  try {
    const first = await admit(pg, 'first');
    const otherStack = await admit(mongo, 'second');
    assert.deepEqual(first.runIndices, [0]);
    assert.deepEqual(otherStack.runIndices, [0]);
    const otherPg = await admit(pg, 'second');
    assert.deepEqual(otherPg.runIndices, [1]);
    const live = readBackendLease(otherPg.reservation!.path, { token: otherPg.reservation!.token });
    releaseCampaignReservation(first.reservation!);
    reservations.splice(reservations.indexOf(first.reservation!), 1);
    assert.deepEqual((await admit(pg, 'first')).runIndices, [0]);
    verifyResourceLocks(live);
  } finally {
    for (const reservation of reservations) releaseCampaignReservation(reservation);
    rmSync(root, { recursive: true, force: true });
  }
});
