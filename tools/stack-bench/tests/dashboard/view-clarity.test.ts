import assert from 'node:assert/strict';
import test from 'node:test';
import type { CampaignProgression, CampaignSheet, SheetAttempt } from '../../dashboard/dashboard-views.js';
import { campaignPage, replayTimeline, selectedProgression } from '../../dashboard/public/views/campaign.js';
import { attemptPage } from '../../dashboard/public/views/attempt.js';

// Distinct aggregate and selected values catch accidental cross-repetition labels.
test('campaign separates aggregate scores from selected evidence and explains pending tabs', () => {
  const attempt: SheetAttempt = {
    id: 'selected', repetition: 2, status: 'running', phase: 'building', stalling: false,
    excluded: null, continued: false, logUpdatedAt: null, score: null, unaided: null,
    repairs: { used: 2, budget: 10 }, timeSec: null, executionStartedAt: null,
    executionCompletedAt: null, spendPending: true, spend: { status: 'unknown', costUsd: null },
    completion: null, variant: 'neutral', climb: [],
  };
  const sheet: CampaignSheet = {
    key: 'example', id: 'example', title: 'Example', status: 'running', mode: 'dependency',
    levels: [1], repetitions: 2, provisional: true, mixedScope: false, executions: 1,
    resumable: false, createdAt: '', updatedAt: '',
    facts: { mode: 'dependency', workSelection: 'progressive', repairSelection: 'feature',
      repairLimits: { perFeature: 5 }, agent: null, model: null,
      guidance: 'neutral', recipes: [], timeLimitMinutes: 240, spendLimitUsd: 50,
      controllerImage: null, buildImage: null, planSha256: 'example', grading: 'pending', gradingReasons: [] },
    stacks: [{ stack: 'spacetime', selectedAttemptId: attempt.id, score: 82, points: null,
      unaided: null, continued: false, regressions: 0,
      timeSec: null, spend: attempt.spend, spendPending: true, completionRate: null, n: 1,
      climb: [], attempts: [attempt], levels: null,
      questlines: [{ id: 'catalog', title: 'Catalog', score: 20, nodes: [] }] }],
  };
  const page = campaignPage({ sheet, progression: null, view: 'grid', step: 0 });
  const selected = page.slice(page.indexOf('<h3>Selected repetition</h3>'));
  assert.match(page, /82%/);
  assert.match(selected, /20%/);
  assert.match(selected, /2<i>\/ 10<\/i>/);
  assert.doesNotMatch(selected, /82%|9<i>\/ 10|Questline average/);
  assert.match(page, /Usable results/);
  assert.match(page, /<nav aria-label="Feature progress view">/);
  assert.doesNotMatch(page.slice(0, page.indexOf('<section class="feature-progress"')), /\?questlines=/);
  assert.match(page, /popovertarget="help-completion"/);
  assert.match(page, /id="help-completion" popover role="tooltip"/);
  assert.doesNotMatch(page, /<details class="metric-help"/);
  for (const tab of ['checks', 'screenshots', 'files', 'log'] as const) {
    const detail = attemptPage({ sheet, attemptId: attempt.id, tab, checks: null, evidence: null, log: '' });
    assert.match(detail, /No (check results|screenshots|files|log output)/);
    assert.match(detail, /aria-current="page"/);
    assert.match(detail, /popovertarget="help-completion"/);
    assert.doesNotMatch(detail, /<details class="metric-help"/);
    assert.ok(detail.indexOf('About Completion') < detail.indexOf('About Weighted score'));
  }
  const progression: CampaignProgression = {
    key: sheet.key, depths: [1], questlines: [{ id: 'catalog', title: 'Catalog', nodes: ['item'] }],
    nodes: [{ id: 'item', title: 'Item', questline: 'catalog', depth: 1, dependencies: [] }],
    stacks: [],
  };
  sheet.stacks = ['spacetime', 'postgres', 'mongodb'].map(stack => ({
    ...sheet.stacks[0]!, stack, selectedAttemptId: `${stack}-2`,
  }));
  progression.stacks = sheet.stacks.flatMap(stack => [1, 2, 3].map(repetition => ({
    stack: stack.stack, attemptId: `${stack.stack}-${repetition}`, updatedAt: '',
    steps: [{ sequence: 1, action: repetition === 2 ? 'build' as const : 'repair' as const,
      targets: ['item'], statuses: [repetition === 2 ? 'passed' : 'failed'],
      score: repetition === 2 ? 100 : 99, repairs: repetition === 2 ? 0 : 77 }],
  })));
  const selectedTracks = selectedProgression(progression, sheet);
  assert.equal(selectedTracks.stacks.length, 3);
  assert.ok(selectedTracks.stacks.every(track => track.attemptId.endsWith('-2')));
  assert.equal(replayTimeline(selectedTracks).length, 3);
  for (const view of ['graph', 'replay'] as const) {
    const html = campaignPage({ sheet, progression, view, step: 99 });
    assert.doesNotMatch(html, /class="d f"|99%|>77</);
    assert.equal((html.match(/class="d p"/g) ?? []).length, 3);
    if (view === 'replay') assert.match(html, /3<i>\/ 3<\/i>/);
  }

});
