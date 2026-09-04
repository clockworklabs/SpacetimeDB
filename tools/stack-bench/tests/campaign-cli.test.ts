import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { auditCompletedReferenceCampaign, campaignStateSummary, parseCampaignArgs,
  validateResumeCampaignState } from '../commands/campaign-cli.js';

const argv = (...args: string[]): string[] => ['node', 'campaign-cli.js', ...args];

test('campaign CLI separates read-only, execution, and status commands', () => {
  assert.equal(parseCampaignArgs(argv('modes')).command, 'modes');
  assert.equal(parseCampaignArgs(argv('show', './campaign.json')).command, 'show');
  assert.equal(parseCampaignArgs(argv('trial', './campaign.json', '--out', './results')).command,
    'trial');
  assert.equal(parseCampaignArgs(argv('run', './campaign.json', '--out', './results')).command, 'run');
  assert.equal(parseCampaignArgs(argv('resume', './campaign.json', '--out', './results')).command,
    'resume');
  assert.equal(parseCampaignArgs(argv('reconcile', './campaign.json', '--out', './results')).command,
    'reconcile');
  assert.deepEqual(parseCampaignArgs(argv('status', './results')), {
    command: 'status', directory: resolve('./results'), full: false,
  });
  assert.deepEqual(parseCampaignArgs(argv('status', './results', '--full')), {
    command: 'status', directory: resolve('./results'), full: true,
  });
  assert.equal(parseCampaignArgs(argv('inspect', './results')).command, 'inspect');
  assert.equal(parseCampaignArgs(argv('report', './results')).command, 'report');
  assert.equal(parseCampaignArgs(argv('audit', './results')).command, 'audit');
  assert.deepEqual(parseCampaignArgs(argv('grant-repairs', './results',
    '--attempt', 'campaign-r1', '--grant-id', 'grant-1', '--level', '3',
    '--feature', 'orders', '--feature', 'inventory', '--repairs', '2')), {
    command: 'grant-repairs', directory: resolve('./results'),
    attemptId: 'campaign-r1', grantId: 'grant-1', level: 3,
    nodeIds: ['orders', 'inventory'], repairs: 2,
  });
  assert.deepEqual(parseCampaignArgs(argv('extend', './depth-3.json', '--from', './depth-2',
    '--depth', '2', '--out', './depth-3')), {
    command: 'extend', path: resolve('./depth-3.json'), parentDirectory: resolve('./depth-2'),
    fromDepth: 2, directory: resolve('./depth-3'),
  });
  assert.throws(() => parseCampaignArgs(argv('run', './campaign.json')), /usage/);
  assert.throws(() => parseCampaignArgs(argv('prepare', './campaign.json', '--out', './results')),
    /usage/);
  assert.throws(() => parseCampaignArgs(argv('run', './campaign.json', '--out', './a', '--out', './b')),
    /usage/);
  assert.throws(() => parseCampaignArgs(argv('status', './results', '--json')), /usage/);
  assert.throws(() => parseCampaignArgs(argv('grant-repairs', './results',
    '--attempt', 'campaign-r1', '--level', '3', '--feature', 'orders', '--repairs', '2')),
  /requires --attempt, --grant-id/);
});

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
