import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import {
  hasPendingRestockForRule,
  isTicketCreator,
  planStockAllocation,
} from '../reference-apps/ecommerce/spacetime/backend/spacetimedb/src/progression-policy.js';

const appRoot = join(STACK_BENCH_ROOT, 'reference-apps', 'ecommerce', 'spacetime');
const read = (path: string) => readFileSync(path, 'utf8');

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

test('support ticket visibility follows the submitting SpacetimeDB identity', () => {
  assert.equal(isTicketCreator('visitor-a', 'visitor-a'), true);
  assert.equal(isTicketCreator('visitor-b', 'visitor-a'), false);

  const schema = read(join(appRoot, 'backend', 'spacetimedb', 'src', 'schema.ts'));
  const backend = read(join(appRoot, 'backend', 'spacetimedb', 'src', 'index.ts'));
  const client = read(join(appRoot, 'client', 'src', 'components', 'ProgressionWorkbench.tsx'));
  assert.match(schema, /creatorIdentity:\s*t\.identity\(\)/);
  assert.match(backend, /creatorIdentity:\s*ctx\.sender/);
  assert.match(backend, /isTicketCreator\(sender, row\.creatorIdentity\.toHexString\(\)\)/);
  assert.match(client, /supportTickets[\s\S]+?\.reference \?\? ''/);
  assert.doesNotMatch(client, /Ticket submitted:/);
});

test('automatic restock suppression is scoped to one rule', () => {
  const pending = [{ reorderRuleId: 7n, status: 'pending' }];
  assert.equal(hasPendingRestockForRule(pending, 7n), true);
  assert.equal(hasPendingRestockForRule(pending, 8n), false);
});
