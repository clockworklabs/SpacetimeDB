import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
const root = join(STACK_BENCH_ROOT, 'reference-apps', 'ecommerce', 'postgres');
const read = (...parts: string[]): string => readFileSync(join(root, ...parts), 'utf8');

test('PostgreSQL progression reference exposes stable application interfaces', () => {
  const client = `${read('client', 'src', 'App.tsx')}\n${read('client', 'src', 'ProgressionPanel.tsx')}`;
  assert.ok(client.includes('id={`staff-role-account-${encodeURIComponent(role.username)}`}'));
  for (const handle of [
    'profile-save', 'staff-signin-submit', 'staff-role-save', 'catalog-save',
    'support-submit', 'support-update', 'support-reply-submit', 'support-link-order', 'support-refund',
    'promotion-submit', 'apply-promotion', 'notification-save', 'stock-alert',
    'schedule-restock-submit', 'pending-restock-cancel', 'reorder-submit', 'restore-cart',
    'dismiss-recommendation', 'completed-order-status', 'payment-record', 'promotion-report',
    'category-filter', 'search-next-page', 'cart-reservation-timer', 'cart-expired-notice',
  ]) assert.ok(client.includes(`data-role="${handle}"`), `missing ${handle}`);
  for (const attribute of [
    'data-restock-input', 'data-transfer-input', 'data-price-input', 'data-ship-input',
    'data-cancel-input', 'data-action-input',
  ]) assert.ok(client.includes(attribute), `missing ${attribute}`);
});
