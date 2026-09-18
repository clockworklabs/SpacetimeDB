import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { setTimeout as delay } from 'node:timers/promises';
import { isDeepStrictEqual } from 'node:util';
import { ConvexClient, ConvexHttpClient } from 'convex/browser';
import { snapshot } from './snapshot.mjs';

const { CONVEX_SELF_HOSTED_URL: url, CONVEX_SELF_HOSTED_ADMIN_KEY: key } = JSON.parse(readFileSync('owned-private.json'));
const phase = process.argv[2] ?? 'initial';
const evidence = { result: 'running', phase, transport: 'native HTTP and WebSocket', observations: [] };
const state = () => snapshot(url, key, ['items', 'orders', 'users', 'markers', 'authAccounts', 'authSessions', 'authRefreshTokens']);
const auth = new ConvexHttpClient(url);
const credentials = { username: 'OwnedLifecycle', password: 'OwnedPassword42' };
const signIn = flow => auth.action('auth:signIn', { provider: 'password', params: { ...credentials, flow } });
const readSaved = () => JSON.parse(readFileSync('lifecycle-private.json'));
const savePrivate = value => writeFileSync('lifecycle-private.json', JSON.stringify(value), { mode: 0o600 });
let live;
async function currentOverWebSocket(token) {
  live = new ConvexClient(url, { unsavedChangesWarning: false });
  live.setAuth(async () => token);
  let current;
  live.onUpdate('accountShop:current', {}, value => { current = value; });
  const deadline = Date.now() + 10000;
  while (!current) { assert(Date.now() < deadline, 'Signed WebSocket query did not complete'); await delay(50); }
  return current;
}
try {
  const admin = new ConvexHttpClient(url); admin.setAdminAuth(key);
  if (phase === 'initial') {
    await admin.mutation('shop:seed', {});
    const { tokens } = await signIn('signUp');
    assert(tokens?.token);
    const actor = new ConvexHttpClient(url); actor.setAuth(tokens.token);
    assert.equal((await actor.query('accountShop:current', {})).name, 'OwnedLifecycle');
    const item = (await state()).tables.items.find(row => row.name === 'Widget');
    const current = await currentOverWebSocket(tokens.token);
    assert.equal(current.name, 'OwnedLifecycle');
    await live.mutation('accountShop:purchase', { itemId: item._id, quantity: 1 });
    await assert.rejects(auth.mutation('accountShop:purchase', { itemId: item._id, quantity: 1 }));
    const after = (await state()).tables;
    assert.equal(after.orders.length, 1, 'Anonymous refusal must not write an order');
    assert.equal(after.orders[0].buyer, current.id);
    assert.equal(after.items.find(row => row._id === item._id).stock, 9);
    evidence.observations = ['real password signup', 'signed HTTP query', 'signed WebSocket query and purchase',
      'anonymous refusal with no effect', 'independent stored order and stock'];
    const scheduled = await admin.mutation('shop:scheduleMarker', { value: 'owned-scheduler-positive', delayMillis: 100 });
    assert.equal(scheduled.state.kind, 'pending');
    const schedulerDeadline = Date.now() + 10000;
    while (!(await state()).tables.markers.some(row => row.value === 'owned-scheduler-positive')) {
      assert(Date.now() < schedulerDeadline, 'Positive scheduler control must execute'); await delay(100);
    }
    savePrivate({ token: tokens.token, adminKey: key, user: current, before: (await state()).tables });
    evidence.observations.push('positive scheduled effect');
  } else if (phase === 'isolation') {
    const saved = readSaved();
    const foreign = JSON.parse(readFileSync('foreign-private.json'));
    const denied = new ConvexHttpClient(url); denied.setAuth(foreign.token);
    const item = saved.before.items.find(row => row.name === 'Widget');
    await assert.rejects(denied.mutation('accountShop:purchase', { itemId: item._id, quantity: 1 }));
    await assert.rejects(snapshot(url, foreign.adminKey));
    assert(isDeepStrictEqual((await state()).tables, saved.before), 'Foreign credentials must have no stored effect');
    assert.deepEqual(await currentOverWebSocket(saved.token), saved.user);
    evidence.observations = ['other deployment JWT/admin key refused', 'exact own data unchanged', 'own original JWT works'];
  } else if (phase === 'warm' || phase === 'retained') {
    const saved = readSaved();
    assert(isDeepStrictEqual((await state()).tables, saved.before), 'Warm restart must retain exact stored rows');
    assert.deepEqual(await currentOverWebSocket(saved.token), saved.user);
    evidence.observations = ['exact data/accounts/sessions retained', 'original JWT still works over WebSocket'];
    if (phase === 'warm') {
      assert((await signIn('signIn')).tokens.token, 'Password account must remain usable');
      evidence.observations.push('password login retained');
    }
  } else if (phase === 'prepare-reset') {
    const saved = readSaved();
    const scheduled = await admin.mutation('shop:scheduleMarker', { value: 'owned-must-not-survive-reset', delayMillis: 30000 });
    assert.equal(scheduled.state.kind, 'pending');
    savePrivate({ ...saved, scheduled: { id: scheduled._id, time: scheduled.scheduledTime } });
    evidence.observations = ['old scheduled operation is pending'];
  } else if (phase === 'reset') {
    const saved = readSaved();
    assert(Object.values((await state()).tables).every(rows => rows.length === 0), 'Reset requires empty stored state');
    const old = new ConvexHttpClient(url); old.setAuth(saved.token);
    await assert.rejects(old.query('accountShop:current', {}));
    await assert.rejects(signIn('signIn'));
    await assert.rejects(snapshot(url, saved.adminKey));
    const remaining = saved.scheduled.time - Date.now() + 500;
    assert(Number.isFinite(remaining) && remaining < 35000, 'Bound scheduler observation');
    if (remaining > 0) await delay(remaining);
    assert(Object.values((await state()).tables).every(rows => rows.length === 0), 'Old scheduled work must remain absent');
    await admin.mutation('shop:seed', {});
    const fresh = new ConvexHttpClient(url); fresh.setAuth((await signIn('signUp')).tokens.token);
    assert.equal((await fresh.query('accountShop:current', {})).name, credentials.username);
    const item = (await state()).tables.items.find(row => row.name === 'Widget');
    await fresh.mutation('accountShop:purchase', { itemId: item._id, quantity: 1 });
    assert.equal((await state()).tables.orders.length, 1);
    evidence.observations = ['old data/accounts/sessions absent', 'old JWT/password/admin key refused',
      'old scheduled work absent past deadline', 'fresh account and purchase usable'];
    evidence.waitedMillis = Math.max(0, remaining);
  } else throw new Error(`Unknown owned lifecycle probe phase ${phase}`);
  evidence.result = 'passed';
} catch (error) { evidence.result = 'failed'; evidence.error = error.stack; process.exitCode = 1; }
finally {
  await live?.close();
  writeFileSync(`owned-probe${phase === 'initial' ? '' : `-${phase}`}.json`, JSON.stringify(evidence, null, 2));
}
console.log(JSON.stringify(evidence));
