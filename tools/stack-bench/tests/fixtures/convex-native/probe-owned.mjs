import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { setTimeout as delay } from 'node:timers/promises';
import { ConvexClient, ConvexHttpClient } from 'convex/browser';
import { snapshot } from './snapshot.mjs';

const { CONVEX_SELF_HOSTED_URL: url, CONVEX_SELF_HOSTED_ADMIN_KEY: key } = JSON.parse(readFileSync('owned-private.json'));
const evidence = { result: 'running', transport: 'native HTTP and WebSocket', observations: [] };
let live;
try {
  const admin = new ConvexHttpClient(url); admin.setAdminAuth(key);
  await admin.mutation('shop:seed', {});
  const auth = new ConvexHttpClient(url);
  const { tokens } = await auth.action('auth:signIn', { provider: 'password', params: {
    flow: 'signUp', username: 'OwnedLifecycle', password: 'OwnedPassword42',
  } });
  assert(tokens?.token);
  const actor = new ConvexHttpClient(url); actor.setAuth(tokens.token);
  assert.equal((await actor.query('accountShop:current', {})).name, 'OwnedLifecycle');
  const state = () => snapshot(url, key, ['items', 'orders', 'users']);
  const item = (await state()).tables.items.find(row => row.name === 'Widget');
  live = new ConvexClient(url, { unsavedChangesWarning: false });
  live.setAuth(async () => tokens.token);
  let current;
  live.onUpdate('accountShop:current', {}, value => { current = value; });
  const deadline = Date.now() + 10000;
  while (!current) { assert(Date.now() < deadline, 'Signed WebSocket query did not complete'); await delay(50); }
  assert.equal(current.name, 'OwnedLifecycle');
  await live.mutation('accountShop:purchase', { itemId: item._id, quantity: 1 });
  await assert.rejects(auth.mutation('accountShop:purchase', { itemId: item._id, quantity: 1 }));
  const after = (await state()).tables;
  assert.equal(after.orders.length, 1, 'Anonymous refusal must not write an order');
  assert.equal(after.orders[0].buyer, current.id);
  assert.equal(after.items.find(row => row._id === item._id).stock, 9);
  evidence.observations = ['real password signup', 'signed HTTP query', 'signed WebSocket query and purchase',
    'anonymous refusal with no effect', 'independent stored order and stock'];
  evidence.result = 'passed';
} catch (error) { evidence.result = 'failed'; evidence.error = error.stack; process.exitCode = 1; }
finally {
  await live?.close();
  writeFileSync('owned-probe.json', JSON.stringify(evidence, null, 2));
}
console.log(JSON.stringify(evidence));
