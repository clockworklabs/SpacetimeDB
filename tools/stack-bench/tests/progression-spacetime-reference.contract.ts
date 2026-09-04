import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { compileProgressionDefinitionFile }
  from '../src/progression/progression-definition.js';

const root = STACK_BENCH_ROOT;
const appRoot = join(root, 'reference-apps', 'ecommerce', 'spacetime');
const trackRoot = join(root, 'tracks', 'ecommerce');
const read = (path: string): string => readFileSync(path, 'utf8');
const readJson = (path: string): unknown => JSON.parse(read(path));

function filesBelow(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? filesBelow(path) : [path];
  });
}

interface ScenarioInterface {
  roles: Set<string>;
  attributes: Set<string>;
}

function collectScenarioInterface(value: unknown, interfaceNames: ScenarioInterface): void {
  if (Array.isArray(value)) {
    for (const entry of value) collectScenarioInterface(entry, interfaceNames);
    return;
  }
  if (!isRecord(value)) return;
  if (typeof value.testid === 'string') interfaceNames.roles.add(value.testid);
  if (typeof value.attribute === 'string' && value.attribute.startsWith('data-')) {
    interfaceNames.attributes.add(value.attribute);
  }
  for (const entry of Object.values(value)) collectScenarioInterface(entry, interfaceNames);
}

const backendMarkers: Record<string, string> = {
  accounts: 'export const signUp',
  catalog: 'export const item = table',
  'catalog-discovery': 'export const catalogDetails',
  'customer-profile': 'export const saveProfile',
  purchasing: 'export const buyNow',
  cart: 'export const addToCart',
  checkout: 'export const checkout',
  'warehouse-admin': 'export const adminRestock',
  reviews: 'export const submitReview',
  'fulfilment-queue': 'export const shipOrder',
  'stock-transfers': 'export const adminTransferStock',
  'order-cancellation': 'export const cancelOrder',
  'price-history': 'export const adminChangePrice',
  'inventory-dashboard': 'export const itemStats = table',
  'sales-dashboard': 'export const categoryTotals',
  recommendations: 'export const recommended',
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
  'support-refunds': 'export const supportRefund',
};

const namedReducerCalls = [
  ['progression-stock-limit.json', 'buy_now', 'buyNow'],
  ['progression-staff-roles.json', 'assign_staff_role', 'assignStaffRole'],
  ['progression-managed-support-privacy.json', 'reply_support', 'replySupport'],
  ['progression-review-access.json', 'submit_review', 'submitReview'],
] as const;

test('the SpacetimeDB progression backend covers every graph feature', () => {
  const graph = progressionGraph();
  assert.deepEqual(Object.keys(backendMarkers).sort(), graph.nodes.map(node => node.id).sort());
  const source = `${read(join(appRoot, 'backend', 'spacetimedb', 'src', 'schema.ts'))}\n`
    + read(join(appRoot, 'backend', 'spacetimedb', 'src', 'index.ts'));
  for (const node of graph.nodes) {
    const marker = backendMarkers[node.id];
    assert(marker, `${node.id} must have an implementation marker`);
    assert(source.includes(marker),
      `${node.id} must have a SpacetimeDB implementation marker`);
  }
});

test('named reducer scenarios match the SpacetimeDB reference handlers', () => {
  const backend = read(join(appRoot, 'backend', 'spacetimedb', 'src', 'index.ts'));
  const client = filesBelow(join(appRoot, 'client', 'src'))
    .filter(path => /\.(tsx|ts)$/.test(path) && !path.includes('module_bindings'))
    .map(read)
    .join('\n');

  for (const [scenario, reducer, handler] of namedReducerCalls) {
    assert(read(join(trackRoot, 'scenarios', scenario)).includes(`"reducer": "${reducer}"`));
    assert(backend.includes(`export const ${handler} = spacetimedb.reducer`));
    assert.match(client, new RegExp(`\\breducers\\??\\.${handler}\\(`));
  }
});

test('the progression candidate retains the maintained SpacetimeDB build layout', () => {
  const reference = referenceLayout(readJson(join(appRoot, 'reference.json')));
  assert.equal(reference.kind, 'spacetime');
  assert.equal(reference.moduleDirectory, 'backend/spacetimedb');
  assert.equal(reference.bindingsDirectory, 'client/src/module_bindings');
  assert.deepEqual(reference.installDirectories, ['backend/spacetimedb', 'client']);
});

test('the client implements every graph feature interface', () => {
  const graph = progressionGraph();
  const catalogPath = join(trackRoot, 'composition', 'recipes', 'progression-catalog.json');
  const packs = recipePackPaths(readJson(catalogPath), catalogPath);
  const interfaceNames: ScenarioInterface = { roles: new Set<string>(), attributes: new Set<string>() };

  for (const node of graph.nodes) {
    for (const featureRef of node.featureRefs) {
      const packPath = packs.get(featureRef);
      assert(packPath, `catalog must resolve ${featureRef}`);
      const pack = compilePackDefinition(readJson(packPath), { source: packPath });
      for (const check of pack.checks) {
        const scenarioPath = resolve(dirname(packPath), '..', '..', check.source);
        collectScenarioInterface(readJson(scenarioPath), interfaceNames);
      }
    }
  }

  const clientSource = filesBelow(join(appRoot, 'client', 'src'))
    .filter(path => /\.(tsx|ts)$/.test(path) && !path.includes('module_bindings'))
    .map(read)
    .join('\n');
  for (const role of interfaceNames.roles) {
    assert(clientSource.includes(`data-role="${role}"`), `client must expose ${role}`);
  }
  for (const attribute of interfaceNames.attributes) {
    assert(clientSource.includes(`${attribute}=`), `client must expose ${attribute}`);
  }
});

function progressionGraph() {
  return compileProgressionDefinitionFile(join(trackRoot, 'progression', 'ecommerce.json'), {
    trackRoot,
  });
}

interface ReferenceLayout {
  kind: string;
  moduleDirectory: string;
  bindingsDirectory: string;
  installDirectories: string[];
}

function referenceLayout(value: unknown): ReferenceLayout {
  if (!isRecord(value) || typeof value.kind !== 'string'
    || typeof value.moduleDirectory !== 'string' || typeof value.bindingsDirectory !== 'string'
    || !stringArray(value.installDirectories)) {
    throw new Error('reference layout is not valid');
  }
  return {
    kind: value.kind,
    moduleDirectory: value.moduleDirectory,
    bindingsDirectory: value.bindingsDirectory,
    installDirectories: value.installDirectories,
  };
}

function recipePackPaths(value: unknown, catalogPath: string): Map<string, string> {
  if (!isRecord(value) || !Array.isArray(value.packs)) {
    throw new Error('progression catalog must have packs');
  }
  const packs = new Map<string, string>();
  for (const entry of value.packs) {
    if (!isRecord(entry) || typeof entry.id !== 'string' || typeof entry.path !== 'string') {
      throw new Error('progression catalog pack is not valid');
    }
    packs.set(entry.id, resolve(dirname(catalogPath), entry.path));
  }
  return packs;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function stringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every(item => typeof item === 'string');
}

test('direct action handles carry scenario-owned values before form interaction', () => {
  const source = read(join(appRoot, 'client', 'src', 'components', 'AdminPanel.tsx'));
  assert.match(source, /item\.name === 'Gaming Mouse'[\s\S]*price: 1/);
  assert.match(source, /warehouse\.name === 'East'/);
  assert.match(source, /warehouse\.name === 'West'/);
  assert.match(source, /item\.name === 'Headphones'[\s\S]*quantity: 25/);
  assert.doesNotMatch(source, /fromWarehouseId: Number\(transfer\.from \|\| 0\)/);
});
