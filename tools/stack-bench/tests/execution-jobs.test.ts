import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { cancelExecutionJob, listExecutionJobs, readExecutionJob, submitExecutionJob,
  workExecutionJob } from '../src/campaigns/execution-jobs.js';
import { controllerInstance } from '../src/campaigns/campaign-lock.js';
import type { executeCampaign, inspectCampaign } from '../src/campaigns/campaign-runner.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { jobCommand, resumeExecutionJob, runJobCli } from '../commands/job-cli.js';

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
    copyFileSync(join(STACK_BENCH_ROOT, 'tests/fixtures/dependency-model-free-campaign.json'), join(root, 'plans/reference.json'));
    const credentials = { adapters: { deterministic: 'team-a' } };
    const job = submitExecutionJob(root, { key: 'resume-job', planFile: 'reference.json', credentials });
    await assert.rejects(resumeExecutionJob(root, job.id), /failed or reconciled interrupted job/);
    const planFile = join(root, 'jobs', job.id, 'plan.json');
    const directory = join(root, 'campaigns', `job-${job.id}`);
    const failed = (async () => { throw new Error('interrupted'); }) as typeof executeCampaign;
    assert.equal((await workExecutionJob(root, job.id, 'worker-a', { execute: failed })).status, 'failed');
    let status = 'completed';
    const inspect = ((path: string) => {
      assert.equal(path, directory);
      return { plan: { contentSha256: job.planSha256, definition: { mode: { id: 'dependency' } } },
        state: { status, attempts: [{ executions: [{}] }] } };
    }) as unknown as typeof inspectCampaign;
    let calls = 0;
    let finish!: () => void;
    const running = new Promise<void>(resolve => { finish = resolve; });
    const execute = (async (path, output, options) => {
      calls++;
      assert.equal(path, planFile);
      assert.equal(output, directory);
      assert.equal(options?.mode, 'model-free-trial');
      assert.equal(options?.capacityPolicy, 'wait');
      assert.deepEqual(options?.executionCredentials, credentials);
      await running;
      return { status: 'completed', summary: { completed: 1, pending: 0, running: 0, invalid: 0 } };
    }) as typeof executeCampaign;
    await assert.rejects(resumeExecutionJob(root, job.id, { execute, inspect }), /scheduled work/);
    assert.equal(calls, 0);
    status = 'prepared';
    const resumed = resumeExecutionJob(root, job.id, { execute, inspect });
    try {
      assert.equal(readExecutionJob(root, job.id).status, 'running');
      await assert.rejects(resumeExecutionJob(root, job.id, { execute, inspect }), /live worker/);
    } finally { finish(); }
    assert.equal((await resumed).status, 'completed');
    assert.equal(readExecutionJob(root, job.id).status, 'completed');
    assert.match(readFileSync(join(root, 'jobs', job.id, 'result.json'), 'utf8'), /interrupted/,
      'prior failure evidence is retained');
    assert.equal(calls, 1);
    await assert.rejects(resumeExecutionJob(root, job.id, { execute, inspect }), /failed or reconciled interrupted job/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('cancel reaches a resumed job and preserves its prior failure', async () => {
  const root = mkdtempSync(join(tmpdir(), 'execution-job-resume-cancel-'));
  try {
    mkdirSync(join(root, 'plans'));
    copyFileSync(join(STACK_BENCH_ROOT, 'tests/fixtures/dependency-model-free-campaign.json'), join(root, 'plans/reference.json'));
    const job = submitExecutionJob(root, { key: 'resume-cancel', planFile: 'reference.json' });
    const failed = (async () => { throw new Error('first failure'); }) as typeof executeCampaign;
    await workExecutionJob(root, job.id, 'worker-a', { execute: failed });
    const inspect = (() => ({ plan: { contentSha256: job.planSha256,
      definition: { mode: { id: 'dependency' } } },
    state: { status: 'prepared', attempts: [{ executions: [{}] }] } })) as unknown as typeof inspectCampaign;
    let abortSignal: AbortSignal | null | undefined;
    const execute = (async (_path, _output, options) => {
      abortSignal = options?.signal;
      await new Promise<void>(resolve => abortSignal!.addEventListener('abort', () => resolve(), { once: true }));
      return { status: 'prepared', summary: { completed: 0, pending: 1, running: 0, invalid: 0 } };
    }) as typeof executeCampaign;
    const active = resumeExecutionJob(root, job.id, { execute, inspect });
    assert.equal(readExecutionJob(root, job.id).status, 'running');
    cancelExecutionJob(root, job.id);
    assert.equal(readExecutionJob(root, job.id).cancellationRequested, true);
    assert.equal((await active).status, 'cancelled');
    assert.equal(abortSignal?.aborted, true);
    assert.match(readFileSync(join(root, 'jobs', job.id, 'result.json'), 'utf8'), /first failure/);
    await assert.rejects(resumeExecutionJob(root, job.id, { execute, inspect }), /failed or reconciled interrupted job/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('an interrupted resume keeps its claim until its owner is proven dead', async () => {
  const root = mkdtempSync(join(tmpdir(), 'execution-job-resume-owner-'));
  try {
    mkdirSync(join(root, 'plans'));
    copyFileSync(join(STACK_BENCH_ROOT, 'tests/fixtures/dependency-model-free-campaign.json'), join(root, 'plans/reference.json'));
    const job = submitExecutionJob(root, { key: 'resume-owner', planFile: 'reference.json' });
    const base = join(root, 'jobs', job.id);
    const owner = { hostId: 'worker-a', token: 'a'.repeat(64), startedAt: new Date().toISOString(),
      ownerPid: process.pid, ownerInstance: controllerInstance() };
    writeFileSync(join(base, 'claim.json'), JSON.stringify(owner));
    const inspect = (() => ({ plan: { contentSha256: job.planSha256,
      definition: { mode: { id: 'dependency' } } },
    state: { status: 'prepared', attempts: [{ executions: [{}] }] } })) as unknown as typeof inspectCampaign;
    const execute = (async () => ({ status: 'completed', summary: { completed: 1,
      pending: 0, running: 0, invalid: 0 } })) as unknown as typeof executeCampaign;
    await assert.rejects(resumeExecutionJob(root, job.id, { execute, inspect }), /live worker/);
    assert.equal(readExecutionJob(root, job.id).status, 'running');
    writeFileSync(join(base, 'claim.json'), JSON.stringify({ ...owner, ownerPid: 2147483647 }));
    assert.equal((await resumeExecutionJob(root, job.id, { execute, inspect })).status, 'completed');

    const repeated = submitExecutionJob(root, { key: 'resume-owner-repeated', planFile: 'reference.json' });
    const failed = (async () => { throw new Error('prior failure'); }) as typeof executeCampaign;
    await workExecutionJob(root, repeated.id, 'worker-a', { execute: failed });
    const repeatedBase = join(root, 'jobs', repeated.id);
    mkdirSync(join(repeatedBase, 'resume-000001'));
    writeFileSync(join(repeatedBase, 'resume-000001', 'claim.json'),
      JSON.stringify({ ...owner, ownerPid: 2147483647 }));
    const repeatedInspect = (() => ({ plan: { contentSha256: repeated.planSha256,
      definition: { mode: { id: 'dependency' } } },
    state: { status: 'prepared', attempts: [{ executions: [{}] }] } })) as unknown as typeof inspectCampaign;
    assert.equal((await resumeExecutionJob(root, repeated.id,
      { execute, inspect: repeatedInspect })).status, 'completed');
    assert.match(readFileSync(join(repeatedBase, 'result.json'), 'utf8'), /prior failure/);
    assert.equal(readdirSync(repeatedBase).includes('resume-000002'), true);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('separate processes cannot both execute one resume claim', async () => {
  const root = mkdtempSync(join(tmpdir(), 'execution-job-resume-race-'));
  const children: ReturnType<typeof spawn>[] = [];
  try {
    mkdirSync(join(root, 'plans'));
    copyFileSync(join(STACK_BENCH_ROOT, 'tests/fixtures/dependency-model-free-campaign.json'), join(root, 'plans/reference.json'));
    const job = submitExecutionJob(root, { key: 'resume-race', planFile: 'reference.json' });
    const failed = (async () => { throw new Error('interrupted'); }) as typeof executeCampaign;
    await workExecutionJob(root, job.id, 'worker-a', { execute: failed });
    const marker = join(root, 'executing');
    const release = join(root, 'release');
    const moduleUrl = new URL('../src/campaigns/execution-jobs.js', import.meta.url).href;
    const script = `import { existsSync, writeFileSync } from 'node:fs';
import { setTimeout } from 'node:timers/promises';
import { resumeOwnedExecutionJob } from ${JSON.stringify(moduleUrl)};
await resumeOwnedExecutionJob(process.argv[1], process.argv[2], { execute: async () => {
  writeFileSync(process.argv[3] + '.' + process.pid, 'entered');
  while (!existsSync(process.argv[4])) await setTimeout(20);
  return { summary: { completed: 1, pending: 0, running: 0, invalid: 0 } };
} });`;
    const launch = () => {
      const child = spawn(process.execPath, ['--input-type=module', '-e', script,
        root, job.id, marker, release], { stdio: ['ignore', 'ignore', 'pipe'] });
      children.push(child);
      let error = '';
      child.stderr.on('data', chunk => { error += String(chunk); });
      return { child, done: new Promise<number | null>(resolve => child.on('exit', code => resolve(code))),
        error: () => error };
    };
    const first = launch();
    const second = launch();
    const deadline = Date.now() + 30_000;
    const entries = () => readdirSync(root).filter(name => name.startsWith('executing.'));
    while (!entries().length || (first.child.exitCode === null && second.child.exitCode === null)) {
      assert(entries().length <= 1, 'both processes entered execution');
      assert(Date.now() < deadline, `${first.error()} ${second.error()}`);
      await delay(20);
    }
    assert.equal(entries().length, 1, 'only one process entered execution');
    writeFileSync(release, 'go');
    const firstCode = await first.done;
    const secondCode = await second.done;
    assert(firstCode === 0 || secondCode === 0, `${first.error()} ${second.error()}`);
    assert.equal(entries().length, 1, 'loser never entered execution');
    assert.equal(readExecutionJob(root, job.id).status, 'completed');
  } finally {
    for (const child of children) if (child.exitCode === null) child.kill();
    rmSync(root, { recursive: true, force: true });
  }
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

test('job CLI prints one JSON document and separates usage errors from command failures', async t => {
  const root = mkdtempSync(join(tmpdir(), 'execution-job-exit-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'plans'));
  copyFileSync(join(STACK_BENCH_ROOT, 'tests/fixtures/campaign.deterministic.json'), join(root, 'plans/test.json'));
  const job = submitExecutionJob(root, { key: 'exit-job', planFile: 'test.json' });
  const failure = (async () => { throw new Error('synthetic failure'); }) as typeof executeCampaign;
  await workExecutionJob(root, job.id, 'h', { execute: failure });
  const env = { STACK_BENCH_RESULTS_DIR: root };
  const printed = t.mock.method(console, 'log', () => {});
  t.mock.method(console, 'error', () => {});
  for (const command of ['status', 'cancel']) {
    printed.mock.resetCalls();
    assert.equal(await runJobCli([command, job.id], env), 0, command);
    assert.equal(printed.mock.callCount(), 1);
    assert.equal(JSON.parse(printed.mock.calls[0]!.arguments[0] as string).status, 'failed');
  }
  assert.equal(await runJobCli(['work', job.id, '--host', 'h'], env), 1, 'running a job to failure fails');
  assert.equal(await runJobCli(['status'], env), 2);
  assert.equal(await runJobCli(['status', job.id, '--bogus'], env), 2);
  assert.equal(await runJobCli(['start', 'review.json'], env), 2, 'start without a host');
  assert.equal(await runJobCli(['status', 'f'.repeat(64)], env), 1);
});
