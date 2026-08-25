import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

const root = join(import.meta.dirname, '..');
const appRoot = join(root, 'reference-apps', 'ecommerce', 'progression', 'spacetime');
const trackRoot = join(root, 'tracks', 'ecommerce');
const read = path => readFileSync(path, 'utf8');
const readJson = path => JSON.parse(read(path));

const backendMarkers = {
  accounts: 'export const signUp',
  catalog: 'export const item = table',
  'customer-profile': 'export const saveProfile',
  purchasing: 'export const buyNow',
  'cart-checkout': 'export const checkout',
  'warehouse-admin': 'export const adminRestock',
  reviews: 'export const writeReview',
  'fulfilment-queue': 'export const shipOrder',
  'stock-transfers': 'export const adminTransferStock',
  'order-cancellation': 'export const cancelOrder',
  'price-history': 'export const adminChangePrice',
  'operational-views': 'export const categoryTotals',
  reservations: 'export const reservation = table',
  'order-delivery': 'export const deliverySchedule = table',
  'order-returns': 'export const returnOrderItem',
  'scheduled-restocks': 'export const scheduleRestock',
  'cart-expiration': 'export const cartExpiry = table',
  'staff-access': 'export const signIn',
  'staff-roles': 'export const assignStaffRole',
  'catalog-management': 'export const addCatalogProduct',
  'support-intake': 'export const createSupportTicket',
  'support-triage': 'export const triageSupport',
  'support-history': 'export const visibleSupportTickets',
  'promotion-rules': 'export const createPromotion',
  'notification-preferences': 'export const saveNotificationPreferences',
  'faceted-search': 'export const itemCategory = table',
  'payment-records': 'export const paymentRecord = table',
  'managed-support': 'export const replySupport',
  'promotion-checkout': 'export const applyPromotion',
  'stock-alerts': 'export const requestStockAlert',
  'personalized-recommendations': 'export const recommended',
  'staff-activity': 'export const activityHistory',
  'order-support': 'export const linkSupportOrder',
  'promotion-reporting': 'export const promotionReports',
  'automatic-reorder': 'export const saveReorderRule',
  'cart-recovery': 'export const restoreExpiredCart',
  'delivery-notifications': 'export const myNotifications',
  'recommendation-feedback': 'export const dismissRecommendation',
  'support-refunds': 'export const refundSupportOrder',
};

test('the SpacetimeDB progression backend covers every graph feature', () => {
  const graph = readJson(join(trackRoot, 'progression', 'ecommerce-1.0.0.json'));
  assert.deepEqual(Object.keys(backendMarkers).sort(), graph.nodes.map(node => node.id).sort());
  const source = `${read(join(appRoot, 'backend', 'spacetimedb', 'src', 'schema.ts'))}\n`
    + read(join(appRoot, 'backend', 'spacetimedb', 'src', 'index.ts'));
  for (const node of graph.nodes) {
    assert(source.includes(backendMarkers[node.id]),
      `${node.id} must have a SpacetimeDB implementation marker`);
  }
});

test('the progression candidate retains the maintained SpacetimeDB build layout', () => {
  const reference = readJson(join(appRoot, 'reference.json'));
  assert.equal(reference.kind, 'spacetime');
  assert.equal(reference.moduleDirectory, 'backend/spacetimedb');
  assert.equal(reference.bindingsDirectory, 'client/src/module_bindings');
  assert.deepEqual(reference.installDirectories, ['backend/spacetimedb', 'client']);
});
