import assert from 'node:assert/strict';
import test from 'node:test';
import { settledLocatorCount } from '../src/evidence/browser-evidence.mjs';

test('an optional locator timeout is counted as a healthy absence', async () => {
  const timeout = Object.assign(new Error('not visible'), { name: 'TimeoutError' });
  const locator = { waitFor: async () => { throw timeout; }, count: async () => 0 };
  assert.equal(await settledLocatorCount(locator, 10), 0);
});

test('a crashed page is never converted into an element count of zero', async () => {
  const crash = new Error('locator.waitFor: Page crashed');
  const locator = { waitFor: async () => { throw crash; }, count: async () => 0 };
  await assert.rejects(() => settledLocatorCount(locator, 10), /Page crashed/);
});

test('count failures propagate after a locator becomes visible', async () => {
  const locator = { waitFor: async () => {}, count: async () => { throw new Error('Target closed'); } };
  await assert.rejects(() => settledLocatorCount(locator, 10), /Target closed/);
});
