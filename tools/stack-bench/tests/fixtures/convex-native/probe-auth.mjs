/* global document, window -- used inside page.evaluate callbacks */
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { build } from 'esbuild';
import { ConvexHttpClient } from 'convex/browser';
import { snapshot } from './snapshot.mjs';
const evidence = { result: 'running', observations: [], compatibilityFailures: [], note: 'Local account feasibility only, not qualification.' };
const record = (name) => evidence.observations.push(name);
const save = () => writeFileSync('auth-evidence.json', JSON.stringify(evidence, null, 2));
save();
let browser; let server;
try {
  const url = process.env.CONVEX_SELF_HOSTED_URL;
  const adminKey = process.env.CONVEX_SELF_HOSTED_ADMIN_KEY;
  assert(url && adminKey);
  const admin = new ConvexHttpClient(url); admin.setAdminAuth(adminKey);
  await admin.mutation('shop:seed', {});
  const initial = await snapshot(url, adminKey);
  const item = initial.tables.items.find(row => row.name === 'Widget');
  const auth = async (username, password, flow = 'signUp') => new ConvexHttpClient(url).action('auth:signIn', {
    provider: 'password', params: { username, password, flow },
  });
  const caller = (token) => { const client = new ConvexHttpClient(url); if (token) client.setAuth(token); return client; };
  const buy = (token, extra = {}) => caller(token).mutation('accountShop:purchase', { itemId: item._id, quantity: 1, ...extra });
  const noEffects = async () => assert.deepEqual((await snapshot(url, adminKey)).tables, initial.tables);
  const buyer = await auth('Buyer-1', 'a');
  assert(buyer.tokens?.token);
  const identity = await caller(buyer.tokens.token).query('accountShop:current', {});
  assert.equal(identity.name, 'Buyer-1'); record('one-character password and username display preserved');
  await assert.rejects(auth('Buyer-1', 'wrong', 'signIn'));
  try {
    const duplicate = await auth('Buyer-1', 'a');
    assert.equal((await caller(duplicate.tokens.token).query('accountShop:current', {})).id, identity.id);
    evidence.compatibilityFailures.push('Taken username with the correct existing password issues a session during signUp; account check 1c requires refusal.');
  } catch (error) {
    assert.match(error.message, /already exists/, 'Only a measured duplicate refusal satisfies this boundary');
    record('duplicate signup refused');
  }
  await assert.rejects(auth('buyer-1', 'a', 'signIn'));
  const differentCase = await auth('buyer-1', 'b');
  assert.notEqual((await caller(differentCase.tokens.token).query('accountShop:current', {})).id, identity.id);
  record('wrong password refused; case variants are distinct accounts');
  evidence.registrationRaces = [];
  for (const samePassword of [true, false]) {
    for (let round = 0; round < 3; round++) {
      const name = `Race-${samePassword ? 'same' : 'different'}-${round}`;
      const passwords = Array.from({ length: 6 }, (_, i) => `RacePassword-${samePassword ? 0 : i}`);
      const results = await Promise.allSettled(passwords.map(password => auth(name, password)));
      const winners = results.flatMap((result, i) => result.status === 'fulfilled' ? [i] : []);
      assert.equal(winners.length, 1, `Exactly one session for ${name}`);
      for (const result of results) if (result.status === 'rejected') assert.match(result.reason.message, /already exists/);
      const stored = (await snapshot(url, adminKey, ['users', 'authAccounts', 'authSessions'])).tables;
      const accounts = stored.authAccounts.filter(row => row.providerAccountId === name);
      assert.equal(accounts.length, 1);
      assert.equal(stored.users.filter(row => row.name === name).length, 1);
      assert.equal(stored.authSessions.filter(row => row.userId === accounts[0].userId).length, 1);
      assert.equal(typeof accounts[0].secret, 'string');
      assert(!passwords.includes(accounts[0].secret), 'Store a password hash, not the supplied password');
      const winner = winners[0];
      assert.equal((await caller(results[winner].value.tokens.token).query('accountShop:current', {})).id, accounts[0].userId);
      const login = await auth(name, passwords[winner], 'signIn');
      assert(login.tokens.token, 'Winning account must remain usable');
      if (!samePassword) await assert.rejects(auth(name, passwords[(winner + 1) % 6], 'signIn'));
      evidence.registrationRaces.push({ samePassword, requests: 6, accepted: 1, refused: 5, accounts: 1, users: 1, initialSessions: 1 });
    }
  }
  await noEffects();
  record('36 concurrent signups: six same/different-password races each create exactly one account, user and initial session');
  const boundary = await auth('Z'.repeat(48), 'p'.repeat(64));
  assert(boundary.tokens.token);
  const emptyPassword = await auth('EmptyPassword', ''); assert(emptyPassword.tokens.token);
  for (const [name, password] of [['Z'.repeat(49), 'a'], ['Invalid_Name', 'a'], ['TooLongPassword', 'p'.repeat(65)]]) {
    await assert.rejects(auth(name, password));
  }
  record('48-character username and 64-character password accepted; invalid bounds refused; no password minimum added');
  await assert.rejects(buy(undefined));
  const parts = buyer.tokens.token.split('.');
  const payload = JSON.parse(Buffer.from(parts[1], 'base64url'));
  parts[1] = Buffer.from(JSON.stringify({ ...payload, sub: 'forged' })).toString('base64url');
  await assert.rejects(buy(parts.join('.')));
  await assert.rejects(buy(buyer.tokens.token, { buyer: identity.id }));
  await noEffects(); record('anonymous, forged token and extra caller field refused with no order or stock effects');
  await buy(buyer.tokens.token);
  const after = await snapshot(url, adminKey);
  assert.equal(after.tables.orders.length, 1);
  assert.equal(after.tables.orders[0].buyer, identity.id);
  assert.equal(after.tables.items.find(row => row._id === item._id).stock, 9);
  record('real issued session purchased and stored the authenticated user ID');
  await caller(buyer.tokens.token).action('auth:signOut', {});
  await assert.rejects(buy(buyer.tokens.token));
  assert.deepEqual((await snapshot(url, adminKey)).tables, after.tables);
  record('signed-out token replay refused without effects');

  // The deployment key never enters the page; its calls use the token returned by signIn.
  const { outputFiles: [{ contents: script }] } = await build({ entryPoints: ['auth-browser.js'], bundle: true, write: false, platform: 'browser', format: 'esm' });
  server = createServer((request, response) => {
    if (request.url === '/app.js') { response.setHeader('Content-Type', 'text/javascript'); response.end(script); return; }
    response.setHeader('Content-Type', 'text/html');
    response.end(`<form><input id="signup-username" aria-label="Username"><input id="signup-password" type="password" aria-label="Password"><button id="signup-submit" value="signUp">Sign up</button><button id="signin-submit" value="signIn">Sign in</button></form><span id="current-user" hidden></span><button id="purchase">Purchase</button><button id="signout">Sign out</button><p id="status"></p><script>window.DEPLOYMENT_URL=${JSON.stringify(url)};window.ITEM_ID=${JSON.stringify(item._id)};</script><script type="module" src="/app.js"></script>`);
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? '/opt/stack-bench/node_modules/playwright/index.mjs');
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  await page.locator('#signup-username').fill('BrowserUser');
  await page.locator('#signup-password').fill('BrowserSecret42');
  await page.locator('#signup-submit').click();
  await page.waitForFunction(() => document.querySelector('#status').textContent === 'Signed in');
  assert.equal(await page.locator('#current-user').textContent(), 'BrowserUser');
  assert(await page.evaluate(() => typeof window.getSessionToken() === 'string'));
  await page.reload();
  await page.waitForFunction(() => document.querySelector('#current-user').textContent === 'BrowserUser');
  await page.locator('#purchase').click();
  await page.waitForFunction(() => document.querySelector('#status').textContent === 'Purchased');
  const browserToken = await page.evaluate(() => window.getSessionToken());
  const browserUser = await caller(browserToken).query('accountShop:current', {});
  const browserState = await snapshot(url, adminKey);
  assert.equal(browserState.tables.orders.length, 2);
  assert(browserState.tables.orders.some(row => row.buyer === browserUser.id));
  await page.locator('#signout').click();
  await page.waitForFunction(() => document.querySelector('#status').textContent === 'Signed out');
  assert.equal(await page.evaluate(() => window.getSessionToken()), null);
  await assert.rejects(buy(browserToken));
  assert.deepEqual((await snapshot(url, adminKey)).tables, browserState.tables);
  await page.locator('#signup-username').fill('BrowserUser');
  await page.locator('#signup-password').fill('WrongPassword42');
  await page.locator('#signin-submit').click();
  await page.waitForFunction(() => document.querySelector('#status').textContent === 'Sign in rejected');
  assert.equal(await page.evaluate(() => window.getSessionToken()), null);
  await page.locator('#purchase').click();
  await page.waitForFunction(() => document.querySelector('#status').textContent === 'Purchase rejected');
  assert.deepEqual((await snapshot(url, adminKey)).tables, browserState.tables);
  await page.locator('#signup-password').fill('BrowserSecret42');
  await page.locator('#signin-submit').click();
  await page.waitForFunction(() => document.querySelector('#status').textContent === 'Signed in');
  record('Chromium signup, native WebSocket operation, reload, session hook and signout work with actual issued token');
  record('Chromium wrong-password login creates no session and cannot purchase; correct-password login succeeds');
  evidence.result = evidence.compatibilityFailures.length ? 'incompatible' : 'passed'; evidence.final = browserState;
  if (evidence.compatibilityFailures.length) process.exitCode = 1;
} catch (error) {
  evidence.result = 'failed'; evidence.error = error.stack ?? String(error); process.exitCode = 1;
} finally {
  try { if (browser) await browser.close(); if (server) { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); } }
  catch (error) { evidence.result = 'failed'; evidence.cleanupError = String(error); process.exitCode = 1; }
  save();
}
console.log(JSON.stringify({ result: evidence.result, observations: evidence.observations, error: evidence.error }));
