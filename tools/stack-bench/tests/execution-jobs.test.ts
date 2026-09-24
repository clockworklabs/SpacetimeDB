import assert from 'node:assert/strict';
import fs from 'node:fs';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { cancelExecutionJob, listExecutionJobs, readExecutionJob, submitExecutionJob,
  workExecutionJob } from '../src/campaigns/execution-jobs.js';
import type { executeCampaign, inspectCampaign } from '../src/campaigns/campaign-runner.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { jobCommand, resumeExecutionJob } from '../commands/job-cli.js';
import { DEPENDENCY_CAMPAIGN } from './fixtures/dashboard-fixture.js';

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

    const stopping = submitExecutionJob(root, { ...request, key: 'request-stop' });
    let activeCalls = 0;
    const untilCancelled = (async (_plan, _directory, options) => {
      activeCalls++;
      await new Promise<void>(resolve => options!.signal!.addEventListener('abort', () => resolve(), { once: true }));
      return { summary: { pending: 1, running: 0, invalid: 0 } };
    }) as typeof executeCampaign;
    const active = workExecutionJob(root, stopping.id, 'worker-a', { execute: untilCancelled });
    cancelExecutionJob(root, stopping.id);
    assert.equal(readExecutionJob(root, stopping.id).cancellationRequested, true);
    assert.equal((await active).status, 'cancelled');
    assert.equal((await workExecutionJob(root, stopping.id, 'worker-a', { execute: untilCancelled })).status, 'cancelled');
    assert.equal(activeCalls, 1, 'cancellation retains ownership and cannot restart the worker');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('job reads tolerate a worker publishing its claim and result between file reads', context => {
  const root = mkdtempSync(join(tmpdir(), 'execution-job-read-'));
  try {
    mkdirSync(join(root, 'plans'));
    copyFileSync(join(STACK_BENCH_ROOT, 'tests/fixtures/campaign.deterministic.json'), join(root, 'plans/test.json'));
    const job = submitExecutionJob(root, { key: 'read-race', planFile: 'test.json' });
    const directory = join(root, 'jobs', job.id);
    const claimPath = join(directory, 'claim.json');
    const resultPath = join(directory, 'result.json');
    const owner = { hostId: 'worker-a', token: 'a'.repeat(64) };
    const exists = fs.existsSync;
    let published = false;
    context.mock.method(fs, 'existsSync', (path: Parameters<typeof fs.existsSync>[0]) => {
      const present = exists(path);
      if (path === claimPath && !published) {
        published = true;
        writeFileSync(claimPath, JSON.stringify({ ...owner, startedAt: new Date().toISOString() }));
        writeFileSync(resultPath, JSON.stringify({ ...owner, status: 'completed', completedAt: new Date().toISOString() }));
      }
      return present;
    });
    syncBuiltinESMExports();
    assert.doesNotThrow(() => readExecutionJob(root, job.id));
    assert.equal(published, true);
    assert.equal(readExecutionJob(root, job.id).status, 'completed');
    writeFileSync(resultPath, JSON.stringify({ ...owner, token: 'b'.repeat(64), status: 'completed', completedAt: new Date().toISOString() }));
    assert.throws(() => readExecutionJob(root, job.id), /does not belong to its worker claim/);
  } finally {
    context.mock.restoreAll();
    syncBuiltinESMExports();
    rmSync(root, { recursive: true, force: true });
  }
});

test('job resume continues the campaign with the job saved accounts and capacity policy', async () => {
  const root = mkdtempSync(join(tmpdir(), 'execution-job-resume-'));
  try {
    mkdirSync(join(root, 'plans'));
    copyFileSync(DEPENDENCY_CAMPAIGN, join(root, 'plans/reference.json'));
    const credentials = { adapters: { 'reference-fixture': 'team-a' } };
    const job = submitExecutionJob(root, { key: 'resume-job', planFile: 'reference.json', credentials });
    const planFile = join(root, 'jobs', job.id, 'plan.json');
    const directory = join(root, 'campaigns', `job-${job.id}`);
    let status = 'completed';
    const inspect = ((path: string) => {
      assert.equal(path, directory);
      return { plan: { contentSha256: job.planSha256, definition: { mode: { id: 'dependency' } } },
        state: { status, attempts: [{ executions: [{}] }] } };
    }) as unknown as typeof inspectCampaign;
    let calls = 0;
    const execute = (async (path, output, options) => {
      calls++;
      assert.equal(path, planFile);
      assert.equal(output, directory);
      assert.equal(options?.mode, 'model-free-trial');
      assert.equal(options?.capacityPolicy, 'wait');
      assert.deepEqual(options?.executionCredentials, credentials);
      return { status: 'completed', summary: { completed: 1 } };
    }) as typeof executeCampaign;
    await assert.rejects(resumeExecutionJob(root, job.id, { execute, inspect }), /scheduled work/);
    assert.equal(calls, 0);
    status = 'prepared';
    assert.deepEqual(await resumeExecutionJob(root, job.id, { execute, inspect }),
      { status: 'completed', campaignDirectory: directory, campaign: { completed: 1 } });
    assert.equal(calls, 1);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('job CLI rejects options its command does not use and resolves results like the dashboard', async () => {
  const root = mkdtempSync(join(tmpdir(), 'execution-job-cli-'));
  const cwd = process.cwd();
  try {
    mkdirSync(join(root, 'plans'));
    copyFileSync(join(STACK_BENCH_ROOT, 'tests/fixtures/campaign.deterministic.json'), join(root, 'plans/test.json'));
    const job = submitExecutionJob(root, { key: 'cli-job', planFile: 'test.json' });
    const env = { STACK_BENCH_RESULTS_DIR: root };
    for (const argv of [['work', job.id, '--host', 'h', '--concurrency', '2'], ['status', job.id, '--host', 'h'],
      ['list', '--concurrency', '2'], ['prepare', 'selection.json', '--limit', '5'], ['options', '--host', 'x'],
      ['resume', job.id, '--host', 'h']]) {
      await assert.rejects(jobCommand(argv, env), /does not take --/, argv.join(' '));
    }
    assert.equal((await jobCommand(['status', job.id], env) as { status: string }).status, 'queued');
    assert.equal((await jobCommand(['list', '--limit', '1'], env) as { jobs: unknown[] }).jobs.length, 1);
    await assert.rejects(jobCommand(['list'], { STACK_BENCH_RESULTS_DIR: 'results' }), /must be an absolute path/);
    // An empty setting means the package results folder, not the working directory.
    process.chdir(root);
    const packageResults = await jobCommand(['list'], { STACK_BENCH_RESULTS_DIR: '' }) as
      { jobs: Array<{ job: { id: string } }> };
    assert.equal(packageResults.jobs.some(entry => entry.job.id === job.id), false);
  } finally { process.chdir(cwd); rmSync(root, { recursive: true, force: true }); }
});
