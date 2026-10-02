import assert from 'node:assert/strict';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { AddressInfo } from 'node:net';
import test from 'node:test';
import { overviewPage } from '../../dashboard/dashboard-views.js';
import { createDashboardServer } from '../../dashboard/dashboard-server.js';
import { campaignsPage } from '../../dashboard/public/views/campaigns.js';
import { compileCampaignFile } from '../../src/campaigns/campaign-compiler.js';
import { createCampaignState, claimNextAttempt, finishCampaignExecution } from '../../src/campaigns/campaign-scheduler.js';
import { EXAMPLE_CAMPAIGN, writeCampaign } from '../fixtures/dashboard-fixture.js';

test('campaign paging filters the full population but inspects only visible evidence; reads do not block HTTP', async t => {
  const root = fs.mkdtempSync(join(tmpdir(), 'dashboard-paging-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const plan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const { state } = claimNextAttempt(createCampaignState(plan), { admissionId: 'test' });
  const execution = state.attempts[0]!.executions[0]!;
  for (let i = 0; i < 21; i++) {
    const directory = join(root, 'campaigns', `sample-${String(i).padStart(2, '0')}`);
    writeCampaign(directory, plan, state);
    fs.mkdirSync(join(directory, execution.output), { recursive: true });
    fs.writeFileSync(join(directory, execution.output, 'run.json'), '{}');
  }
  writeCampaign(join(root, 'campaigns', 'ready'), plan, createCampaignState(plan));
  writeCampaign(join(root, 'campaigns', 'broken'), plan, createCampaignState(plan));
  fs.writeFileSync(join(root, 'campaigns', 'broken', 'plan.json'), '{}');
  const reads: string[] = [];
  const originalRead = fs.readFileSync;
  t.mock.method(fs, 'readFileSync', (...args: Parameters<typeof fs.readFileSync>) => {
    if (String(args[0]).endsWith('run.json')) reads.push(String(args[0]));
    return originalRead(...args);
  });
  syncBuiltinESMExports();
  t.after(() => { t.mock.restoreAll(); syncBuiltinESMExports(); });
  const options = { controllerActive: () => true };
  const first = overviewPage(join(root, 'campaigns'), 1, 'all', options);
  assert.equal(first.campaigns.length, 20);
  assert.equal(first.total, 23);
  assert.equal(first.pages, 2);
  assert.equal(first.running.length, 21); // Live runs remain visible off-page.
  const second = overviewPage(join(root, 'campaigns'), 2, 'all', options);
  assert.equal(second.campaigns.length, 3);
  assert.equal(new Set([...first.campaigns, ...second.campaigns].map(item => item.key)).size, 23);
  assert.equal(second.campaigns.at(-1)!.status, 'unreadable');
  assert.equal(overviewPage(join(root, 'campaigns'), 99, 'all', options).page, 2);
  const ready = overviewPage(join(root, 'campaigns'), 2, 'ready', options);
  assert.equal(ready.total, 1);
  assert.equal(ready.page, 1);
  assert.equal(ready.counts.all, 23);
  assert.equal(ready.counts.attention, 1);
  const html = campaignsPage({ campaigns: first.campaigns, sheets: [], filter: 'all', pagination: first });
  assert.match(html, /Page 1 of 2 · 23 campaigns/);
  assert.match(html, /href="\/\?filter=all&page=2">Next/);
  assert.equal(reads.length, 0); // Running attempts have no comparable final score.
  const completed = finishCampaignExecution(state, execution.id, { exitCode: 0, run: { outcome: { kind: 'passed' } } });
  writeCampaign(join(root, 'campaigns', 'sample-00'), plan, completed);
  const updated = overviewPage(join(root, 'campaigns'), 1, 'all', options);
  assert.ok(reads.length > 0); // Completed scores still require valid evidence.
  const invalid = updated.campaigns.find(item => item.key === 'sample-00');
  assert.ok(invalid && 'scores' in invalid);
  assert.ok(Object.values(invalid.scores).every(score => score === null));
  fs.writeFileSync(join(root, 'campaigns', 'sample-00', execution.output, 'run.json'), '{"changed":true}');
  reads.length = 0;
  overviewPage(join(root, 'campaigns'), 1, 'attention', options);
  assert.equal(reads.length, 0); // Changed evidence outside this page is not inspected.
  t.mock.restoreAll(); syncBuiltinESMExports();

  const { server } = createDashboardServer({ resultsRoot: root, plansRoot: join(root, 'plans') });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise<void>(resolve => server.close(() => resolve())));
  const origin = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  // The cold worker is still reading when the main HTTP thread answers health.
  let finished = false;
  const overview = fetch(`${origin}/api/overview`).then(async response => {
    const value = await response.json(); finished = true; return value;
  });
  assert.equal((await fetch(`${origin}/api/health`)).status, 200);
  assert.equal(finished, false);
  assert.equal((await overview).campaigns.length, 20);
  assert.equal((await fetch(`${origin}/api/overview?page=0`)).status, 400);
  assert.equal((await fetch(`${origin}/api/overview?filter=unknown`)).status, 400);
  const empty = await (await fetch(`${origin}/api/overview?filter=completed`)).json();
  assert.equal(empty.total, 0);
  assert.deepEqual(empty.campaigns, []);
});
