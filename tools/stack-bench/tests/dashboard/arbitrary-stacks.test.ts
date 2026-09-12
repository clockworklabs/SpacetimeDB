import assert from 'node:assert/strict';
import test from 'node:test';
import type { OverviewCampaign } from '../../dashboard/dashboard-views.js';
import { campaignsPage } from '../../dashboard/public/views/campaigns.js';
import { compareCampaign } from '../../dashboard/public/metrics.js';

test('campaign history derives all stack columns from data, including new stacks', () => {
  const stacks = ['custom-a', 'postgres', 'custom-b', 'spacetime', 'mongodb'];
  const campaign: OverviewCampaign = {
    key: 'five-stacks', id: 'five-stacks', title: 'Five stacks', status: 'completed',
    mode: 'dependency', levels: [1], repetitions: 1, provisional: false, updatedAt: null,
    scores: Object.fromEntries(stacks.map((stack, index) => [stack, 90 + index])),
    attempts: { total: 5, running: 0, completed: 5 },
  };
  const input = { campaigns: [campaign], sheets: [] };
  const html = campaignsPage({ ...input, filter: 'all' });
  assert.equal((html.match(/<th class="stack">/g) ?? []).length, stacks.length);
  assert.match(html, /<th class="stack">custom-a<\/th>/);
  assert.match(html, /<th class="stack">custom-b<\/th>/);
  assert.match(html, /<u>94%<\/u>/);
  assert.match(campaignsPage({ ...input, filter: 'ready' }), /colspan="9"/);
  assert.match(campaignsPage({ campaigns: [], sheets: [], filter: 'all' }), /colspan="4"/);

  const comparison = compareCampaign({ attempts: stacks.map(stack => ({
    id: stack, stack, status: 'pending', execution: null, result: null, dependency: null,
  })) });
  assert.deepEqual(comparison.rows.map(row => row.stack), stacks);
});
