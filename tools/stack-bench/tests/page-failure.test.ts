import assert from 'node:assert/strict';
import test from 'node:test';

import { isFinding } from '../src/actions/action-findings.js';
import { pageFailure } from '../src/actions/browser-action-executors.js';
import type { Finding } from '../src/actions/action-findings.js';

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
  assert.equal(pageFailure(message).message, 'the category-units control did not become available in time');
});

test('a timeout without a named control stays a page timeout', () => {
  const finding = findingOf('page.goto: Timeout 30000ms exceeded.');
  assert.equal(finding.kind, 'page-timeout');
  assert.equal(finding.fields.control, undefined);
  assert.equal(pageFailure('page.goto: Timeout 30000ms exceeded.').message, 'the page did not respond in time');
});
