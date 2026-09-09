import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { submitExecutionJob, workExecutionJob } from '../src/campaigns/execution-jobs.js';
import { readCampaignState } from '../src/campaigns/campaign-scheduler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

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
