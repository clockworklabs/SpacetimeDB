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
  releaseResourceLocks, verifyResourceLocks, writeBackendLease } from '../src/runtime/backend-lease.js';

const linux = { skip: process.platform !== 'linux' ? 'Kernel flock requires Linux' : false };

test('separate campaign processes compete before either enters resource preflight', linux, async () => {
  const root = mkdtempSync(join(tmpdir(), 'campaign-admission-race-'));
  const helper = compiledEntrypoint('tests', 'fixtures', 'campaign-admission-process.js');
  const directories = ['first', 'second'].map(name => join(root, name));
  for (const directory of directories) mkdirSync(directory);
  const children = directories.map(directory => fork(helper, [directory, join(root, 'locks')],
    { stdio: ['ignore', 'ignore', 'pipe', 'ipc'] }));
  try {
    await Promise.all(children.map(child => once(child, 'message', { signal: AbortSignal.timeout(15000) })));
    const results = children.map(child => once(child, 'message', { signal: AbortSignal.timeout(15000) }));
    children.forEach(child => child.send('start'));
    const outcomes = (await Promise.all(results)).map(([message]) => message).sort();
    assert.deepEqual(outcomes, ['admitted', 'refused']);
    assert.equal(directories.filter(directory => existsSync(join(directory, 'preflight-entered'))).length, 1);
  } finally {
    await Promise.all(children.map(async child => {
      if (child.exitCode === null) { const exited = once(child, 'exit'); child.kill('SIGKILL'); await exited; }
    }));
    rmSync(root, { recursive: true, force: true });
  }
});

test('one-use child delegation preserves parent capacity across successive attempts', linux, () => {
  const root = mkdtempSync(join(tmpdir(), 'campaign-borrow-'));
  try {
    const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign.deterministic.json'));
    const admitted = runCampaignAdmission(plan, root, {
      env: { STACK_BENCH_RESOURCE_LOCK_DIR: join(root, 'locks') },
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


for (const mode of ['campaign', 'standalone']) test(
  `independent ${mode} processes race for a shared two-worker pool and reuse only released slots`, linux, async () => {
  const root = mkdtempSync(join(tmpdir(), 'campaign-parallel-admission-'));
  const locks = join(root, 'locks');
  const helper = compiledEntrypoint('tests', 'fixtures', 'campaign-admission-process.js');
  const directories = ['first', 'second', 'third'].map(name => join(root, name));
  for (const directory of directories) mkdirSync(directory);
  const children = directories.map((directory, index) => fork(helper,
    [directory, locks, '2', '3', ...(mode === 'standalone' ? [String(index)] : [])],
    { stdio: ['ignore', 'ignore', 'pipe', 'ipc'] }));
  type Admitted = { status: 'admitted'; runIndices: number[]; keys: string[] };
  try {
    await Promise.all(children.map(child => once(child, 'message', { signal: AbortSignal.timeout(15000) })));
    const results = children.map(child => once(child, 'message', { signal: AbortSignal.timeout(15000) }));
    children.forEach(child => child.send('start'));
    const outcomes = (await Promise.all(results)).map(([message]) => message as Admitted | 'refused');
    const admitted = outcomes.filter((result): result is Admitted => result !== 'refused');
    assert.equal(admitted.length, 2, directories.map(directory => {
      const path = join(directory, 'admission-error');
      return existsSync(path) ? readFileSync(path, 'utf8') : 'admitted';
    }).join(' | '));
    assert.equal(outcomes.filter(result => result === 'refused').length, 1);
    assert.equal(directories.filter(directory => existsSync(join(directory, 'preflight-entered'))).length,
      mode === 'campaign' ? 2 : 0);
    assert.equal(admitted[0]!.keys.filter(key => admitted[1]!.keys.includes(key)).length, 0);
    assert.notDeepEqual(admitted[0]!.runIndices, admitted[1]!.runIndices);
    assert.deepEqual(admitted.flatMap(result => result.keys.filter(key => key.startsWith('capacity:'))).sort(),
      ['capacity:runner:0', 'capacity:runner:1']);
    const releasedIndex = outcomes.indexOf(admitted[0]!);
    const released = once(children[releasedIndex]!, 'message', { signal: AbortSignal.timeout(15000) });
    children[releasedIndex]!.send('release');
    assert.equal((await released)[0], 'released');
    const waitingIndex = outcomes.indexOf('refused');
    if (mode === 'standalone') {
      const conflicting = once(children[waitingIndex]!, 'message', { signal: AbortSignal.timeout(15000) });
      children[waitingIndex]!.send({ port: 20000 + outcomes.indexOf(admitted[1]!) });
      assert.equal((await conflicting)[0], 'refused');
      assert.match(readFileSync(join(directories[waitingIndex]!, 'admission-error'), 'utf8'), /port:.*already leased/);
      assert.equal(readdirSync(locks).filter(name => name.endsWith('.lock.json')).length, 2,
        'a fixed-port conflict must leave the free capacity slot unclaimed');
    }
    const reused = once(children[waitingIndex]!, 'message', { signal: AbortSignal.timeout(15000) });
    children[waitingIndex]!.send('start');
    const [replacement] = await reused as [Admitted];
    assert.equal(replacement.status, 'admitted');
    const capacityKeys = (keys: string[]) => keys.filter(key => key.startsWith('capacity:'));
    assert.deepEqual(capacityKeys(replacement.keys), capacityKeys(admitted[0]!.keys));
    if (mode === 'campaign') assert.deepEqual(replacement.keys, admitted[0]!.keys);
    else assert(replacement.keys.includes(`port:${20000 + waitingIndex}`));
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


test('admission cannot reclaim an owner that dies between free-capacity selection and claim', linux, () => {
  const root = mkdtempSync(join(tmpdir(), 'campaign-dead-owner-race-'));
  try {
    const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign.deterministic.json'));
    const locks = join(root, 'locks');
    const stale = createBackendLease({ runId: 'crashed-owner', backend: 'stub', track: plan.definition.track, runIndex: 0 });
    stale.ownerPid = 2147483647;
    let published = false;
    assert.throws(() => runCampaignAdmission(plan, root, {
      env: { STACK_BENCH_RESOURCE_LOCK_DIR: locks, STACK_BENCH_RUNNER_CAPACITY: '1' },
      probePort: () => {
        if (!published) {
          published = true;
          claimBackendResources(join(root, 'crashed.json'), stale,
            { root: locks, keys: ['capacity:runner:0'] });
        }
        return { free: true };
      },
      preflight: () => { throw new Error('must not enter preflight'); },
    }), /required runner slots are free/);
    verifyResourceLocks(stale);
    assert.equal(readBackendLease(join(root, 'crashed.json')).state, stale.state);
    releaseResourceLocks(stale);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
