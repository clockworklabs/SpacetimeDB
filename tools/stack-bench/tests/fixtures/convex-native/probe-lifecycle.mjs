import assert from 'node:assert/strict';
import { writeFileSync, readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { setTimeout as delay } from 'node:timers/promises';
import { build } from 'esbuild';
import { ConvexHttpClient } from 'convex/browser';
import { snapshot } from './snapshot.mjs';
const phase = process.argv[2];
const url = process.env.CONVEX_SELF_HOSTED_URL;
const key = process.env.CONVEX_SELF_HOSTED_ADMIN_KEY;
assert(url && key);
const admin = new ConvexHttpClient(url); admin.setAdminAuth(key);
const state = () => snapshot(url, key, ['items', 'orders', 'users', 'markers']);
const auth = (flow) => new ConvexHttpClient(url).action('auth:signIn', { provider: 'password', params: {
  username: 'LifecycleUser', password: 'LifecycleSecret42', flow,
}});
const path = `lifecycle-${phase}.json`;
const evidence = { result: 'running', phase };
const save = () => writeFileSync(path, JSON.stringify(evidence, null, 2));
save();
try {
  if (phase === 'prepare') {
    await admin.mutation('shop:seed', {});
    const tokens = (await auth('signUp')).tokens;
    const actor = new ConvexHttpClient(url); actor.setAuth(tokens.token);
    const user = await actor.query('accountShop:current', {});
    const item = (await state()).tables.items.find(row => row.name === 'Widget');
    await actor.mutation('accountShop:purchase', { itemId: item._id, quantity: 1 });
    const scheduled = await admin.mutation('shop:scheduleMarker', { value: 'scheduler-positive', delayMillis: 100 });
    assert.equal(scheduled.state.kind, 'pending');
    const deadline = Date.now() + 10000;
    while (!(await state()).tables.markers.some(row => row.value === 'scheduler-positive')) {
      assert(Date.now() < deadline, 'Scheduler positive control must execute'); await delay(100);
    }
    writeFileSync('lifecycle-private.json', JSON.stringify({ token: tokens.token, user, itemId: item._id }), { mode: 0o600 });
    evidence.schedulerPositive = true;
  } else if (phase === 'serve') {
    const saved = JSON.parse(readFileSync('lifecycle-private.json'));
    const { outputFiles: [{ contents: script }] } = await build({ entryPoints: ['auth-browser.js'], bundle: true, write: false, platform: 'browser', format: 'esm' });
    createServer((request, response) => {
      if (request.url === '/app.js') { response.setHeader('Content-Type', 'text/javascript'); response.end(script); return; }
      const api = new URL(request.url, 'http://localhost').searchParams.get('api') ?? url;
      response.setHeader('Content-Type', 'text/html');
      response.end(`<form><input id="signup-username" aria-label="Username"><input id="signup-password" type="password" aria-label="Password"><button id="signup-submit" value="signUp">Sign up</button><button id="signin-submit" value="signIn">Sign in</button></form><span id="current-user" hidden></span><button id="purchase">Purchase</button><button id="signout">Sign out</button><p id="status"></p><script>window.DEPLOYMENT_URL=${JSON.stringify(api)};window.ITEM_ID=${JSON.stringify(saved.itemId)};</script><script type="module" src="/app.js"></script>`);
    }).listen(3100, '0.0.0.0');
    evidence.result = 'serving'; save();
  } else if (phase === 'capture') {
    const before = await state();
    assert.equal(before.tables.orders.length, 3, 'Host, container and initial account each purchased');
    assert.equal(before.tables.items.find(row => row.name === 'Widget').stock, 7);
    assert.deepEqual(before.tables.users.map(row => row.name).sort(), ['ContainerUser', 'HostUser', 'LifecycleUser']);
    evidence.tables = before.tables;
  } else if (phase === 'warm') {
    const before = JSON.parse(readFileSync('lifecycle-capture.json'));
    assert.equal(before.result, 'passed');
    assert.deepEqual((await state()).tables, before.tables);
    const saved = JSON.parse(readFileSync('lifecycle-private.json'));
    const actor = new ConvexHttpClient(url); actor.setAuth(saved.token);
    assert.deepEqual(await actor.query('accountShop:current', {}), saved.user);
    assert((await auth('signIn')).tokens.token, 'Stored password login survives restart');
    evidence.dataAndSavedSessionPreserved = true;
  } else if (phase === 'schedule-reset') {
    const scheduled = await admin.mutation('shop:scheduleMarker', { value: 'must-not-survive-reset', delayMillis: 20000 });
    assert.equal(scheduled.state.kind, 'pending');
    evidence.scheduled = { id: scheduled._id, time: scheduled.scheduledTime, state: scheduled.state.kind };
  } else if (phase === 'reset') {
    const scheduled = JSON.parse(readFileSync('lifecycle-schedule-reset.json'));
    assert.equal(scheduled.result, 'passed');
    const empty = (await state()).tables;
    assert(Object.values(empty).every(rows => rows.length === 0), 'Fresh deployment must have no app data or accounts');
    const saved = JSON.parse(readFileSync('lifecycle-private.json'));
    const actor = new ConvexHttpClient(url); actor.setAuth(saved.token);
    await assert.rejects(actor.query('accountShop:current', {}));
    await assert.rejects(auth('signIn'));
    const remaining = scheduled.scheduled.time - Date.now() + 500;
    assert(Number.isFinite(remaining) && remaining < 30000, 'Bound reset observation wait');
    if (remaining > 0) await delay(remaining);
    assert(Object.values((await state()).tables).every(rows => rows.length === 0), 'Old scheduled work must not run after its due time');
    await admin.mutation('shop:seed', {});
    const fresh = await auth('signUp');
    assert(fresh.tokens.token, 'Same username can register on the fresh deployment');
    evidence.oldDataAccountsAndScheduledWorkAbsent = true;
    evidence.waitedMillis = Math.max(0, remaining);
  } else throw new Error(`Unknown phase: ${phase}`);
  if (phase !== 'serve') evidence.result = 'passed';
} catch (error) { evidence.result = 'failed'; evidence.error = error.stack; process.exitCode = 1; }
finally { save(); }
console.log(JSON.stringify(evidence));
