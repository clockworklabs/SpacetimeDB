import assert from 'node:assert/strict';
import test from 'node:test';
import { settledLocatorCount } from '../src/evidence/browser-evidence.js';

test('a crashed page is never converted into an element count of zero', async () => {
  // An optional locator timeout is a healthy absence.
  const timeout = Object.assign(new Error('not visible'), { name: 'TimeoutError' });
  assert.equal(await settledLocatorCount({ waitFor: async () => { throw timeout; }, count: async () => 0 }, 10), 0);
  const crash = new Error('locator.waitFor: Page crashed');
  const locator = { waitFor: async () => { throw crash; }, count: async () => 0 };
  await assert.rejects(() => settledLocatorCount(locator, 10), /Page crashed/);
  // Count failures propagate after a locator becomes visible.
  const closed = { waitFor: async () => {}, count: async () => { throw new Error('Target closed'); } };
  await assert.rejects(() => settledLocatorCount(closed, 10), /Target closed/);
});
