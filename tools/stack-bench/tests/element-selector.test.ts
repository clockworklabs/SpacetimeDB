import assert from 'node:assert/strict';
import test from 'node:test';

import { stableElementSelector } from '../src/actions/element-selector.js';

test('stable selectors support application interface names', () => {
  assert.equal(stableElementSelector('account-name'),
    '[data-role="account-name"],#account-name');
  assert.throws(() => stableElementSelector('account name'), /invalid application interface name/);
});
