import assert from 'node:assert/strict';
import test from 'node:test';
import { performance } from 'node:perf_hooks';
import { executeAction } from '../src/actions/action-contract.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import type { CheckoutState } from '../src/stacks/checkout-state.js';

test('lost-reply action requires observed commit and a clean gate, and always restores delivery', async t => {
  let now = 0;
  t.mock.method(performance, 'now', () => now);
  const before: CheckoutState = { accountId: 'a', itemId: 'i', priceMinor: 100, cart: [], reservations: [],
    orders: [], payments: [], refunds: [], stock: [{ itemId: 'i', warehouseId: 'w', quantity: 10 }],
    orphanOrderLines: 0, orphanAllocations: 0, orphanRefunds: 0 };
  const prepared = structuredClone(before); prepared.cart = [{ itemId: 'i', quantity: 1 }];
  const after = structuredClone(before); after.stock[0]!.quantity--;
  after.orders.push({ id: 'o', accountId: 'a', totalMinor: 100, refundedMinor: 0, status: 'pending',
    lines: [{ itemId: 'i', quantity: 1, priceMinor: 100, allocations: [{ warehouseId: 'w', quantity: 1 }] }] });
  const snapshot = (state: CheckoutState) => ({ account: 'a', item: 'i', state, scope: 'orders',
    storage: { kind: 'order-data', cart: true, warehouses: true }, schemaSha256: { schema: 'fixed' },
    catalog: [{ itemId: 'i', name: 'i', priceMinor: 100 }] });
  for (const mode of ['valid', 'reader-error', 'gate-error', 'overflow', 'schema-change', 'no-drop', 'no-commit']) {
    let armed = false, finished = false, clicked = false;
    const gate = {
      arm() { armed = true; }, async finish() { finished = true; },
      evidence: () => ({ state: finished ? 'finished' : 'armed', truncated: mode === 'overflow',
        errors: mode === 'gate-error' ? ['interception failed'] : [],
        events: mode === 'no-drop' ? [] : [{ kind: 'http-request' }, { kind: 'http-response' }] }),
    };
    const result = await executeAction(ACTION_REGISTRY, 'loseCheckoutResponse', {
      do: 'loseCheckoutResponse', actor: 'a', before: 'before', prepared: 'prepared', quantity: 1,
    }, { capabilities: {
      actors: { get: () => ({ loc: () => ({ click: async () => { assert(armed); clicked = true; } }) }) },
      'response-loss': { get: () => gate }, clock: { sleep: async () => { now += 10_000; } },
      'database-read': { checkoutSnapshots: new Map([['before', snapshot(before)], ['prepared', snapshot(prepared)]]),
        getCheckoutState: () => {
          if (mode === 'reader-error') throw new Error('reader unavailable');
          const value = snapshot(mode === 'no-commit' ? prepared : after);
          if (mode === 'schema-change') value.schemaSha256.schema = 'changed';
          return value;
        } },
    } });
    assert(clicked && finished, mode);
    assert.equal(result.status, mode === 'valid' ? 'passed'
      : ['no-drop', 'no-commit'].includes(mode) ? 'inconclusive' : 'harness_failure', mode);
    assert.equal((result.observation as { fault: { state: string } }).fault.state, 'finished');
  }
});
