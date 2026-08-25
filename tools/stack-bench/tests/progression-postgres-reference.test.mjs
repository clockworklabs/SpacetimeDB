import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

const root = join(import.meta.dirname, '..', 'reference-apps', 'ecommerce', 'progression', 'postgres');
const read = (...parts) => readFileSync(join(root, ...parts), 'utf8');

test('PostgreSQL progression reference has local production builds', () => {
  const server = JSON.parse(read('server', 'package.json'));
  const client = JSON.parse(read('client', 'package.json'));
  assert.equal(server.scripts.build, 'tsc --noEmit');
  assert.equal(client.scripts.build, 'vite build');
});

test('PostgreSQL progression reference exposes required progression actions', () => {
  const server = `${read('server', 'src', 'index.ts')}\n${read('server', 'src', 'progression.ts')}`;
  for (const route of [
    '/api/profile', '/api/staff/:id/role', '/api/catalog/products', '/api/support/cases',
    '/api/promotions', '/api/cart/promotion', '/api/notifications/preferences',
    '/api/items/:id/stock-alert', '/api/admin/scheduled-restocks', '/api/reorders/:itemId',
    '/api/cart/recover/:id', '/api/recommendations/:itemId/dismiss',
  ]) assert.match(server, new RegExp(route.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  for (const behavior of [
    'cart_reservation_allocation', "interval '90 seconds'", "interval '5 minutes'",
    "interval '60 seconds'", 'payment_status', 'refund_entry', 'recommendation_dismissal',
  ]) assert.ok(server.includes(behavior), `missing ${behavior}`);
});

test('PostgreSQL progression reference exposes stable testing handles', () => {
  const client = `${read('client', 'src', 'App.tsx')}\n${read('client', 'src', 'ProgressionPanel.tsx')}`;
  for (const handle of [
    'profile-save', 'staff-signin-submit', 'staff-role-save', 'catalog-save',
    'support-submit', 'support-update', 'support-reply-submit', 'support-link-order', 'support-refund',
    'promotion-submit', 'apply-promotion', 'notification-save', 'stock-alert',
    'schedule-restock-submit', 'pending-restock-cancel', 'reorder-submit', 'restore-cart',
    'dismiss-recommendation', 'completed-order-status', 'payment-record', 'promotion-report',
    'category-filter', 'search-next-page', 'cart-reservation-timer', 'cart-expired-notice',
  ]) assert.ok(client.includes(`data-testid=\"${handle}\"`), `missing ${handle}`);
  for (const attribute of [
    'data-restock-input', 'data-transfer-input', 'data-price-input', 'data-ship-input',
    'data-cancel-input', 'data-action-input',
  ]) assert.ok(client.includes(attribute), `missing ${attribute}`);
});
