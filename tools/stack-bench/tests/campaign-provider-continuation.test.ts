import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { campaignLockTransaction, controllerInstance } from '../src/campaigns/campaign-lock.js';
import { claimNextAttempt, initializeCampaignDirectory, writeCampaignState } from '../src/campaigns/campaign-scheduler.js';
import { persistCampaignProviderInvocation, readCampaignProviderContinuationStatus,
  readCampaignProviderWaitHistory, requestCampaignProviderContinuation, waitForCampaignProviderContinuation }
  from '../src/campaigns/campaign-provider-continuation.js';

test('live provider continuation binds one durable acceptance to its wait and rejects dead owners', () => {
  const directory = mkdtempSync(join(tmpdir(), 'provider-continuation-'));
  try {
    const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign.deterministic.json'));
    const initialized = initializeCampaignDirectory(plan, directory);
    const claimed = claimNextAttempt(initialized.state, { admissionId: 'test-admission' });
    assert(claimed.claim);
    writeCampaignState(initialized.paths.state, plan, claimed.state);
    const attemptId = claimed.claim.attempt.id;
    const execution = claimed.state.attempts.find(a => a.plan.id === attemptId)!.executions.at(-1)!;
    const marker = createHash('sha256').update('test').digest('hex');
    campaignLockTransaction({ operation: 'acquire', lock: { path: join(directory, '.campaign.lock.json'),
      token: 'test', record: { version: 2, campaignId: plan.id, campaignSha256: plan.contentSha256,
        ownerPid: process.pid, ownerInstance: controllerInstance(), ownershipMarkerSha256: marker,
        acquiredAt: new Date().toISOString() } } });
    const root = join(directory, execution.output, 'provider-waits');
    const env = { STACK_BENCH_PROVIDER_WAIT_CONTEXT: JSON.stringify({ directory, campaignSha256: plan.contentSha256,
      attemptId, executionId: execution.id, ownershipMarkerSha256: marker, root }) };
    const evidence = { actionId: 'action-1', invocation: 1, costUsd: 0.5 };
    persistCampaignProviderInvocation({ env, evidence });
    assert.throws(() => persistCampaignProviderInvocation({ env, evidence }), /EEXIST/);
    const before = readFileSync(initialized.paths.state, 'utf8');
    let validations = 0;
    const result = waitForCampaignProviderContinuation({ env, evidence,
      validate: () => { validations++; }, sleep: () => {
        assert.equal(readCampaignProviderContinuationStatus(directory, attemptId).eligible, true);
        const request = requestCampaignProviderContinuation(directory, { attemptId, requestId: 'continue-1' });
        assert.deepEqual(requestCampaignProviderContinuation(directory, { attemptId, requestId: 'continue-1' }), request);
      } });
    assert(result);
    assert.equal(validations, 2);
    assert.equal(JSON.parse(readFileSync(join(root, 'events', `${result.generation}.accepted.json`), 'utf8')).requestId, 'continue-1');
    assert.equal(JSON.parse(readFileSync(join(root, 'events', `${result.generation}.waiting.json`), 'utf8')).disposition, 'waiting');
    assert.equal(readCampaignProviderContinuationStatus(directory, attemptId).waiting, false);
    assert.equal(readFileSync(initialized.paths.state, 'utf8'), before);
    assert.throws(() => waitForCampaignProviderContinuation({ env, evidence, validate: () => {}, sleep: () => {
      requestCampaignProviderContinuation(directory, { attemptId, requestId: 'continue-1' });
    } }), /different wait generation/);
    const attempt = claimed.state.attempts.find(a => a.plan.id === attemptId)!;
    const originalMinutes = plan.definition.budgets.attemptTimeoutMinutes;
    execution.startedAt = new Date(Date.now() - (originalMinutes + 1) * 60_000).toISOString();
    claimed.state.createdAt = execution.startedAt;
    writeCampaignState(initialized.paths.state, plan, claimed.state);
    assert.match(readCampaignProviderContinuationStatus(directory, attemptId).reason ?? '', /duration/);
    assert.throws(() => requestCampaignProviderContinuation(directory, { attemptId, requestId: 'after-deadline' }), /duration/);
    const grantAt = new Date().toISOString();
    claimed.state.updatedAt = grantAt;
    attempt.timeGrants = [{ disposition: 'accepted', acceptedAt: grantAt,
      previousMinutes: originalMinutes, effectiveMinutes: originalMinutes + 60,
      request: { attemptId, executionId: execution.id, campaignSha256: plan.contentSha256,
        grantId: 'more-time', minutes: 60, requestedAt: grantAt } }];
    writeCampaignState(initialized.paths.state, plan, claimed.state);
    assert(waitForCampaignProviderContinuation({ env, evidence, validate: () => {}, sleep: () => {
      requestCampaignProviderContinuation(directory, { attemptId, requestId: 'after-grant' });
    } }));
    const lockPath = join(directory, '.campaign.lock.json');
    const lock = JSON.parse(readFileSync(lockPath, 'utf8'));
    writeFileSync(lockPath, JSON.stringify({ ...lock, ownerPid: 2_147_483_647 }));
    assert.equal(readCampaignProviderContinuationStatus(directory, attemptId).eligible, false);
    assert.throws(() => requestCampaignProviderContinuation(directory, { attemptId, requestId: 'continue-2' }), /active/);
    persistCampaignProviderInvocation({ env, evidence: { ...evidence, invocation: 2 } });
    assert.equal(JSON.parse(readFileSync(join(root, 'invocations', 'action-1-2.json'), 'utf8')).evidence.costUsd, 0.5);
    assert.throws(() => persistCampaignProviderInvocation({ env: { STACK_BENCH_PROVIDER_WAIT_CONTEXT:
      JSON.stringify({ ...JSON.parse(env.STACK_BENCH_PROVIDER_WAIT_CONTEXT), root: join(directory, 'other') }) },
    evidence: { ...evidence, invocation: 3 } }), /does not belong/);
    const killedAt = Date.now();
    const killed = { ...JSON.parse(readFileSync(join(root, 'current.json'), 'utf8')),
      generation: randomUUID(), disposition: 'waiting',
      createdAt: new Date(killedAt - 90_000).toISOString(), heartbeatAt: new Date(killedAt - 60_000).toISOString() };
    writeFileSync(join(root, 'events', `${killed.generation}.waiting.json`), JSON.stringify(killed));
    writeFileSync(join(root, 'current.json'), JSON.stringify(killed));
    const history = readCampaignProviderWaitHistory(directory, attemptId, execution.id);
    assert(history);
    assert.equal(history.waits, 4);
    assert.equal(history.continued, 2);
    assert.equal(history.stopped, 2);
    assert.equal(history.durationKind, 'lower-bound');
    assert.equal(history.details.find(wait => wait.generation === killed.generation)?.waitedMs, 30_000);
    const acceptedPath = join(root, 'events', `${result.generation}.accepted.json`);
    writeFileSync(acceptedPath, JSON.stringify({ ...JSON.parse(readFileSync(acceptedPath, 'utf8')), executionId: 'foreign' }));
    assert.throws(() => readCampaignProviderWaitHistory(directory, attemptId, execution.id), /acceptance binding/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
