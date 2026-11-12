import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import ts from 'typescript';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

// Install the MongoDB reference client's locked dependencies first (npm ci).
// Then: node --test dist/tests/reference-notification-draft.browser.js
// Exercise both actual components with React 18 and controlled request/subscription delivery.
// No database, model call, or running qualification app is used.
for (const stack of ['mongodb', 'convex']) {
  test(`${stack}: background updates preserve form edits; saved and account state still load`, async t => {
    const browser = await chromium.launch({ headless: true });
    t.after(() => browser.close());
    const page = await browser.newPage();
    await page.clock.install();
    await page.setContent('<div id="root"></div>');
    const dependencies = join(STACK_BENCH_ROOT, 'reference-apps/ecommerce/mongodb/client/node_modules');
    for (const script of ['react/umd/react.development.js', 'react-dom/umd/react-dom.development.js']) {
      await page.addScriptTag({ path: join(dependencies, script) });
    }
    await page.addScriptTag({ content: `
      window.saved = [];
      window.persisted = { order: false, stock: false };
      window.profileData = { name: 'Original', address: 'Original address' };
      window.acceptSave = true;
      window.failRefresh = false;
      window.revision = 0;
      window.snapshot = () => ({ preference: { ...persisted }, profile: profileData && { ...profileData }, notifications:
        Array.from({ length: ++revision }, (_, id) => ({ id, message: 'update' })) });
      window.transport = {
        request: async (path, token, options = {}) => {
          if (path === '/api/progression/state') {
            if (failRefresh) throw Error('Temporary refresh failure');
            return snapshot();
          }
          if (path !== '/api/progression/preferences') throw Error('Unexpected request: ' + path);
          const choice = JSON.parse(options.body);
          saved.push(choice);
          if (acceptSave) persisted = choice;
          return { preference: { ...persisted } };
        },
        subscribeProgression: callback => {
          window.emit = () => callback(snapshot());
          emit();
          return () => { window.emit = null; };
        },
        mutate: async (name, choice) => {
          if (name !== 'progression:savePreferences') throw Error('Unexpected mutation: ' + name);
          saved.push({ ...choice });
          if (acceptSave) persisted = { ...choice };
          emit();
          return null;
        }
      };
    ` });
    const source = readFileSync(join(STACK_BENCH_ROOT,
      `reference-apps/ecommerce/${stack}/client/src/ProgressionPanel.tsx`), 'utf8');
    const code = ts.transpileModule(source, { compilerOptions: {
      target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
      jsx: ts.JsxEmit.React, esModuleInterop: true,
    } }).outputText;
    await page.addScriptTag({ content: `
      (() => {
        const exports = {};
        const require = name => {
          if (name === 'react') return React;
          if (name === './request') return transport;
          throw Error('Unexpected import: ' + name);
        };
        ${code}
        const root = ReactDOM.createRoot(document.getElementById('root'));
        window.renderAccount = token => ReactDOM.flushSync(() => root.render(
          React.createElement(exports.ProgressionPanel, { token,
            user: { username: token, isAdmin: false, isStaff: false }, items: [], orders: [],
            onSignIn: async () => {}, onRefreshItems: async () => {}, onRefreshCart: async () => {} })));
        renderAccount('owner');
      })();
    ` });
    await page.locator('[data-role="notification-settings"]').click();
    await page.locator('[data-role="notifications-panel"][aria-busy="false"]').waitFor();
    await page.locator('[data-role="profile-link"]').click();
    await page.locator('[data-role="profile-name"]').fill('Edited name');
    await page.locator('[data-role="profile-address"]').fill('Edited address');
    for (const kind of ['order', 'stock']) await page.locator(`[data-role="notification-${kind}"]`).click();
    const before = Number(await page.locator('[data-role="notification-unread-count"]').textContent());
    if (stack === 'mongodb') {
      await page.evaluate('failRefresh = true');
      await page.clock.fastForward(1000);
      await page.getByText('Temporary refresh failure', { exact: true }).waitFor();
      assert.equal(await page.locator('[data-role="notification-order"]').getAttribute('data-state'), 'on');
      assert.equal(await page.locator('[data-role="profile-name"]').inputValue(), 'Edited name');
      await page.evaluate('failRefresh = false');
    }
    if (stack === 'mongodb') await page.clock.fastForward(1000);
    else await page.evaluate('emit()');
    await page.waitForFunction(`document.querySelector('[data-role="notification-unread-count"]').textContent === '${before + 1}'`);
    for (const kind of ['order', 'stock']) {
      assert.equal(await page.locator(`[data-role="notification-${kind}"]`).getAttribute('data-state'), 'on',
        'An unrelated server update must not erase an unsaved edit');
    }
    assert.equal(await page.locator('[data-role="profile-name"]').inputValue(), 'Edited name');
    assert.equal(await page.locator('[data-role="profile-address"]').inputValue(), 'Edited address');
    await page.locator('[data-role="notification-save"]').click();
    await page.waitForFunction('saved.length === 1');
    assert.deepEqual(await page.evaluate('saved[0]'), { order: true, stock: true });

    // A changed persisted value still loads. Then change account with an unsaved
    // draft and identical stored values, to test account identity independently.
    await page.evaluate('persisted = { order: false, stock: false }');
    if (stack === 'mongodb') await page.clock.fastForward(1000);
    else await page.evaluate('emit()');
    await page.locator('[data-role="notification-order"][data-state="off"]').waitFor();
    for (const kind of ['order', 'stock']) await page.locator(`[data-role="notification-${kind}"]`).click();
    await page.evaluate("persisted = { order: false, stock: false }; renderAccount('other')");
    await page.locator('[data-role="notification-order"][data-state="off"]').waitFor();
    assert.equal(await page.locator('[data-role="notification-stock"]').getAttribute('data-state'), 'off');
    assert.equal(await page.locator('[data-role="profile-name"]').inputValue(), 'Original');
    assert.equal(await page.locator('[data-role="profile-address"]').inputValue(), 'Original address');

    if (stack === 'mongodb') {
      // A server which did not save must not be hidden by optimistic draft state.
      await page.evaluate('acceptSave = false');
      await page.locator('[data-role="notification-order"]').click();
      await page.locator('[data-role="notification-save"]').click();
      await page.waitForFunction('saved.length === 2');
      assert.deepEqual(await page.evaluate('saved[1]'), { order: true, stock: false });
      await page.locator('[data-role="notification-order"][data-state="off"]').waitFor();
    }
    await page.evaluate("profileData = null; renderAccount('empty-profile')");
    await page.waitForFunction("document.querySelector('[data-role=\"profile-name\"]').value === ''");
    assert.equal(await page.locator('[data-role="profile-address"]').inputValue(), '');
  });
}
