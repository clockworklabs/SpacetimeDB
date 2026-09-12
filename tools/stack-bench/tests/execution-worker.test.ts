import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { runExecutionWorker } from '../src/campaigns/execution-worker.js';
import { readExecutionJob, submitExecutionJob, workExecutionJob } from '../src/campaigns/execution-jobs.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import type { executeCampaign } from '../src/campaigns/campaign-runner.js';

test('worker discovers jobs, honors placement and concurrency, and drains without cancelling claims', { timeout: 30_000 }, async () => {
  const root = mkdtempSync(join(tmpdir(), 'execution-worker-'));
  const stop = new AbortController();
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  let started!: () => void;
  const ready = new Promise<void>(resolve => { started = resolve; });
  let worker: Promise<void> | undefined;
  try {
    mkdirSync(join(root, 'plans'));
    copyFileSync(join(STACK_BENCH_ROOT, 'tests/fixtures/campaign.deterministic.json'), join(root, 'plans/test.json'));
    const request = { planFile: 'test.json', hostId: 'worker-a' };
    const foreign = submitExecutionJob(root, { ...request, key: 'foreign', hostId: 'worker-b' });
    const selected = ['one', 'two', 'three'].map(key => submitExecutionJob(root, { ...request, key }));
    let calls = 0;
    const execute = (async (_plan, _out, options) => {
      calls++;
      if (calls === 2) started();
      await gate;
      assert.equal(options?.signal?.aborted, false, 'worker shutdown must not cancel a paid claim');
      return { summary: { completed: 9, pending: 0, running: 0, invalid: 0 } };
    }) as typeof executeCampaign;
    worker = runExecutionWorker(root, 'worker-a', { concurrency: 2, signal: stop.signal,
      work: (results, id, host, options) => workExecutionJob(results, id, host, { ...options, execute }) });
    await ready;
    assert.equal(calls, 2);
    stop.abort();
    release();
    await worker;
    assert.equal(calls, 2, 'shutdown must not claim the pending job');
    assert.equal(readExecutionJob(root, foreign.id).status, 'queued');
    const states = selected.map(job => readExecutionJob(root, job.id).status).sort();
    assert.deepEqual(states, ['completed', 'completed', 'queued']);
    await assert.rejects(runExecutionWorker(root, 'worker-a', { concurrency: 0, signal: stop.signal }), /concurrency/);
  } finally {
    stop.abort(); release(); await worker;
    rmSync(root, { recursive: true, force: true });
  }
});

test('worker isolates corrupt records and pre-claim faults without fabricating results', { timeout: 15_000 }, async () => {
  const root = mkdtempSync(join(tmpdir(), 'execution-worker-failure-'));
  const stop = new AbortController();
  const reports: string[] = [];
  try {
    mkdirSync(join(root, 'plans'));
    copyFileSync(join(STACK_BENCH_ROOT, 'tests/fixtures/campaign.deterministic.json'), join(root, 'plans/test.json'));
    const jobs = ['first', 'second', 'third'].map(key => submitExecutionJob(root,
      { key, planFile: 'test.json' })).sort((a, b) => a.id.localeCompare(b.id));
    writeFileSync(join(root, 'jobs', jobs[0]!.id, 'job.json'), '{broken');
    const calls: string[] = [];
    await runExecutionWorker(root, 'worker-a', { concurrency: 1, signal: stop.signal,
      report: message => reports.push(message), work: async (results, id, host, options) => {
        calls.push(id);
        if (id === jobs[1]!.id) throw new Error('plan identity changed');
        const result = await workExecutionJob(results, id, host, { ...options,
          execute: (async (_plan, _out, _options) => ({ summary: { completed: 9, pending: 0, running: 0, invalid: 0 } })) as typeof executeCampaign });
        stop.abort();
        return result;
      } });
    assert.deepEqual(calls, [jobs[1]!.id, jobs[2]!.id]);
    assert.equal(readExecutionJob(root, jobs[1]!.id).status, 'queued');
    assert.equal(readExecutionJob(root, jobs[2]!.id).status, 'completed');
    assert(reports.some(line => line.includes('cannot be read')));
    assert(reports.some(line => line.includes('plan identity changed')));
  } finally { stop.abort(); rmSync(root, { recursive: true, force: true }); }
});
