/// <reference lib="dom" />
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { createDashboardServer } from '../dashboard/dashboard-server.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

// Failure cases: dead navigation; wrong default scope; missing repeated work;
// unreadable source; filters lost on reload; blank results; mobile overflow;
// script injection through search; accidental campaign/model work from this page.
test('check guide exposes current procedures through the real dashboard', async () => {
  const evidence = join(STACK_BENCH_ROOT, 'local-notes', 'dashboard-check-guide-evidence',
    new Date().toISOString().replace(/[:.]/g, '-'));
  mkdirSync(evidence, { recursive: true });
  const { server } = createDashboardServer({ resultsRoot: join(evidence, 'results'),
    plansRoot: join(evidence, 'plans'), allowLaunch: false, token: 'test' });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  assert(address && typeof address !== 'string');
  const browser = await chromium.launch();
  let result = 'failed';
  const errors: string[] = [], api: string[] = [];
  let count = '';
  let definition = '';
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 950 } });
    page.on('pageerror', error => errors.push(String(error)));
    page.on('request', request => { if (new URL(request.url()).pathname.startsWith('/api/')) api.push(request.url()); });
    await page.goto(`http://127.0.0.1:${address.port}/checks`);
    await page.getByRole('heading', { name: 'Checks', exact: true }).waitFor();
    count = await page.locator('[data-guide-count]').innerText();
    definition = await page.locator('.guide-help .guide-key').textContent() ?? '';
    assert.equal(await page.getByText(/See the exact input below for its fields/).count(), 0,
      'all current actions have a readable description');
    assert((await page.locator('.guide-check:visible').count()) > 0);
    assert.equal(await page.locator('.guide-check[data-active="false"]:visible').count(), 0);
    await page.getByLabel('Search checks').fill('catalog-volume');
    assert.equal(await page.locator('.guide-check:visible').count(), 1);
    await page.getByRole('button', { name: 'Expand visible' }).click();
    const check = page.locator('.guide-check:visible');
    assert.match(await check.innerText(), /Repeat 1000 times/);
    await check.getByText('Technical details', { exact: true }).click();
    await page.evaluate(() => window.scrollTo(0, 0));
    const raw = JSON.parse(await check.locator('pre').innerText()) as {
      criterion: { steps: { forEach?: unknown[] }[] };
    };
    assert.equal(raw.criterion.steps.find(step => step.forEach)?.forEach?.length, 1000);
    assert.match(await check.innerText(), /dbExpectCatalogItem|committed-write barrier/);
    await check.getByText('Technical details', { exact: true }).click();
    await page.screenshot({ path: join(evidence, 'desktop.png') });
    await page.reload();
    assert.equal(await page.getByLabel('Search checks').inputValue(), 'catalog-volume');
    await page.getByLabel('Search checks').fill('');
    await page.getByLabel('Include other selections').check();
    assert((await page.locator('.guide-check[data-active="false"]:visible').count()) > 0);
    await page.getByLabel('Search checks').fill('<img src=x onerror=alert(1)>');
    await page.getByText('No checks match this search.', { exact: true }).waitFor();
    assert.equal(await page.locator('.guide img').count(), 0);
    await page.getByLabel('Search checks').fill('catalog-volume');
    await page.getByRole('button', { name: 'Expand visible' }).click();
    await page.setViewportSize({ width: 390, height: 844 });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.screenshot({ path: join(evidence, 'mobile.png') });
    assert.deepEqual(api, [], 'guide needs no campaign reads or model calls');
    await page.getByRole('link', { name: 'Campaigns', exact: true }).click();
    await page.getByRole('link', { name: 'Checks', exact: true }).click();
    await page.getByRole('heading', { name: 'Checks', exact: true }).waitFor();
    assert.deepEqual(errors, []);
    result = 'passed';
  } finally {
    await browser.close();
    await new Promise<void>(resolve => server.close(() => resolve()));
    writeFileSync(join(evidence, 'receipt.json'), JSON.stringify({ result, count, definition, errors,
      runtime: { node: process.version, chromium: browser.version() },
      command: 'node --test dist/tests/dashboard-check-guide.integration.js',
      inputs: 'Current local definitions; catalog-volume search; 1280px and 390px viewports',
      browserClosed: true, serverClosed: true,
    }, null, 2));
  }
});
