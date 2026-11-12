import assert from 'node:assert/strict';
import test from 'node:test';
import { chromium } from 'playwright';

// Run against an isolated appliance with its setup presets installed:
// STACK_BENCH_BROWSER_TEST_URL=http://127.0.0.1:7331 node --test dist/tests/dashboard/form-navigation.browser.js
// All review submissions are intercepted. This test cannot start a model run.
test('form errors stay on their page, including responses received after navigation', async t => {
  const origin = process.env.STACK_BENCH_BROWSER_TEST_URL;
  assert.ok(origin, 'set STACK_BENCH_BROWSER_TEST_URL to the isolated test dashboard');
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  const page = await browser.newPage();
  let release: (() => void) | undefined;
  let delayed = false;
  await page.route('**/api/runs/prepare', async route => {
    if (delayed) await new Promise<void>(resolve => { release = resolve; });
    await route.fulfill({ status: 400, contentType: 'application/json',
      body: JSON.stringify({ error: 'Navigation test: no account' }) });
  });
  await page.goto(`${origin}/new`);
  await page.getByRole('button', { name: 'Review run', exact: true }).click();
  await page.getByRole('alert').filter({ hasText: 'Navigation test: no account' }).waitFor();
  await page.getByRole('link', { name: 'Campaigns', exact: true }).first().click();
  await page.getByRole('link', { name: 'New run', exact: true }).click();
  assert.equal(await page.getByText('Navigation test: no account', { exact: true }).count(), 0);

  delayed = true;
  const submitted = page.waitForRequest('**/api/runs/prepare');
  await page.getByRole('button', { name: 'Review run', exact: true }).click();
  await submitted;
  await page.getByRole('link', { name: 'Campaigns', exact: true }).first().click();
  const received = page.waitForResponse('**/api/runs/prepare');
  assert.ok(release, 'the review request reached the interceptor');
  release();
  await received;
  await page.getByRole('link', { name: 'New run', exact: true }).click();
  await page.getByRole('button', { name: 'Review run', exact: true }).waitFor();
  assert.equal(await page.getByText('Navigation test: no account', { exact: true }).count(), 0);
});
