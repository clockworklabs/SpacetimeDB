import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { compileCampaignFile } from '../src/campaigns/campaign-compiler.mjs';
import { claimNextAttempt, classifyCampaignExecution, createCampaignState, finishCampaignExecution,
  initializeCampaignDirectory, markInterruptedExecution, readCampaignState,
  scheduleDependencyContinuation, validateCampaignState, writeCampaignState }
  from '../src/campaigns/campaign-scheduler.mjs';

const example = join(import.meta.dirname, '..', 'appliance', 'campaign.example.json');
const plan = () => compileCampaignFile(example);
const prepared = () => createCampaignState(plan(), { now: '2026-08-12T00:00:00.000Z' });
const claimed = () => claimNextAttempt(prepared(), { now: '2026-08-12T00:01:00.000Z',
  admissionId: 'admission-1' });

test('provider failures remain distinct from harness failures in campaign state', () => {
  assert.deepEqual(classifyCampaignExecution({ exitCode: 1, run: { outcome: {
    kind: 'provider_failure', reason: 'provider-connection-error',
  } } }), { status: 'invalid', outcome: 'provider_failure',
    reason: 'attempt process exited 1: provider-connection-error' });
});

test('a contaminated run remains contaminated when its process exits nonzero', () => {
  assert.deepEqual(classifyCampaignExecution({ exitCode: 4, run: {
    contaminated: true,
    contamination: { verdict: 'scores unusable' },
    outcome: { kind: 'ungraded' },
  } }), { status: 'invalid', outcome: 'contaminated', reason: 'scores unusable' });
});

test('a targeted dependency grant schedules one exact completed attempt for resume', () => {
  let state = prepared();
  state.attempts[0].plan.mode = { id: 'dependency', version: '2.1.0' };
  for (let index = 0; index < state.attempts.length; index += 1) {
    const minute = String(index * 2 + 1).padStart(2, '0');
    const completedMinute = String(index * 2 + 2).padStart(2, '0');
    const claimedAttempt = claimNextAttempt(state, {
      now: `2026-08-12T00:${minute}:00.000Z`, admissionId: 'admission-1',
    });
    state = finishCampaignExecution(claimedAttempt.state, claimedAttempt.claim.executionId,
      { exitCode: 0, run: { outcome: { kind: 'passed' } } },
      { now: `2026-08-12T00:${completedMinute}:00.000Z` });
  }
  assert.equal(state.status, 'completed');
  const attempt = state.attempts[0];
  const priorOutput = attempt.executions[0].output;
  state = scheduleDependencyContinuation(state, attempt.plan.id, {
    grantId: 'operator-grant-1', level: 1, nodeIds: ['catalog', 'accounts'],
    strikes: 2, snapshotSha256: 'a'.repeat(64),
    resumeFrom: `continuations/${attempt.plan.id}/operator-grant-1`,
  }, { now: '2026-08-12T00:20:00.000Z' });
  assert.equal(state.status, 'prepared');
  assert.deepEqual(state.attempts[0].executions[0].continuation.nodeIds,
    ['accounts', 'catalog']);

  const resumed = claimNextAttempt(state, {
    now: '2026-08-12T00:21:00.000Z', admissionId: 'admission-2',
  });
  assert.equal(resumed.claim.resumeFrom,
    `continuations/${attempt.plan.id}/operator-grant-1`);
  assert.deepEqual(resumed.claim.priorOutputs, [priorOutput]);
  assert.equal(resumed.state.attempts[0].executions[0].status, 'completed');
  assert.equal(resumed.state.attempts[0].executions[1].status, 'running');
  const wrongMode = structuredClone(resumed.state);
  wrongMode.attempts[0].plan.mode = { id: 'sequential', version: '1.0.0' };
  assert.throws(() => validateCampaignState(wrongMode),
    /continuation requires a completed dependency execution/);
});

test('dependency continuation scheduling rejects non-terminal and duplicate requests', () => {
  let state = prepared();
  state.attempts[0].plan.mode = { id: 'dependency', version: '2.1.0' };
  assert.throws(() => scheduleDependencyContinuation(state, state.attempts[0].plan.id, {
    grantId: 'grant', level: 1, nodeIds: ['accounts'], strikes: 1,
    snapshotSha256: 'a'.repeat(64),
    resumeFrom: `continuations/${state.attempts[0].plan.id}/grant`,
  }), /completed campaign/);
});

function parallelPlan(parallelism = 3, repetitions = 1) {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-parallel-plan-'));
  const path = join(root, 'campaign.json');
  const definition = JSON.parse(readFileSync(example, 'utf8'));
  definition.parallelism = parallelism;
  definition.repetitions = repetitions;
  writeFileSync(path, `${JSON.stringify(definition, null, 2)}\n`);
  try { return compileCampaignFile(path); }
  finally { rmSync(root, { recursive: true, force: true }); }
}

test('serial campaign state materializes every attempt and enforces its one-slot capacity', () => {
  const campaign = plan();
  const initial = createCampaignState(campaign, { now: '2026-08-12T00:00:00.000Z' });
  assert.deepEqual(initial.summary, { completed: 0, executions: 0, invalid: 0, pending: 9,
    running: 0, total: 9 });
  const claimed = claimNextAttempt(initial, { now: '2026-08-12T00:01:00.000Z',
    admissionId: 'admission-1' });
  assert.equal(claimed.claim.attempt.id, campaign.attempts[0].id);
  assert.equal(claimed.claim.executionId, `${campaign.attempts[0].id}-execution1`);
  assert.equal(claimed.state.summary.running, 1);
  const full = claimNextAttempt(claimed.state, { admissionId: 'admission-1' });
  assert.equal(full.claim, null);
  assert.equal(full.capacityFull, true);
});

test('parallel campaign claims unique slots and reuses a slot only after completion', () => {
  const campaign = parallelPlan(3, 2);
  let state = createCampaignState(campaign, { now: '2026-08-12T00:00:00.000Z' });
  const claims = [];
  for (let index = 0; index < 3; index += 1) {
    const next = claimNextAttempt(state, { now: `2026-08-12T00:0${index + 1}:00.000Z`,
      admissionId: 'admission-1' });
    state = next.state;
    claims.push(next.claim);
  }
  assert.deepEqual(claims.map(claim => claim.runIndex), [0, 1, 2]);
  assert.equal(state.summary.running, 3);
  assert.equal(claimNextAttempt(state, { admissionId: 'admission-1' }).capacityFull, true);
  state = finishCampaignExecution(state, claims[1].executionId,
    { exitCode: 0, run: { outcome: { kind: 'passed' } } },
    { now: '2026-08-12T00:05:00.000Z' });
  const reused = claimNextAttempt(state, { now: '2026-08-12T00:06:00.000Z',
    admissionId: 'admission-1' });
  assert.equal(reused.claim.runIndex, 1);
  assert.equal(reused.capacityFull, false);
});

test('invalid executions remain visible and retries append rather than overwrite', () => {
  const campaign = plan();
  const first = claimNextAttempt(createCampaignState(campaign, { now: '2026-08-12T00:00:00.000Z' }),
    { now: '2026-08-12T00:01:00.000Z', admissionId: 'admission-1' }).state;
  const executionId = first.attempts[0].executions[0].id;
  const retryable = finishCampaignExecution(first, executionId, {
    exitCode: 1, run: { outcome: { kind: 'harness_failure', reason: 'provider overloaded' } },
    retryAuthority: { transient: true, recoveryClean: true, budgetKnown: true,
      cause: 'provider-http-503' },
  }, { retries: 1, retryOn: ['harness_failure'], now: '2026-08-12T00:02:00.000Z' });
  assert.equal(retryable.attempts[0].status, 'pending');
  assert.equal(retryable.attempts[0].executions[0].status, 'invalid');
  assert.deepEqual(retryable.attempts[0].executions[0].retry, {
    requested: true, transient: true, recoveryClean: true, budgetKnown: true, scheduled: true,
    cause: 'provider-http-503', reason: 'transient failure has clean recovery proof',
  });
  const second = claimNextAttempt(retryable, { now: '2026-08-12T00:03:00.000Z',
    admissionId: 'admission-1' });
  assert.equal(second.claim.executionId, `${campaign.attempts[0].id}-execution2`);
  assert.equal(second.claim.resumeFrom, retryable.attempts[0].executions[0].output);
  assert.deepEqual(second.claim.priorOutputs, [retryable.attempts[0].executions[0].output]);
  const complete = finishCampaignExecution(second.state, second.claim.executionId, {
    exitCode: 0, run: { outcome: { kind: 'passed' } },
  }, { retries: 1, retryOn: ['harness_failure'], now: '2026-08-12T00:04:00.000Z' });
  assert.equal(complete.attempts[0].status, 'completed');
  assert.deepEqual(complete.attempts[0].executions.map(item => item.status), ['invalid', 'completed']);
  assert.equal(complete.summary.executions, 2);
});

test('a retry is blocked when prior provider spend is unknown', () => {
  const active = claimed();
  const state = finishCampaignExecution(active.state, active.claim.executionId, {
    exitCode: 1,
    run: { outcome: { kind: 'harness_failure', reason: 'provider overloaded' } },
    retryAuthority: { transient: true, recoveryClean: true, budgetKnown: false,
      cause: 'usage-receipt-missing' },
  }, { retries: 1, retryOn: ['harness_failure'], now: '2026-08-12T00:04:00.000Z' });
  assert.equal(state.attempts[0].status, 'invalid');
  assert.equal(state.attempts[0].executions[0].retry.scheduled, false);
  assert.equal(state.attempts[0].executions[0].retry.reason, 'prior provider spend is unknown');
});

test('an inconclusive measurement requires operator review instead of spending another attempt', () => {
  const active = claimed();
  const state = finishCampaignExecution(active.state, active.claim.executionId, {
    exitCode: 0, run: { outcome: { kind: 'inconclusive',
      inconclusive: ['ecommerce.spec.concurrency-safety.duplicate-checkout.203b'] } },
  }, { retries: 1, retryOn: ['inconclusive'], now: '2026-08-12T00:04:00.000Z' });
  assert.equal(state.attempts[0].status, 'invalid');
  assert.equal(state.attempts[0].executions[0].status, 'invalid');
  assert.equal(state.attempts[0].executions[0].outcome, 'inconclusive');
  assert.match(state.attempts[0].executions[0].reason, /pass-or-fail/);
  assert.equal(state.attempts[0].executions[0].retry.scheduled, false);
  assert.match(state.attempts[0].executions[0].retry.reason, /not explicitly transient/);
});

test('deterministic harness failures and unproven cleanup never retry', () => {
  const deterministic = claimed();
  const stopped = finishCampaignExecution(deterministic.state, deterministic.claim.executionId, {
    exitCode: 2, run: { outcome: { kind: 'harness_failure',
      reason: 'invalid campaign configuration' } },
  }, { retries: 3, retryOn: ['harness_failure'], now: '2026-08-12T00:04:00.000Z' });
  assert.equal(stopped.attempts[0].status, 'invalid');
  assert.deepEqual(stopped.attempts[0].executions[0].retry, {
    requested: true, transient: false, recoveryClean: false, budgetKnown: false, scheduled: false,
    cause: null, reason: 'failure is not explicitly transient',
  });

  const dirty = claimed();
  const unproven = finishCampaignExecution(dirty.state, dirty.claim.executionId, {
    exitCode: 1, run: { outcome: { kind: 'harness_failure', reason: 'provider overloaded' } },
    retryAuthority: { transient: true, recoveryClean: false, cause: 'provider-http-503' },
  }, { retries: 3, retryOn: ['harness_failure'], now: '2026-08-12T00:04:00.000Z' });
  assert.equal(unproven.attempts[0].status, 'invalid');
  assert.equal(unproven.attempts[0].executions[0].retry.scheduled, false);
  assert.equal(unproven.attempts[0].executions[0].retry.reason, 'clean recovery was not proven');
});

test('a plausible run artifact cannot hide a failed attempt process', () => {
  const active = claimed();
  const state = finishCampaignExecution(active.state, active.claim.executionId, {
    exitCode: 7, run: { outcome: { kind: 'passed' } },
  }, { now: '2026-08-12T00:04:00.000Z' });
  assert.equal(state.attempts[0].status, 'invalid');
  assert.equal(state.attempts[0].executions[0].outcome, 'harness_failure');
  assert.match(state.attempts[0].executions[0].reason, /exited 7/);
  const explained = claimed();
  const explainedState = finishCampaignExecution(explained.state, explained.claim.executionId, {
    exitCode: 9, run: { outcome: { kind: 'harness_failure', reason: 'cleanup was quarantined' } },
  }, { now: '2026-08-12T00:04:00.000Z' });
  assert.match(explainedState.attempts[0].executions[0].reason,
    /exited 9: cleanup was quarantined/);
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
    const claimed = claimNextAttempt(initialized.state, { now: '2026-08-12T00:01:00.000Z',
      admissionId: 'admission-1' });
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
  const running = claimNextAttempt(createCampaignState(plan()), { admissionId: 'admission-1' }).state;
  running.attempts[1].status = 'running';
  running.attempts[1].executions.push({ ...running.attempts[0].executions[0],
    id: `${running.attempts[1].plan.id}-execution1`,
    output: `attempts/${running.attempts[1].plan.id}/execution-1` });
  assert.throws(() => validateCampaignState(running), /runIndex is already in use/);
  const wrongPath = claimNextAttempt(createCampaignState(plan()), { admissionId: 'admission-1' }).state;
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

test('interrupted initialization recreates only missing state from the exact stored plan', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-init-recovery-'));
  try {
    const campaign = plan();
    const initialized = initializeCampaignDirectory(campaign, root,
      { now: '2026-08-12T00:00:00.000Z' });
    unlinkSync(initialized.paths.state);
    const recovered = initializeCampaignDirectory(campaign, root,
      { now: '2026-08-12T00:01:00.000Z' });
    assert.equal(recovered.state.status, 'prepared');
    assert.equal(recovered.state.summary.executions, 0);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
