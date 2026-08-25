import assert from 'node:assert/strict';
import test from 'node:test';

import { parseCampaignArgs, validateResumeCampaignState } from '../commands/campaign-cli.mjs';

const argv = (...args) => ['node', 'campaign-cli.mjs', ...args];

test('campaign CLI separates read-only, preparation, execution, and status commands', () => {
  assert.equal(parseCampaignArgs(argv('modes')).command, 'modes');
  assert.equal(parseCampaignArgs(argv('show', './campaign.json')).command, 'show');
  assert.equal(parseCampaignArgs(argv('prepare', './campaign.json', '--out', './results')).command,
    'prepare');
  assert.equal(parseCampaignArgs(argv('trial', './campaign.json', '--out', './results')).command,
    'trial');
  assert.equal(parseCampaignArgs(argv('run', './campaign.json', '--out', './results')).command, 'run');
  assert.equal(parseCampaignArgs(argv('resume', './campaign.json', '--out', './results')).command,
    'resume');
  assert.equal(parseCampaignArgs(argv('reconcile', './campaign.json', '--out', './results')).command,
    'reconcile');
  assert.equal(parseCampaignArgs(argv('status', './results')).command, 'status');
  assert.equal(parseCampaignArgs(argv('inspect', './results')).command, 'inspect');
  assert.equal(parseCampaignArgs(argv('report', './results')).command, 'report');
  assert.throws(() => parseCampaignArgs(argv('run', './campaign.json')), /usage/);
  assert.throws(() => parseCampaignArgs(argv('run', './campaign.json', '--out', './a', '--out', './b')),
    /usage/);
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
  }), /interrupted dependency campaign/);
  assert.throws(() => validateResumeCampaignState(requested, {
    ...existing, state: { ...existing.state, status: 'running' },
  }), /interrupted dependency campaign/);
});
