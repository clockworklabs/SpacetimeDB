import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { compileCampaignFile } from '../campaign-compiler.mjs';
import { claimNextAttempt, createCampaignState, finishCampaignExecution,
  initializeCampaignDirectory, markInterruptedExecution, readCampaignState,
  validateCampaignState, writeCampaignState } from '../campaign-scheduler.mjs';

const example = join(import.meta.dirname, '..', 'appliance', 'campaign.example.json');
const plan = () => compileCampaignFile(example);
const prepared = () => createCampaignState(plan(), { now: '2026-08-12T00:00:00.000Z' });
const claimed = () => claimNextAttempt(prepared(), { now: '2026-08-12T00:01:00.000Z' });

test('campaign state materializes every attempt and claims one exact slot at a time', () => {
  const campaign = plan();
  const initial = createCampaignState(campaign, { now: '2026-08-12T00:00:00.000Z' });
  assert.deepEqual(initial.summary, { completed: 0, executions: 0, invalid: 0, pending: 9,
    running: 0, total: 9 });
  const claimed = claimNextAttempt(initial, { now: '2026-08-12T00:01:00.000Z' });
  assert.equal(claimed.claim.attempt.id, campaign.attempts[0].id);
  assert.equal(claimed.claim.executionId, `${campaign.attempts[0].id}-execution1`);
  assert.equal(claimed.state.summary.running, 1);
  assert.throws(() => claimNextAttempt(claimed.state), /already has a running attempt/);
});

test('invalid executions remain visible and retries append rather than overwrite', () => {
  const campaign = plan();
  const first = claimNextAttempt(createCampaignState(campaign, { now: '2026-08-12T00:00:00.000Z' }),
    { now: '2026-08-12T00:01:00.000Z' }).state;
  const executionId = first.attempts[0].executions[0].id;
  const retryable = finishCampaignExecution(first, executionId, {
    exitCode: 1, run: { outcome: { kind: 'harness_failure', reason: 'browser crashed' } },
  }, { retries: 1, retryOn: ['harness_failure'], now: '2026-08-12T00:02:00.000Z' });
  assert.equal(retryable.attempts[0].status, 'pending');
  assert.equal(retryable.attempts[0].executions[0].status, 'invalid');
  const second = claimNextAttempt(retryable, { now: '2026-08-12T00:03:00.000Z' });
  assert.equal(second.claim.executionId, `${campaign.attempts[0].id}-execution2`);
  const complete = finishCampaignExecution(second.state, second.claim.executionId, {
    exitCode: 0, run: { outcome: { kind: 'passed' } },
  }, { retries: 1, retryOn: ['harness_failure'], now: '2026-08-12T00:04:00.000Z' });
  assert.equal(complete.attempts[0].status, 'completed');
  assert.deepEqual(complete.attempts[0].executions.map(item => item.status), ['invalid', 'completed']);
  assert.equal(complete.summary.executions, 2);
});

test('a plausible run artifact cannot hide a failed attempt process', () => {
  const active = claimed();
  const state = finishCampaignExecution(active.state, active.claim.executionId, {
    exitCode: 7, run: { outcome: { kind: 'passed' } },
  }, { now: '2026-08-12T00:04:00.000Z' });
  assert.equal(state.attempts[0].status, 'invalid');
  assert.equal(state.attempts[0].executions[0].outcome, 'harness_failure');
  assert.match(state.attempts[0].executions[0].reason, /exited 7/);
});

test('interrupted and missing-artifact executions fail closed without invented results', () => {
  const initial = claimed().state;
  const executionId = initial.attempts[0].executions[0].id;
  const interrupted = markInterruptedExecution(initial, executionId,
    { now: '2026-08-12T00:05:00.000Z' });
  assert.equal(interrupted.attempts[0].status, 'invalid');
  assert.equal(interrupted.attempts[0].executions[0].outcome, 'scheduler_interrupted');

  const other = claimed().state;
  const missing = finishCampaignExecution(other, other.attempts[0].executions[0].id,
    { exitCode: 0, run: null }, { now: '2026-08-12T00:06:00.000Z' });
  assert.equal(missing.attempts[0].executions[0].outcome, 'missing_artifact');
  assert.equal(missing.attempts[0].status, 'invalid');
});

test('campaign directory initialization is identity-bound and resumes exact state', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-state-'));
  try {
    const campaign = plan();
    const initialized = initializeCampaignDirectory(campaign, root,
      { now: '2026-08-12T00:00:00.000Z' });
    const claimed = claimNextAttempt(initialized.state, { now: '2026-08-12T00:01:00.000Z' });
    writeCampaignState(initialized.paths.state, campaign, claimed.state);
    const resumed = readCampaignState(root);
    assert.equal(resumed.plan.contentSha256, campaign.contentSha256);
    assert.equal(resumed.state.attempts[0].executions[0].status, 'running');
    assert.equal(initializeCampaignDirectory(campaign, root).state.summary.running, 1);
    assert.throws(() => initializeCampaignDirectory({ ...campaign, contentSha256: 'a'.repeat(64) }, root),
      /content identity/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('malformed state and inconsistent summaries never become resumable', () => {
  const state = createCampaignState(plan());
  state.status = 'completed';
  assert.throws(() => validateCampaignState(state), /summary or status/);
  const running = claimNextAttempt(createCampaignState(plan())).state;
  running.attempts[1].status = 'running';
  running.attempts[1].executions.push({ ...running.attempts[0].executions[0],
    id: `${running.attempts[1].plan.id}-execution1`,
    output: `attempts/${running.attempts[1].plan.id}/execution-1` });
  assert.throws(() => validateCampaignState(running), /more than one execution/);
  const wrongPath = claimNextAttempt(createCampaignState(plan())).state;
  wrongPath.attempts[0].executions[0].output = '../../outside';
  assert.throws(() => validateCampaignState(wrongPath), /exact execution directory/);
  const historicalRunning = claimed().state;
  historicalRunning.attempts[0].executions.push({
    ...historicalRunning.attempts[0].executions[0],
    id: `${historicalRunning.attempts[0].plan.id}-execution2`, ordinal: 2,
    output: `attempts/${historicalRunning.attempts[0].plan.id}/execution-2`,
  });
  assert.throws(() => validateCampaignState(historicalRunning), /historical but not invalid/);
});
