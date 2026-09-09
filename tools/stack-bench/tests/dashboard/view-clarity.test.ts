import assert from 'node:assert/strict';
import test from 'node:test';
import { spend } from '../../dashboard/public/format.js';
import type { CampaignProgression, CampaignSheet, SheetAttempt } from '../../dashboard/dashboard-views.js';
import { campaignPage, replayTimeline, selectedProgression } from '../../dashboard/public/views/campaign.js';
import { attemptPage } from '../../dashboard/public/views/attempt.js';
import { topbar } from '../../dashboard/public/views/plans.js';

test('report links appear only for files present in the campaign', () => {
  const input = { page: 'campaign' as const, key: 'example', canStart: false, resumable: false, error: '' };
  assert.doesNotMatch(topbar(input), />report<|>export manifest</);
  const reportFiles = ['report/report.html'];
  assert.match(topbar({ ...input, reportFiles }), />report</);
  assert.doesNotMatch(topbar({ ...input, reportFiles }), />export manifest</);
  assert.match(topbar({ ...input, reportFiles: [...reportFiles, 'report/export-manifest.json'] }), />export manifest</);
});

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
    stacks: [{ stack: 'spacetime', costPerValidRun: 6, selectedAttemptId: attempt.id, score: 82, points: null,
      unaided: null, continued: false, regressions: 0,
      timeSec: null, spend: attempt.spend, spendPending: true, completionRate: null, n: 1,
      attempts: [attempt], levels: null,
      questlines: [{ id: 'catalog', title: 'Catalog', score: 20, nodes: [] }] }],
  };
  const page = campaignPage({ sheet, progression: null, view: 'grid', step: 0 });
  const selected = page.slice(page.indexOf('<h3>Runs</h3>'));
  assert.match(page, /82%/);
  assert.match(selected, /20%/);
  assert.match(selected, /2<i>\/ 10<\/i>/);
  assert.doesNotMatch(selected, /82%|9<i>\/ 10|Questline average/);
  assert.match(page, /Valid runs/);
  const metricsTable = page.split('<table class="sheet">')[1]!.split('</table>')[0]!;
  for (const label of ['Cost per valid run', 'Weighted score', 'Before repairs', 'Regressions', 'Time', 'Valid runs', 'Excluded', 'Total spend']) {
    assert.ok(metricsTable.includes(label), `${label} belongs in the comparison table`);
  }
  assert.match(metricsTable, /\$6\.00/);
  assert.ok(metricsTable.indexOf('Cost per valid run') < metricsTable.indexOf('Total spend'));
  assert.doesNotMatch(page, /Unaided/);
  assert.match(page, /Later levels retain earlier fixes and feedback/);
  assert.doesNotMatch(page, /<h3>Selected repetition<\/h3>/);
  assert.ok(page.indexOf('<h3>Results</h3>') < page.indexOf('<h3>Runs</h3>'));
  assert.doesNotMatch(page, /More comparison metrics/);
  assert.doesNotMatch(page, /<summary>Configuration and provenance<\/summary>/);
  assert.doesNotMatch(page, /class="label">Qualification/);
  assert.match(page, /<summary>Explore · grid<\/summary>/);
  assert.match(page, /<nav aria-label="Feature progress view">/);
  assert.doesNotMatch(page.slice(0, page.indexOf('<section class="feature-progress"')), /aria-label="Feature progress view"/);
  assert.match(page, /popovertarget="help-completion"/);
  assert.match(page, /id="help-completion" popover role="tooltip"/);
  assert.doesNotMatch(page, /<details class="metric-help"/);
  for (const tab of ['checks', 'screenshots', 'files', 'log'] as const) {
    const detail = attemptPage({ sheet, attemptId: attempt.id, tab, checks: null, evidence: null, log: '' });
    assert.doesNotMatch(detail, /Unaided/);
    assert.match(detail, /Later levels retain earlier fixes and feedback/);
    assert.match(detail, /No (check results|screenshots|files|log output)/);
    assert.match(detail, /aria-current="page"/);
    assert.match(detail, /popovertarget="help-completion"/);
    assert.doesNotMatch(detail, /<details class="metric-help"/);
    assert.ok(detail.indexOf('About Completion') < detail.indexOf('About Weighted score'));
  }
  const grantInput = { sheet, attemptId: attempt.id, tab: 'checks' as const,
    checks: null, evidence: null, log: '', canControl: true };
  const request = { campaignSha256: 'a'.repeat(64), attemptId: attempt.id,
    executionId: 'execution-1', grantId: 'extension-1', minutes: 120, requestedAt: '2026-09-07T00:00:00.000Z' };
  const budget = { originalMinutes: 240, effectiveMinutes: 240, consumedMs: 0, liveGrantSupported: true,
    extensionCount: 0, grants: [{ request, disposition: 'pending' as const }] };
  const pending = attemptPage({ ...grantInput, timeBudget: budget });
  assert.match(pending, /disabled>Awaiting controller/);
  assert.match(pending, /limit has not changed yet/);
  const accepted = attemptPage({ ...grantInput, timeBudget: { ...budget,
    effectiveMinutes: 360, extensionCount: 1,
    grants: [{ request, disposition: 'accepted', effectiveMinutes: 360 }] } });
  assert.match(accepted, /Time added. Limit: 6h 0m/);
  assert.match(accepted, /name="minutes" type="number" min="1" step="1"/);
  assert.doesNotMatch(attemptPage({ ...grantInput, canControl: false }), /data-run="grant-time"/);
  attempt.status = 'invalid';
  assert.doesNotMatch(attemptPage(grantInput), /data-run="grant-time"/);
  assert.match(attemptPage({ ...grantInput, timeBudget: { ...budget, grants: [],
    continuation: { eligible: true } } }), /Add time and resume/);
  assert.match(attemptPage({ ...grantInput, timeBudget: { ...budget, grants: [],
    continuation: { eligible: false, reason: 'Coding session was interrupted.' } } }),
  /Cannot resume: Coding session was interrupted/);
  const transcriptHtml = attemptPage({ ...grantInput, tab: 'transcript', transcript: {
    sessions: [{ id: 'one', label: 'Session 1' }], session: 'one', before: 100, skipped: 0,
    messages: [{ id: 'm1', role: 'assistant', text: '<script>alert(1)</script>', tool: false },
      { id: 'm2', role: 'Bash', text: 'echo hello', tool: true }],
  } });
  assert.match(transcriptHtml, /&lt;script&gt;/);
  assert.doesNotMatch(transcriptHtml, /<script>|<details[^>]*open/);
  assert.match(transcriptHtml, /data-transcript-before="100"/);
  attempt.status = 'running';
  const progression: CampaignProgression = {
    key: sheet.key, depths: [1], questlines: [{ id: 'catalog', title: 'Catalog', nodes: ['item'] }],
    nodes: [{ id: 'item', title: 'Item', questline: 'catalog', depth: 1, dependencies: [] }],
    stacks: [],
  };
  sheet.stacks = ['spacetime', 'postgres', 'mongodb', 'custom-sql', 'custom-kv'].map(stack => ({
    ...sheet.stacks[0]!, stack, selectedAttemptId: `${stack}-2`,
  }));
  progression.stacks = sheet.stacks.flatMap(stack => [1, 2, 3].map(repetition => ({
    stack: stack.stack, attemptId: `${stack.stack}-${repetition}`, updatedAt: '',
    steps: [{ sequence: 1, action: repetition === 2 ? 'build' as const : 'repair' as const,
      targets: ['item'], statuses: [repetition === 2 ? 'passed' : 'failed'],
      score: repetition === 2 ? 100 : 99, repairs: repetition === 2 ? 0 : 77 }],
  })));
  const selectedTracks = selectedProgression(progression, sheet);
  const gridPage = campaignPage({ sheet, progression, view: 'grid', step: 0 });
  const comparison = gridPage.split('<table class="sheet">')[1]!.split('</table>')[0]!;
  assert.equal((comparison.split('</thead>')[0]!.match(/scope="col"/g) ?? []).length, 6);
  assert.match(comparison, /custom-sql/);
  assert.match(comparison, /custom-kv/);
  assert.equal(selectedTracks.stacks.length, 5);
  assert.ok(selectedTracks.stacks.every(track => track.attemptId.endsWith('-2')));
  assert.equal(replayTimeline(selectedTracks).length, 5);
  for (const view of ['graph', 'replay'] as const) {
    const html = campaignPage({ sheet, progression, view, step: 99 });
    assert.doesNotMatch(html, /class="d f"|99%|>77</);
    assert.equal((html.match(/class="d p"/g) ?? []).length, 5);
    if (view === 'replay') assert.match(html, /5<i>\/ 5<\/i>/);
  }

});

test('pending cost has an accessible activity dot without changing the amount', () => {
  const unknown = { status: 'unknown' as const, costUsd: null };
  assert.equal(spend(unknown), 'Unknown');
  assert.match(spend(unknown, true), /Unknown.*aria-label="Cost still updating"/);
  assert.doesNotMatch(spend(unknown, true), /so far|partial/);
});
