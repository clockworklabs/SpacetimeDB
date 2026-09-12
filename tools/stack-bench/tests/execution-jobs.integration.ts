import assert from 'node:assert/strict';
import { spawn, type ChildProcess } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { cancelExecutionJob, readExecutionJob, submitExecutionJob, workExecutionJob } from '../src/campaigns/execution-jobs.js';
import { readCampaignState } from '../src/campaigns/campaign-scheduler.js';
import { compiledEntrypoint, STACK_BENCH_ROOT } from '../src/package-root.js';

test('a real model-free job retains terminal evidence and duplicate work does not run again', {
  skip: process.platform !== 'linux' ? 'Uses the isolated Linux model-free runner' : false,
  timeout: 300_000,
}, async () => {
  const root = mkdtempSync(join(tmpdir(), 'execution-job-live-'));
  try {
    mkdirSync(join(root, 'plans'));
    copyFileSync(join(STACK_BENCH_ROOT, 'tests/fixtures/dependency-model-free-campaign.json'),
      join(root, 'plans/test.json'));
    // This fixture uses a fixed fresh recipe, not an upgrade/action recipe.
    const planFile = join(root, 'plans/test.json');
    const fixture = JSON.parse(readFileSync(planFile, 'utf8'));
    fixture.mode.retainPriorContracts = false;
    writeFileSync(planFile, JSON.stringify(fixture));
    const job = submitExecutionJob(root, { key: 'model-free', planFile: 'test.json' });
    const result = await workExecutionJob(root, job.id, 'test', {
      env: { PATH: process.env.PATH, HOME: root, PLAYWRIGHT_BROWSERS_PATH: process.env.PLAYWRIGHT_BROWSERS_PATH },
    });
    assert.equal(result.status, 'completed', result.error);
    const { state } = readCampaignState(result.campaignDirectory);
    const executions = state.attempts.flatMap((attempt: { executions: Array<{ status: string; output: string }> }) => attempt.executions);
    assert.equal(executions.length, 1);
    assert(executions[0]);
    assert.equal(executions[0].status, 'completed');
    const runPath = join(result.campaignDirectory, executions[0].output, 'run.json');
    const before = readFileSync(runPath, 'utf8');
    const run = JSON.parse(before);
    assert.equal(run.payload.backend, 'stub');
    assert.equal(run.payload.outcome.kind, 'app_failure');
    assert.equal(run.payload.progressionStatus.phase, 'terminal');
    assert.equal((await workExecutionJob(root, job.id, 'test')).status, 'completed');
    assert.equal(readFileSync(runPath, 'utf8'), before, 'duplicate work preserves the exact completed run evidence');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('the worker dispatches nine attempts, cancels a job, drains on SIGTERM, and resumes its queue', {
  skip: process.platform !== 'linux' ? 'Uses the isolated Linux model-free runner' : false,
  timeout: 300_000,
}, async () => {
  const root = mkdtempSync(join(tmpdir(), 'execution-worker-live-'));
  const children: ChildProcess[] = [];
  let output = '';
  const waitFor = async (condition: () => boolean) => {
    const deadline = Date.now() + 120_000;
    while (!condition()) {
      assert(Date.now() < deadline, `worker condition timed out\n${output}`);
      await delay(50);
    }
  };
  const start = () => {
    const child = spawn(process.execPath, [compiledEntrypoint('commands', 'job-cli.js'),
      'worker', '--host', 'local-test', '--concurrency', '2', '--results', root], {
      detached: true, stdio: ['ignore', 'pipe', 'pipe'],
      env: { PATH: process.env.PATH, HOME: root,
        PLAYWRIGHT_BROWSERS_PATH: process.env.PLAYWRIGHT_BROWSERS_PATH },
    });
    children.push(child);
    child.stdout!.on('data', chunk => { output += chunk; });
    child.stderr!.on('data', chunk => { output += chunk; });
    return child;
  };
  const stop = async (child: ChildProcess) => {
    await waitFor(() => child.exitCode !== null || child.signalCode !== null);
    assert.equal(child.exitCode, 0, output);
    assert.equal(child.signalCode, null, output);
  };
  try {
    mkdirSync(join(root, 'plans'));
    const fixture = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
      'tests/fixtures/dependency-model-free-campaign.json'), 'utf8'));
    fixture.mode.retainPriorContracts = false;
    writeFileSync(join(root, 'plans/single.json'), JSON.stringify(fixture));
    fixture.repetitions = fixture.parallelism = 9;
    writeFileSync(join(root, 'plans/nine.json'), JSON.stringify(fixture));
    const submit = (key: string, planFile: string) => submitExecutionJob(root,
      { key, planFile, hostId: 'local-test' });
    const main = submit('nine', 'nine.json');
    const cancelled = submit('cancel', 'nine.json');
    const first = start();
    await waitFor(() => [main, cancelled].every(job => {
      const status = readExecutionJob(root, job.id);
      assert(['queued', 'running'].includes(status.status), `${status.status}: ${status.error}\n${output}`);
      if (status.status !== 'running') return false;
      if (!existsSync(join(status.campaignDirectory, 'state.json'))) return false;
      return readCampaignState(status.campaignDirectory).state.attempts
        .filter(attempt => attempt.status === 'running').length === 9;
    }));
    // SIGTERM stops admission. Explicit cancellation stops only the selected job.
    first.kill('SIGTERM');
    await delay(100);
    const queued = submit('restart', 'single.json');
    cancelExecutionJob(root, cancelled.id);
    await stop(first);
    const cancelledResult = readExecutionJob(root, cancelled.id);
    assert.equal(cancelledResult.status, 'cancelled', output);
    assert.equal(readCampaignState(cancelledResult.campaignDirectory).state.summary.running, 0);
    assert.equal(readExecutionJob(root, queued.id).status, 'queued', output);
    const completed = readExecutionJob(root, main.id);
    assert.equal(completed.status, 'completed', output);
    const { state } = readCampaignState(completed.campaignDirectory);
    assert.equal(state.attempts.length, 9);
    const evidence = state.attempts.map(attempt => {
      assert.equal(attempt.executions.length, 1);
      assert.equal(attempt.executions[0]!.status, 'completed');
      const path = join(completed.campaignDirectory, attempt.executions[0]!.output, 'run.json');
      return { path, content: readFileSync(path, 'utf8') };
    });
    const restarted = start();
    await waitFor(() => readExecutionJob(root, queued.id).status === 'completed');
    restarted.kill('SIGTERM');
    await stop(restarted);
    for (const { path, content } of evidence) assert.equal(readFileSync(path, 'utf8'), content);
    assert.equal(readExecutionJob(root, cancelled.id).status, 'cancelled');
  } finally {
    // Each child owns its process group. Never terminate another worker's children.
    for (const child of children) {
      if (child.pid) {
        try { process.kill(-child.pid, 'SIGKILL'); }
        catch (error) { assert.equal((error as NodeJS.ErrnoException).code, 'ESRCH'); }
      }
    }
    rmSync(root, { recursive: true, force: true });
  }
});
