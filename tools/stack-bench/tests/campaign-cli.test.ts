import assert from 'node:assert/strict';
import test from 'node:test';

import { auditCompletedReferenceCampaign, campaignStateSummary, parseCampaignArgs,
  validateResumeCampaignState } from '../commands/campaign-cli.js';
import { SAFE_RESULT_NAME } from '../src/runtime/operational-paths.js';

test('campaign commands print a compact result and retain failed attempt details', () => {
  const plan = { id: 'campaign', version: '1.0.0', contentSha256: 'a'.repeat(64) };
  const state = {
    status: 'attention-required',
    summary: { total: 3, completed: 2, invalid: 1, executions: 3 },
    attempts: [
      { plan: { id: 'passed' }, status: 'completed', executions: [
        { id: 'passed-execution1', outcome: 'passed', reason: null },
      ] },
      { plan: { id: 'application-failure' }, status: 'completed', executions: [
        { id: 'application-failure-execution1', outcome: 'app_failure', reason: null },
      ] },
      { plan: { id: 'invalid' }, status: 'invalid', executions: [
        { id: 'invalid-execution1', outcome: 'harness_failure', reason: 'grader stopped' },
      ] },
    ],
  };

  const summary = campaignStateSummary(plan, state);
  assert.equal(summary.status, 'needs attention');
  assert.deepEqual(summary.summary, state.summary);
  assert.deepEqual(summary.failures.map(failure => [failure.attempt, failure.outcome]), [
    ['application-failure', 'application failure'],
    ['invalid', 'harness failure'],
  ]);
  assert.equal(JSON.stringify(summary).includes('passed-execution1'), false);
});

test('automatic reference audit runs only after campaign completion', () => {
  let calls = 0;
  const plan = { attempts: [{ mode: { id: 'dependency' }, agentAdapter: 'reference-fixture' }] };
  const audit = (directory: string) => {
    calls += 1;
    assert.equal(directory, 'results');
    return { ok: true };
  };
  assert.equal(auditCompletedReferenceCampaign('results', plan, { status: 'running' }, { audit }),
    null);
  assert.deepEqual(auditCompletedReferenceCampaign('results', plan, { status: 'completed' },
    { audit }), { ok: true });
  assert.equal(auditCompletedReferenceCampaign('results', { attempts: [] },
    { status: 'completed' }, { audit }), null);
  assert.equal(calls, 1);
});

test('campaign CLI resumes only an existing matching dependency campaign', () => {
  const sha256 = 'a'.repeat(64);
  const requested = { contentSha256: sha256 };
  const existing = {
    plan: { contentSha256: sha256, definition: { mode: { id: 'dependency' } } },
    state: { status: 'prepared', attempts: [{ executions: [{}] }] },
  };
  assert.equal(validateResumeCampaignState(requested, existing), existing);
  assert.throws(() => validateResumeCampaignState({ contentSha256: 'b'.repeat(64) }, existing),
    /exact campaign plan/);
  assert.throws(() => validateResumeCampaignState(requested, {
    ...existing, plan: { ...existing.plan, definition: { mode: { id: 'sequential' } } },
  }), /only for dependency/);
  assert.throws(() => validateResumeCampaignState(requested, {
    ...existing, state: { status: 'prepared', attempts: [{ executions: [] }] },
  }), /scheduled work/);
  assert.throws(() => validateResumeCampaignState(requested, {
    ...existing, state: { ...existing.state, status: 'running' },
  }), /scheduled work/);
});

test('new campaign folders must use a name the dashboard can open', () => {
  for (const out of ['campaigns/Pilot_1', 'campaigns/l3']) {
    assert.throws(() => parseCampaignArgs(['node', 'x', 'run', 'p.json', '--out', out]), /folder name/);
    assert.throws(() => parseCampaignArgs(['node', 'x', 'extend', 'p.json', '--from', 'campaigns/a1', '--depth', '2',
      '--out', out]), /folder name/);
    assert.equal(SAFE_RESULT_NAME.test(out.slice('campaigns/'.length)), false);
  }
  const trial = parseCampaignArgs(['node', 'x', 'trial', 'p.json', '--out', 'campaigns/pilot-1']) as { directory: string };
  assert.match(trial.directory, /pilot-1$/);
  // Resume and reconcile act on folders that already exist.
  assert.doesNotThrow(() => parseCampaignArgs(['node', 'x', 'resume', 'p.json', '--out', 'campaigns/Pilot_1']));
});
