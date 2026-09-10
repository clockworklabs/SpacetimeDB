/// <reference lib="dom" />
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { createDashboardServer } from '../dashboard/dashboard-server.js';
import { campaignSheet } from '../dashboard/dashboard-views.js';
import { EXAMPLE_CAMPAIGN, writeCampaign, writeRunEvidence } from './fixtures/dashboard-fixture.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { claimNextAttempt, createCampaignState, finishCampaignExecution } from '../src/campaigns/campaign-scheduler.js';

test('dashboard refresh follows child events, coalesces bursts, and refreshes cached tabs', async t => {
  const root = mkdtempSync(join(tmpdir(), 'dashboard-refresh-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const key = 'fixture-run-0';
  const plan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const claimed = claimNextAttempt(createCampaignState(plan), { admissionId: 'test' });
  assert.ok(claimed.claim);
  const state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 0, run: { outcome: { kind: 'passed' } } });
  const campaign = join(root, 'campaigns', key);
  writeCampaign(campaign, plan, state);
  writeRunEvidence(join(campaign, claimed.claim.output), plan, claimed.claim.attempt, 0);
  const attemptId = campaignSheet(root, key).stacks[0]!.attempts[0]!.id;
  const { server } = createDashboardServer({ resultsRoot: root, plansRoot: join(root, 'plans'),
    allowLaunch: false, token: 'test' });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => server.close());
  const address = server.address();
  assert.ok(address && typeof address !== 'string');
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  const page = await browser.newPage();
  await page.addInitScript(() => {
    const target = window as unknown as { events: EventTarget; hidden: boolean; EventSource: unknown };
    target.hidden = false;
    Object.defineProperty(document, 'hidden', { get: () => target.hidden });
    target.EventSource = class extends EventTarget {
      constructor() { super(); target.events = this; }
    };
  });
  const counts = new Map<string, number>();
  let hold: Promise<void> | null = null;
  let transcriptHold: Promise<void> | null = null;
  let release = (): void => {};
  await page.route('**/api/**', async request => {
    const path = new URL(request.request().url()).pathname;
    counts.set(path, (counts.get(path) ?? 0) + 1);
    if (path === `/api/campaigns/${key}` && hold) await hold;
    if (path.endsWith('/transcript')) {
      if (transcriptHold) await transcriptHold;
      const session = new URL(request.request().url()).searchParams.get('session') || 'first';
      await request.fulfill({ json: { sessions: [{ id: 'first', label: 'First' }, { id: 'second', label: 'Second' }],
        session, before: null, skipped: 0, messages: [{ id: session, role: 'assistant', text: `${session} message`, tool: false }] } });
      return;
    }
    await request.continue();
  });
  const idle = () => page.locator('main[aria-busy="false"]').waitFor();
  const change = (type = 'log', id: string | null = attemptId) => page.evaluate(({ type, key, id }) => {
    (window as unknown as { events: EventTarget }).events.dispatchEvent(
      new MessageEvent(type, { data: JSON.stringify({ key, ...(id ? { attemptId: id } : {}) }) }));
  }, { type, key, id });
  await page.goto(`http://127.0.0.1:${address.port}/c/${key}`);
  await idle();
  const campaignPath = `/api/campaigns/${key}`;
  const before = counts.get(campaignPath)!;
  hold = new Promise<void>(resolve => { release = resolve; });
  const started = page.waitForRequest(request => new URL(request.url()).pathname === campaignPath);
  await change();
  await started;
  await Promise.all([change(), change(), change()]);
  assert.equal(counts.get(campaignPath), before + 1);
  hold = null;
  release();
  await page.waitForResponse(response => new URL(response.url()).pathname === campaignPath);
  await idle();
  assert.equal(counts.get(campaignPath), before + 2);

  for (const [tab, endpoint] of [['checks', 'checks'], ['files', 'package']] as const) {
    await page.goto(`http://127.0.0.1:${address.port}/c/${key}/a/${attemptId}?tab=${tab}`);
    await idle();
    const path = `${campaignPath}/attempts/${attemptId}/${endpoint}`;
    const previous = counts.get(path)!;
    const updated = page.waitForResponse(response => new URL(response.url()).pathname === path);
    await change('campaign', null);
    await updated;
    await idle();
    assert.equal(counts.get(path), previous + 1);
  }
  const previous = counts.get(campaignPath)!;
  await page.evaluate(() => { (window as unknown as { hidden: boolean }).hidden = true; });
  await change();
  assert.equal(counts.get(campaignPath), previous);
  const visible = page.waitForResponse(response => new URL(response.url()).pathname === campaignPath);
  await page.evaluate(() => {
    (window as unknown as { hidden: boolean }).hidden = false;
    document.dispatchEvent(new Event('visibilitychange'));
  });
  await visible;
  await idle();
  assert.equal(counts.get(campaignPath), previous + 1);

  await page.goto(`http://127.0.0.1:${address.port}/c/${key}/a/${attemptId}?tab=transcript`);
  await idle();
  transcriptHold = new Promise<void>(resolve => { release = resolve; });
  const reading = page.waitForRequest(request => new URL(request.url()).pathname.endsWith('/transcript'));
  await change();
  await reading;
  await page.locator('[data-transcript-session]').selectOption('second');
  const selected = page.waitForResponse(response => response.url().includes('session=second'));
  transcriptHold = null;
  release();
  await selected;
  await page.getByText('second message', { exact: true }).waitFor();
});
