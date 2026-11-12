import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { ConvexClient, ConvexHttpClient } from 'convex/browser';
import { snapshot } from './snapshot.mjs';
const evidence = { result: 'running', observations: [], snapshots: [], note: 'Pinned vendor-internal administrative write; not release qualification.' };
writeFileSync('probe-evidence.json', JSON.stringify(evidence, null, 2));
async function probe() {
const { convexFunctionRequest, classifyConvexFunctionResponse } = await import(
  process.env.STACK_BENCH_CONVEX_PROTOCOL ?? new URL('../../../dist/src/stacks/backends/convex-protocol.js', import.meta.url));
const url = process.env.CONVEX_SELF_HOSTED_URL;
const adminKey = process.env.CONVEX_SELF_HOSTED_ADMIN_KEY;
assert(url && adminKey, 'Use this fixture deployment URL and admin key');
const tokens = JSON.parse(readFileSync('identities.json', 'utf8'));
const admin = new ConvexHttpClient(url);
admin.setAdminAuth(adminKey);
const state = async (table) => {
  const result = await snapshot(url, adminKey);
  evidence.snapshots.push(result);
  return result.tables[table];
};
const call = async (path, args, token) => {
  const request = convexFunctionRequest({ deploymentUrl: url, kind: 'mutation', path, args, token });
  const response = await fetch(request.url, { ...request, signal: AbortSignal.timeout(10000) });
  const raw = await response.text();
  const result = classifyConvexFunctionResponse(response.status, raw);
  evidence.observations.push({ path, result, raw });
  return result;
};
await admin.mutation('shop:seed', {});
const initial = await state('items');
const item = initial.find((x) => x.name === 'Widget');
const other = initial.find((x) => x.name === 'Other');
const live = new ConvexClient(url, { unsavedChangesWarning: false });
live.setAuth(async () => tokens.observer);
let latest; const stop = live.onUpdate('shop:list', {}, (rows) => { latest = rows; });
const waitStock = async (stock) => {
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) {
    if (latest?.find((x) => x._id === item._id)?.stock === stock) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`Subscription did not observe stock ${stock}`);
};
try {
  await waitStock(10);
  assert.equal((await call('shop:purchase', { itemId: item._id, quantity: 2 }, tokens.buyer)).kind, 'accepted');
  await waitStock(8);
  for (const token of [tokens.observer, undefined]) assert.equal((await call('shop:purchase', { itemId: item._id, quantity: 1 }, token)).kind, 'application-error');
  assert.equal((await call('shop:deliberateError', {}, tokens.buyer)).kind, 'application-error');
  assert.equal((await call('shop:unhandledError', {}, tokens.buyer)).kind, 'function-error');
  assert(['function-error', 'http-error'].includes((await call('shop:missing', {}, tokens.buyer)).kind));
  assert.equal((await state('orders')).length, 1);
  assert.equal((await state('items')).find(x => x._id === item._id).stock, 8);
  // Vendor dashboard mutation, not a fixture business handler or table replacement.
  assert.deepEqual(await admin.mutation('_system/frontend/patchDocumentsFields', { table: 'items', ids: [item._id], fields: { stock: 19 }, componentId: null }), { success: true });
  await waitStock(19);
  const final = await state('items');
  assert.deepEqual(final.find((x) => x._id === other._id), other);
  assert.deepEqual(final.find((x) => x._id === item._id), { ...item, stock: 19 });
  assert.equal((await state('orders')).length, 1);

  // Drop a real native HTTP reply after the backend has completed the write.
  // The caller sees an unknown outcome, even though independent state proves a commit.
  let upstream; let proxyError;
  const proxy = createServer(async (request, response) => {
    try {
      const chunks = []; for await (const chunk of request) chunks.push(chunk);
      const reply = await fetch(`${url}/api/mutation`, { method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${tokens.buyer}` },
        body: Buffer.concat(chunks), signal: AbortSignal.timeout(10000) });
      upstream = classifyConvexFunctionResponse(reply.status, await reply.text());
    } catch (error) { proxyError = error; }
    finally { response.destroy(); }
  });
  proxy.listen(0, '127.0.0.1'); await once(proxy, 'listening');
  try {
    const request = convexFunctionRequest({ deploymentUrl: `http://127.0.0.1:${proxy.address().port}`,
      kind: 'mutation', path: 'shop:purchase', args: { itemId: item._id, quantity: 1 } });
    await assert.rejects(fetch(request.url, { ...request, signal: AbortSignal.timeout(10000) }));
    assert.equal(proxyError, undefined);
    assert.equal(upstream?.kind, 'accepted');
    assert.equal((await state('orders')).length, 2);
    assert.equal((await state('items')).find(x => x._id === item._id).stock, 18);
    await waitStock(18);
    evidence.observations.push({ path: 'shop:purchase', kind: 'transport-unknown', committed: true });
  } finally { proxy.closeAllConnections(); await new Promise(resolve => proxy.close(resolve)); }
  const end = await snapshot(url, adminKey); evidence.snapshots.push(end);
  evidence.initial = initial; evidence.final = end.tables.items; evidence.orders = end.tables.orders;
} finally {
  try { stop(); } finally { await live.close(); }
}
}
try { await probe(); evidence.result = 'passed'; }
catch (error) { evidence.result = 'failed'; evidence.error = String(error); throw error; }
finally { writeFileSync('probe-evidence.json', JSON.stringify(evidence, null, 2)); }
console.log('Convex native calls, refusal, subscriptions, targeted patch and lost-response slice passed');
