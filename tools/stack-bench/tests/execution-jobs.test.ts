import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { cancelExecutionJob, listExecutionJobs, readExecutionJob, submitExecutionJob,
  workExecutionJob } from '../src/campaigns/execution-jobs.js';
import type { executeCampaign } from '../src/campaigns/campaign-runner.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('job submission, exclusive workers, cancellation and retained failures use durable records', async () => {
  const root = mkdtempSync(join(tmpdir(), 'execution-jobs-'));
  try {
    mkdirSync(join(root, 'plans'));
    copyFileSync(join(STACK_BENCH_ROOT, 'tests/fixtures/campaign.deterministic.json'), join(root, 'plans/test.json'));
    const request = { key: 'request-a', planFile: 'test.json', hostId: 'worker-a', credentials: {} };
    const job = submitExecutionJob(root, request);
    assert.deepEqual(submitExecutionJob(root, request), job, 'retry returns identical submission');
    assert.throws(() => submitExecutionJob(root, { ...request, hostId: 'worker-b' }), /different submission/);
    assert.throws(() => submitExecutionJob(root, { ...request, key: '../escape' }));
    assert.equal(listExecutionJobs(root).jobs.length, 1);
    await assert.rejects(workExecutionJob(root, job.id, 'worker-b'), /another host/);
    let finish!: () => void;
    const running = new Promise<void>(resolve => { finish = resolve; });
    let invocations = 0;
    const execute = (async (_plan, _output, options) => {
      invocations++;
      assert.equal(options?.mode, 'model-free-trial');
      assert.equal(options?.capacityPolicy, 'wait');
      assert.deepEqual(options?.executionCredentials, {});
      await running;
      return { summary: { completed: 9, running: 0, pending: 0 } };
    }) as typeof executeCampaign;
    const first = workExecutionJob(root, job.id, 'worker-a', { execute });
    assert.equal((await workExecutionJob(root, job.id, 'worker-a', { execute })).status, 'running');
    finish();
    assert.equal((await first).status, 'completed');
    await workExecutionJob(root, job.id, 'worker-a', { execute });
    assert.equal(invocations, 1, 'duplicate claims never invoke the runner twice');

    const cancelled = submitExecutionJob(root, { ...request, key: 'request-b' });
    cancelExecutionJob(root, cancelled.id);
    assert.equal((await workExecutionJob(root, cancelled.id, 'worker-a', { execute })).status, 'cancelled');
    assert.equal(invocations, 1);

    const failed = submitExecutionJob(root, { ...request, key: 'request-c' });
    const failure = (async () => { throw new Error('owned cleanup is incomplete'); }) as typeof executeCampaign;
    const outcome = await workExecutionJob(root, failed.id, 'worker-a', { execute: failure });
    assert.equal(outcome.status, 'failed');
    assert.match(outcome.error!, /cleanup/);
    assert.equal((await workExecutionJob(root, failed.id, 'worker-a', { execute })).status, 'failed');
    assert.equal(invocations, 1, 'failed jobs require explicit recovery; no implicit retry');
    const invalid = submitExecutionJob(root, { ...request, key: 'request-invalid' });
    const invalidResult = (async () => ({ summary: { pending: 0, running: 0, completed: 0, invalid: 1 } })) as unknown as typeof executeCampaign;
    assert.equal((await workExecutionJob(root, invalid.id, 'worker-a', { execute: invalidResult })).status, 'failed');
    const page = listExecutionJobs(root, { limit: 2 });
    assert.equal(page.jobs.length, 2);
    assert.equal(listExecutionJobs(root, { after: page.next! }).jobs.length, 2);

    const modified = submitExecutionJob(root, { ...request, key: 'request-d' });
    const path = join(root, 'jobs', modified.id, 'plan.json');
    const plan = JSON.parse(readFileSync(path, 'utf8')); plan.title = 'changed';
    writeFileSync(path, JSON.stringify(plan));
    await assert.rejects(workExecutionJob(root, modified.id, 'worker-a', { execute }), /identity changed/);
    assert.equal(readExecutionJob(root, modified.id).status, 'queued');
  } finally { rmSync(root, { recursive: true, force: true }); }
});
