import assert from 'node:assert/strict';
import test from 'node:test';

import { isFinding } from '../src/actions/action-findings.js';
import { browserApplicationBoundary, pageFailure } from '../src/actions/browser-action-executors.js';
import type { Finding } from '../src/actions/action-findings.js';
import { createActionRegistry, executeAction } from '../src/actions/action-contract.js';
import { chromium } from 'playwright';

function findingOf(message: string): Finding {
  const failure = pageFailure(message);
  assert(isFinding(failure.details.finding));
  return failure.details.finding;
}

test('a locator timeout names the awaited control, not its container', () => {
  const message = 'locator.waitFor: Timeout 5000ms exceeded.\nCall log:\n'
    + '  - waiting for locator(\'[data-role="category-row"],#category-row\').filter({ hasText: \'Audio\' })'
    + '.first().locator(\'[data-role="category-units"],#category-units\').first() to be visible\n';
  const finding = findingOf(message);
  assert.equal(finding.kind, 'page-timeout');
  assert.equal(finding.fields.control, 'category-units');
  assert.equal(pageFailure(message).message, 'the category-units control inside the category-row control did not become available in time');

  // A timeout without a named control stays a page timeout.
  const unnamed = findingOf('page.goto: Timeout 30000ms exceeded.');
  assert.equal(unnamed.kind, 'page-timeout');
  assert.equal(unnamed.fields.control, undefined);
  assert.equal(pageFailure('page.goto: Timeout 30000ms exceeded.').message, 'the page did not respond in time');

  // Alternative controls are not reported as nested controls.
  for (const operation of ['or', 'and']) {
    const alternative = `locator.waitFor: Timeout waiting for locator('[data-role="signup-username"]').${operation}(locator('[data-role="signup-toggle"]'))`;
    const failure = pageFailure(alternative);
    assert.doesNotMatch(failure.message, /inside/);
    if (operation === 'or') assert.match(failure.message, /signup-username, signup-toggle/);
    else assert.doesNotMatch(failure.message, /signup-toggle|signup-username/);
    const finding = findingOf(alternative);
    assert.equal(finding.kind, 'page-timeout');
    if (finding.kind === 'page-timeout') assert.equal(finding.fields.scope, undefined);
  }
});


test('scoped failures expose the parent control without probe text or raw errors', async () => {
  const message = 'locator.fill: Timeout waiting for [data-role="restock-input"] SECRET_PROBE';
  const run = browserApplicationBoundary(async (_args: { scope: string }) => {
    throw Object.assign(new Error(message), { name: 'TimeoutError' });
  }, args => args.scope);
  await assert.rejects(run({ scope: 'admin-location-row' }), error => {
    assert(error instanceof Error);
    assert.equal(error.message,
      'the restock-input control inside the admin-location-row control did not become available in time');
    assert(!error.message.includes('SECRET_PROBE'));
    return true;
  });
  for (const message of ['locator.selectOption: failed', 'locator.click: element intercepts pointer events']) {
    assert(pageFailure(message, 'admin-location-row').message.includes('inside the admin-location-row control'));
  }
});


test('real browser control failures score, but grader selector and script errors do not', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(100);
    await page.setContent('<button>Button</button>');
    for (const [operation, status] of [
      [() => page.locator('#missing').click(), 'failed'],
      [() => page.locator('button').fill('value'), 'failed'],
      [() => page.locator('[').count(), 'harness_failure'],
      [() => page.evaluate(() => { throw new Error('broken grader observation'); }), 'harness_failure'],
      [() => page.keyboard.press('NotAKey'), 'harness_failure'],
    ] as const) {
      const registry = createActionRegistry([{ id: 'probe', version: '1', category: 'browser-observation',
        capabilities: [], sensitivity: [], timeoutMs: 5000, compile: input => input,
        execute: browserApplicationBoundary(async () => { await operation(); }) }]);
      const result = await executeAction(registry, 'probe', {}, {});
      assert.equal(result.status, status, result.summary ?? undefined);
    }
  } finally { await browser.close(); }
});
