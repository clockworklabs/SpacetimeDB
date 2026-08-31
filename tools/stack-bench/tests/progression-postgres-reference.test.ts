import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
const root = join(STACK_BENCH_ROOT, 'reference-apps', 'ecommerce', 'postgres');
const read = (...parts: string[]): string => readFileSync(join(root, ...parts), 'utf8');

type PackageMetadata = {
  scripts: Record<string, string>;
  dependencies: Record<string, string>;
  devDependencies: Record<string, string>;
};

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function stringMap(value: unknown, at: string): Record<string, string> {
  if (!object(value)) throw new Error(`${at} must be an object`);
  const entries: Record<string, string> = {};
  for (const [key, entry] of Object.entries(value)) {
    if (typeof entry !== 'string') throw new Error(`${at}.${key} must be a string`);
    entries[key] = entry;
  }
  return entries;
}

function readPackage(...parts: string[]): PackageMetadata {
  const value: unknown = JSON.parse(read(...parts));
  if (!object(value)) throw new Error(`package ${parts.join('/')} must be an object`);
  return { scripts: stringMap(value.scripts, 'package.scripts'),
    dependencies: stringMap(value.dependencies, 'package.dependencies'),
    devDependencies: stringMap(value.devDependencies, 'package.devDependencies') };
}

function required(map: Record<string, string>, key: string): string {
  const value = map[key];
  if (value === undefined) throw new Error(`package field ${key} is missing`);
  return value;
}

test('PostgreSQL progression reference has local production builds', () => {
  const server = readPackage('server', 'package.json');
  const client = readPackage('client', 'package.json');
  assert.equal(server.scripts.build, 'tsc --noEmit');
  assert.equal(client.scripts.build, 'vite build');
  assert.match(required(server.dependencies, 'drizzle-orm'), /0\.45\.[2-9]|0\.[5-9]\d|[1-9]\d*\./);
  assert.equal(required(server.devDependencies, 'drizzle-kit'), '^0.31.10');
  assert.match(read('server', 'drizzle.config.ts'), /schema: '\.\/src\/schema\.ts'/);
  assert.match(required(client.devDependencies, 'vite'), /8\./);
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
  ]) assert.ok(client.includes(`data-testid="${handle}"`), `missing ${handle}`);
  for (const attribute of [
    'data-restock-input', 'data-transfer-input', 'data-price-input', 'data-ship-input',
    'data-cancel-input', 'data-action-input',
  ]) assert.ok(client.includes(attribute), `missing ${attribute}`);
});

test('support order choices expose product names as separate actions', () => {
  const client = read('client', 'src', 'ProgressionPanel.tsx');
  assert.match(client, /orders\.map\(\(order\) => <button data-testid="support-order-option"/);
  assert.match(client, /order\.items\?\.map\(\(item\) => item\.name\)/);
  assert.doesNotMatch(client, /<select data-testid="support-order-option"/);
});

test('scheduled restocks expose the identifier used by access-control replay', () => {
  const client = read('client', 'src', 'ProgressionPanel.tsx');
  assert.match(client,
    /data-testid="pending-restock-item" data-entity-id=\{String\(item\.id\)\}/);
});
