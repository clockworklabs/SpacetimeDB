import assert from 'node:assert/strict';
import test from 'node:test';

import {
  hasPendingRestockForRule,
  isGuestTicketCreator,
  planStockAllocation,
} from '../reference-apps/ecommerce/spacetime/backend/spacetimedb/src/progression-policy.js';

test('cart reservations can span warehouses without over-allocating stock', () => {
  const allocation = planStockAllocation([
    { warehouseId: 1n, quantity: 2 },
    { warehouseId: 2n, quantity: 3 },
  ], 4);

  assert.deepEqual(allocation, [
    { warehouseId: 1n, quantity: 2 },
    { warehouseId: 2n, quantity: 2 },
  ]);
  assert.equal(planStockAllocation([{ warehouseId: 1n, quantity: 2 }], 3), null);
});

test('creator identity grants access only to guest support tickets', () => {
  assert.equal(isGuestTicketCreator('visitor-a', 'visitor-a', undefined), true);
  assert.equal(isGuestTicketCreator('visitor-b', 'visitor-a', undefined), false);
  assert.equal(isGuestTicketCreator('visitor-a', 'visitor-a', 7n), false,
    'the same identity must not bypass account authorization after logout');
});

test('automatic restock suppression is scoped to one rule', () => {
  const pending = [{ reorderRuleId: 7n, status: 'pending' }];
  assert.equal(hasPendingRestockForRule(pending, 7n), true);
  assert.equal(hasPendingRestockForRule(pending, 8n), false);
});
