import assert from 'node:assert/strict';
import test from 'node:test';

import { parseCampaignArgs } from '../campaign-cli.mjs';

const argv = (...args) => ['node', 'campaign-cli.mjs', ...args];

test('campaign CLI separates read-only, preparation, execution, and status commands', () => {
  assert.equal(parseCampaignArgs(argv('show', './campaign.json')).command, 'show');
  assert.equal(parseCampaignArgs(argv('prepare', './campaign.json', '--out', './results')).command,
    'prepare');
  assert.equal(parseCampaignArgs(argv('trial', './campaign.json', '--out', './results')).command,
    'trial');
  assert.equal(parseCampaignArgs(argv('run', './campaign.json', '--out', './results')).command, 'run');
  assert.equal(parseCampaignArgs(argv('reconcile', './campaign.json', '--out', './results')).command,
    'reconcile');
  assert.equal(parseCampaignArgs(argv('status', './results')).command, 'status');
  assert.equal(parseCampaignArgs(argv('report', './results')).command, 'report');
  assert.throws(() => parseCampaignArgs(argv('run', './campaign.json')), /usage/);
  assert.throws(() => parseCampaignArgs(argv('run', './campaign.json', '--out', './a', '--out', './b')),
    /usage/);
});
