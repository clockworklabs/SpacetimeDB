import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
const root = join(STACK_BENCH_ROOT, 'reference-apps', 'ecommerce', 'postgres');
const read = (...parts: string[]): string => readFileSync(join(root, ...parts), 'utf8');

test('PostgreSQL progression reference exposes required routes', () => {
  const server = `${read('server', 'src', 'index.ts')}\n${read('server', 'src', 'progression.ts')}`;
  for (const route of [
    '/api/profile', '/api/staff/:id/role', '/api/catalog/products', '/api/support/cases',
    '/api/promotions', '/api/cart/promotion', '/api/notifications/preferences',
    '/api/items/:id/stock-alert', '/api/admin/scheduled-restocks', '/api/reorders/:itemId',
    '/api/cart/recover/:id', '/api/recommendations/:itemId/dismiss',
  ]) assert.match(server, new RegExp(route.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
});

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

test('support order choices expose product names as separate actions', () => {
  const client = read('client', 'src', 'ProgressionPanel.tsx');
  assert.match(client, /orders\.map\(\(order\) => <button data-role="support-order-option"/);
  assert.match(client, /order\.items\?\.map\(\(item\) => item\.name\)/);
  assert.doesNotMatch(client, /<select data-role="support-order-option"/);
});

test('scheduled restocks expose the identifier used by access-control replay', () => {
  const client = read('client', 'src', 'ProgressionPanel.tsx');
  assert.match(client,
    /data-role="pending-restock-item" data-entity-id=\{String\(item\.id\)\}/);
});

test('HTTP reference actions use the declared promotion, role, and reply interfaces', () => {
  for (const stack of ['postgres', 'mongodb']) {
    const directory = join(STACK_BENCH_ROOT, 'reference-apps', 'ecommerce', stack);
    const server = readFileSync(join(directory, 'server/src/progression.ts'), 'utf8');
    const client = readFileSync(join(directory, 'client/src/ProgressionPanel.tsx'), 'utf8');
    for (const [method, path] of [['post', '/api/promotions'], ['put', '/api/staff/:id/role'],
      ['post', '/api/support/:id/replies']]) {
      assert(server.includes(`app.${method}("${path}"`), `${stack}: ${method} ${path}`);
    }
    for (const field of ['discountPercent', 'startMicros', 'endMicros', 'usageLimit']) {
      assert(server.includes(`req.body?.${field}`), `${stack} reads ${field}`);
      assert(client.includes(`${field}:`), `${stack} sends ${field}`);
    }
    const reply = server.slice(server.indexOf('app.post("/api/support/:id/replies"'));
    assert(reply.split(/\n {2}\}\)\)?;/)[0]!
      .includes('req.body?.body'), `${stack} reply reads body`);
    assert.match(client, /\/api\/support\/\$\{[^}]+\}\/replies/);
    assert.doesNotMatch(client, /\/api\/support\/cases\/\$\{[^}]+\}\/replies|\/api\/progression\/support\/\$\{[^}]+\}\/replies/);
  }
});
