import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

import { compileCampaignFile, validateCampaignDefinition }
  from '../src/campaigns/campaign-compiler.js';
import { claimNextAttempt, classifyCampaignExecution, createCampaignState, finishCampaignExecution,
  markInterruptedExecution, scheduleDependencyContinuation, validateCampaignState }
  from '../src/campaigns/campaign-scheduler.js';
import type { CampaignClaim, CampaignState } from '../src/campaigns/campaign-scheduler.js';

const example = join(STACK_BENCH_ROOT, 'appliance', 'campaign.example.json');
const compiledExample = compileCampaignFile(example);
const initialState = createCampaignState(compiledExample,
  { now: '2026-08-12T00:00:00.000Z' });
const plan = () => structuredClone(compiledExample);
const prepared = () => structuredClone(initialState);

function attemptAt(state: CampaignState, index = 0) {
  const attempt = state.attempts[index];
  assert(attempt);
  return attempt;
}

function executionAt(state: CampaignState, attemptIndex = 0, executionIndex = 0) {
  const execution = attemptAt(state, attemptIndex).executions[executionIndex];
  assert(execution);
  return execution;
}

function retryAt(state: CampaignState, attemptIndex = 0, executionIndex = 0) {
  const retry = executionAt(state, attemptIndex, executionIndex).retry;
  assert(retry);
  return retry;
}

function requireClaim(result: { state: CampaignState; claim: CampaignClaim | null;
  capacityFull: boolean }) {
  assert.ok(result.claim);
  return { ...result, claim: result.claim };
}

const claimed = () => requireClaim(claimNextAttempt(prepared(), {
  now: '2026-08-12T00:01:00.000Z', admissionId: 'admission-1',
}));

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
  attemptAt(state).plan.mode = { id: 'dependency', version: '3.0.0' };
  for (let index = 0; index < state.attempts.length; index += 1) {
    const minute = String(index * 2 + 1).padStart(2, '0');
    const completedMinute = String(index * 2 + 2).padStart(2, '0');
    const claimedAttempt = requireClaim(claimNextAttempt(state, {
      now: `2026-08-12T00:${minute}:00.000Z`, admissionId: 'admission-1',
    }));
    state = finishCampaignExecution(claimedAttempt.state, claimedAttempt.claim.executionId,
      { exitCode: 0, run: { outcome: { kind: 'passed' } } },
      { now: `2026-08-12T00:${completedMinute}:00.000Z` });
  }
  assert.equal(state.status, 'completed');
  const attempt = attemptAt(state);
  const priorOutput = executionAt(state).output;
  state = scheduleDependencyContinuation(state, attempt.plan.id, {
    grantId: 'operator-grant-1', level: 1, nodeIds: ['catalog', 'accounts'],
    strikes: 2, stateSha256: 'a'.repeat(64),
    resumeFrom: `continuations/${attempt.plan.id}/operator-grant-1`,
  }, { now: '2026-08-12T00:20:00.000Z' });
  assert.equal(state.status, 'prepared');
  assert.deepEqual(executionAt(state).continuation?.nodeIds,
    ['accounts', 'catalog']);

  const resumed = requireClaim(claimNextAttempt(state, {
    now: '2026-08-12T00:21:00.000Z', admissionId: 'admission-2',
  }));
  assert.equal(resumed.claim.resumeFrom,
    `continuations/${attempt.plan.id}/operator-grant-1`);
  assert.deepEqual(resumed.claim.priorOutputs, [priorOutput]);
  assert.equal(executionAt(resumed.state).status, 'completed');
  assert.equal(executionAt(resumed.state, 0, 1).status, 'running');
  const wrongMode = structuredClone(resumed.state);
  attemptAt(wrongMode).plan.mode = { id: 'sequential', version: '1.0.0' };
  assert.throws(() => validateCampaignState(wrongMode),
    /continuation requires a completed dependency execution/);
});

test('dependency continuation scheduling rejects non-terminal and duplicate requests', () => {
  const state = prepared();
  const attempt = attemptAt(state);
  attempt.plan.mode = { id: 'dependency', version: '3.0.0' };
  assert.throws(() => scheduleDependencyContinuation(state, attempt.plan.id, {
    grantId: 'grant', level: 1, nodeIds: ['accounts'], strikes: 1,
    stateSha256: 'a'.repeat(64),
    resumeFrom: `continuations/${attempt.plan.id}/grant`,
  }), /completed campaign/);
});

function parallelPlan(parallelism = 3, repetitions = 1) {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-parallel-plan-'));
  const path = join(root, 'campaign.json');
  const definition = validateCampaignDefinition(JSON.parse(readFileSync(example, 'utf8')),
    { source: example });
  definition.parallelism = parallelism;
  definition.repetitions = repetitions;
  writeFileSync(path, `${JSON.stringify(definition, null, 2)}\n`);
  try { return compileCampaignFile(path); }
  finally { rmSync(root, { recursive: true, force: true }); }
}

test('serial campaign state materializes every attempt and enforces its one-slot capacity', () => {
  const campaign = plan();
  const initial = prepared();
  assert.deepEqual(initial.summary, { completed: 0, executions: 0, invalid: 0, pending: 9,
    running: 0, total: 9 });
  const claimed = requireClaim(claimNextAttempt(initial, {
    now: '2026-08-12T00:01:00.000Z', admissionId: 'admission-1',
  }));
  const firstPlan = campaign.attempts[0];
  assert(firstPlan);
  assert.equal(claimed.claim.attempt.id, firstPlan.id);
  assert.equal(claimed.claim.executionId, `${firstPlan.id}-execution1`);
  assert.equal(claimed.state.summary.running, 1);
  const full = claimNextAttempt(claimed.state, { admissionId: 'admission-1' });
  assert.equal(full.claim, null);
  assert.equal(full.capacityFull, true);
});

test('parallel campaign claims unique slots and reuses a slot only after completion', () => {
  const campaign = parallelPlan(3, 2);
  let state = createCampaignState(campaign, { now: '2026-08-12T00:00:00.000Z' });
  const claims: CampaignClaim[] = [];
  for (let index = 0; index < 3; index += 1) {
    const next = requireClaim(claimNextAttempt(state, {
      now: `2026-08-12T00:0${index + 1}:00.000Z`, admissionId: 'admission-1',
    }));
    state = next.state;
    claims.push(next.claim);
  }
  assert.deepEqual(claims.map(claim => claim.runIndex), [0, 1, 2]);
  assert.equal(state.summary.running, 3);
  assert.equal(claimNextAttempt(state, { admissionId: 'admission-1' }).capacityFull, true);
  const releasedClaim = claims[1];
  assert(releasedClaim);
  state = finishCampaignExecution(state, releasedClaim.executionId,
    { exitCode: 0, run: { outcome: { kind: 'passed' } } },
    { now: '2026-08-12T00:05:00.000Z' });
  const reused = requireClaim(claimNextAttempt(state, {
    now: '2026-08-12T00:06:00.000Z', admissionId: 'admission-1',
  }));
  assert.equal(reused.claim.runIndex, 1);
  assert.equal(reused.capacityFull, false);
});

test('invalid executions remain visible and retries append rather than overwrite', () => {
  const campaign = plan();
  const first = claimNextAttempt(createCampaignState(campaign, { now: '2026-08-12T00:00:00.000Z' }),
    { now: '2026-08-12T00:01:00.000Z', admissionId: 'admission-1' }).state;
  const executionId = executionAt(first).id;
  const retryable = finishCampaignExecution(first, executionId, {
    exitCode: 1, run: { outcome: { kind: 'harness_failure', reason: 'provider overloaded' } },
    retryAuthority: { transient: true, recoveryClean: true, budgetKnown: true,
      cause: 'provider-http-503' },
  }, { retries: 1, retryOn: ['harness_failure'], now: '2026-08-12T00:02:00.000Z' });
  assert.equal(attemptAt(retryable).status, 'pending');
  assert.equal(executionAt(retryable).status, 'invalid');
  assert.deepEqual(retryAt(retryable), {
    requested: true, transient: true, recoveryClean: true, budgetKnown: true, scheduled: true,
    cause: 'provider-http-503', reason: 'transient failure has clean recovery proof',
  });
  const second = requireClaim(claimNextAttempt(retryable, {
    now: '2026-08-12T00:03:00.000Z', admissionId: 'admission-1',
  }));
  const firstPlan = campaign.attempts[0];
  assert(firstPlan);
  const priorExecution = executionAt(retryable);
  assert.equal(second.claim.executionId, `${firstPlan.id}-execution2`);
  assert.equal(second.claim.resumeFrom, priorExecution.output);
  assert.deepEqual(second.claim.priorOutputs, [priorExecution.output]);
  const complete = finishCampaignExecution(second.state, second.claim.executionId, {
    exitCode: 0, run: { outcome: { kind: 'passed' } },
  }, { retries: 1, retryOn: ['harness_failure'], now: '2026-08-12T00:04:00.000Z' });
  assert.equal(attemptAt(complete).status, 'completed');
  assert.deepEqual(attemptAt(complete).executions.map(item => item.status), ['invalid', 'completed']);
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
  assert.equal(attemptAt(state).status, 'invalid');
  assert.equal(retryAt(state).scheduled, false);
  assert.equal(retryAt(state).reason, 'prior provider spend is unknown');
});

test('an inconclusive measurement requires operator review instead of spending another attempt', () => {
  const active = claimed();
  const state = finishCampaignExecution(active.state, active.claim.executionId, {
    exitCode: 0, run: { outcome: { kind: 'inconclusive',
      inconclusive: ['ecommerce.spec.concurrency-safety.duplicate-checkout.203b'] } },
  }, { retries: 1, retryOn: ['inconclusive'], now: '2026-08-12T00:04:00.000Z' });
  assert.equal(attemptAt(state).status, 'invalid');
  assert.equal(executionAt(state).status, 'invalid');
  assert.equal(executionAt(state).outcome, 'inconclusive');
  assert.match(executionAt(state).reason ?? '', /pass-or-fail/);
  assert.equal(retryAt(state).scheduled, false);
  assert.match(retryAt(state).reason, /not explicitly transient/);
});

test('deterministic harness failures and unproven cleanup never retry', () => {
  const deterministic = claimed();
  const stopped = finishCampaignExecution(deterministic.state, deterministic.claim.executionId, {
    exitCode: 2, run: { outcome: { kind: 'harness_failure',
      reason: 'invalid campaign configuration' } },
  }, { retries: 3, retryOn: ['harness_failure'], now: '2026-08-12T00:04:00.000Z' });
  assert.equal(attemptAt(stopped).status, 'invalid');
  assert.deepEqual(retryAt(stopped), {
    requested: true, transient: false, recoveryClean: false, budgetKnown: false, scheduled: false,
    cause: null, reason: 'failure is not explicitly transient',
  });

  const dirty = claimed();
  const unproven = finishCampaignExecution(dirty.state, dirty.claim.executionId, {
    exitCode: 1, run: { outcome: { kind: 'harness_failure', reason: 'provider overloaded' } },
    retryAuthority: { transient: true, recoveryClean: false, cause: 'provider-http-503' },
  }, { retries: 3, retryOn: ['harness_failure'], now: '2026-08-12T00:04:00.000Z' });
  assert.equal(attemptAt(unproven).status, 'invalid');
  assert.equal(retryAt(unproven).scheduled, false);
  assert.equal(retryAt(unproven).reason, 'clean recovery was not proven');
});

test('a plausible run artifact cannot hide a failed attempt process', () => {
  const active = claimed();
  const state = finishCampaignExecution(active.state, active.claim.executionId, {
    exitCode: 7, run: { outcome: { kind: 'passed' } },
  }, { now: '2026-08-12T00:04:00.000Z' });
  assert.equal(attemptAt(state).status, 'invalid');
  assert.equal(executionAt(state).outcome, 'harness_failure');
  assert.match(executionAt(state).reason ?? '', /exited 7/);
  const explained = claimed();
  const explainedState = finishCampaignExecution(explained.state, explained.claim.executionId, {
    exitCode: 9, run: { outcome: { kind: 'harness_failure', reason: 'cleanup was quarantined' } },
  }, { now: '2026-08-12T00:04:00.000Z' });
  assert.match(executionAt(explainedState).reason ?? '',
    /exited 9: cleanup was quarantined/);
});

test('interrupted and missing-artifact executions fail closed without invented results', () => {
  const initial = claimed().state;
  const executionId = executionAt(initial).id;
  const interrupted = markInterruptedExecution(initial, executionId,
    { now: '2026-08-12T00:05:00.000Z' });
  assert.equal(attemptAt(interrupted).status, 'invalid');
  assert.equal(executionAt(interrupted).outcome, 'scheduler_interrupted');

  const other = claimed().state;
  const missing = finishCampaignExecution(other, executionAt(other).id,
    { exitCode: 0, run: null }, { now: '2026-08-12T00:06:00.000Z' });
  assert.equal(executionAt(missing).outcome, 'missing_artifact');
  assert.equal(attemptAt(missing).status, 'invalid');
});

test('malformed state and inconsistent summaries never become resumable', () => {
  const state = prepared();
  state.status = 'completed';
  assert.throws(() => validateCampaignState(state), /summary or status/);
  const running = claimNextAttempt(prepared(), { admissionId: 'admission-1' }).state;
  const runningAttempt = attemptAt(running, 1);
  runningAttempt.status = 'running';
  runningAttempt.executions.push({ ...executionAt(running),
    id: `${runningAttempt.plan.id}-execution1`,
    output: `attempts/${runningAttempt.plan.id}/execution-1` });
  assert.throws(() => validateCampaignState(running), /runIndex is already in use/);
  const wrongPath = claimNextAttempt(prepared(), { admissionId: 'admission-1' }).state;
  executionAt(wrongPath).output = '../../outside';
  assert.throws(() => validateCampaignState(wrongPath), /exact execution directory/);
  const historicalRunning = claimed().state;
  const historicalAttempt = attemptAt(historicalRunning);
  historicalAttempt.executions.push({
    ...executionAt(historicalRunning),
    id: `${historicalAttempt.plan.id}-execution2`, ordinal: 2,
    output: `attempts/${historicalAttempt.plan.id}/execution-2`,
  });
  assert.throws(() => validateCampaignState(historicalRunning), /historical but not invalid/);
});
