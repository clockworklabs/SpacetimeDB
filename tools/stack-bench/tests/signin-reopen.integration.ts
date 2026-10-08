import assert from 'node:assert/strict';
import test from 'node:test';
import { chromium } from 'playwright';

import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';

// A header toggle that only opens and closes the account panel closes it when a
// failed sign-up left it open on sign-up; reopening shows sign-in. A modal left on
// sign-up covers the toggle until its overlay-close dismisses it, and may drop the
// toggle's ID while it is open. A toggle that opens slowly from a closed panel must
// still be clicked only once.
test('sign-in reopens a toggle that closed a panel left on sign-up', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    for (const layout of ['left-on-signup', 'modal-on-signup', 'modal-drops-toggle', 'slow-open']) {
      const page = await browser.newPage();
      await page.setContent(`<header><button id="signin-toggle">Sign in</button></header><main></main><script>
        let open = ${layout !== 'slow-open'}, mode = 'signup', clicks = 0;
        const main = document.querySelector('main');
        const modal = ${layout.startsWith('modal')};
        const toggle = document.querySelector('#signin-toggle');
        const render = () => {
          if (${layout === 'modal-drops-toggle'}) toggle.id = open ? '' : 'signin-toggle';
          main.innerHTML = !open ? '' : mode === 'signup'
            ? (modal ? '<div style="position:fixed;inset:0;background:#0008"><button id="overlay-close">x</button>' : '')
              + '<form><input id="signup-username"><button type="button" class="switch">Sign in instead</button></form>'
              + (modal ? '</div>' : '')
            : '<form id="signin"><input id="signin-username"><input id="signin-password"><button id="signin-submit">Go</button></form>';
          const close = document.querySelector('#overlay-close');
          if (close) close.onclick = () => { open = false; render(); };
          const signin = document.querySelector('#signin');
          if (signin) signin.onsubmit = event => { event.preventDefault();
            main.innerHTML = '<strong id="current-user">' + document.querySelector('#signin-username').value + '</strong>'; };
        };
        toggle.onclick = () => {
          window.toggleClicks = ++clicks;
          if (open) { open = false; render(); return; }
          mode = 'signin';
          setTimeout(() => { open = true; render(); }, ${layout === 'slow-open' ? 1500 : 0});
        };
        render();
      </script>`);
      const actor = { page, loc: (id: string) => page.locator(`#${id}`) };
      const capabilities = { actors: { get: () => actor }, 'browser-interaction': {
        defaultWithin: 4000, scopedUser: (user: string) => user, testId: (id: string) => `#${id}`,
        sleep: (ms: number) => page.waitForTimeout(ms),
      } };
      const result = await executeAction(ACTION_REGISTRY, 'signIn',
        { do: 'signIn', actor: 'probe', name: 'customer', exact: true, password: 'secret' }, { capabilities });
      assert.equal(result.status, 'passed', `${layout}: ${JSON.stringify(result.summary)}`);
      assert.equal(await page.locator('#current-user').innerText(), 'customer');
      assert.equal(await page.evaluate(() => (window as unknown as { toggleClicks: number }).toggleClicks),
        layout === 'left-on-signup' ? 2 : 1, layout);
      if (layout.startsWith('modal')) assert.equal(await page.locator('#overlay-close').count(), 0);
      await page.close();
    }
  } finally { await browser.close(); }
});

// Sign-in belongs on the page the application's address opens while signed out; a
// signed-out page elsewhere in the app may offer none. Sign-in opens that page.
test('sign-in opens the application address when the current page has no sign-in', async () => {
  const { createServer } = await import('node:http');
  const server = createServer((_request, response) => {
    response.setHeader('content-type', 'text/html');
    response.end(`<main></main><script>
      const render = () => { document.querySelector('main').innerHTML = location.hash === '#/orders'
        ? '<p>Sign in to see your orders.</p>'
        : '<form id="signin"><input id="signin-username"><input id="signin-password"><button id="signin-submit">Go</button></form>';
        const form = document.querySelector('#signin');
        if (form) form.onsubmit = event => { event.preventDefault();
          document.querySelector('main').innerHTML = '<strong id="current-user">' + form.querySelector('#signin-username').value + '</strong>'; };
      };
      addEventListener('hashchange', render); render();
    </script>`);
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const applicationUrl = `http://127.0.0.1:${(server.address() as { port: number }).port}/`;
  const browser = await chromium.launch({ headless: true });
  try {
    for (const withAddress of [true, false]) {
      const page = await browser.newPage();
      await page.goto(`${applicationUrl}#/orders`);
      const actor = { page, loc: (id: string) => page.locator(`#${id}`) };
      const capabilities = { actors: { get: () => actor }, 'browser-interaction': {
        defaultWithin: 2000, scopedUser: (user: string) => user, testId: (id: string) => `#${id}`,
        sleep: (ms: number) => page.waitForTimeout(ms), ...(withAddress ? { applicationUrl } : {}),
      } };
      const result = await executeAction(ACTION_REGISTRY, 'signIn',
        { do: 'signIn', actor: 'probe', name: 'customer', exact: true, password: 'secret' }, { capabilities });
      assert.equal(result.status, withAddress ? 'passed' : 'failed', JSON.stringify(result.summary));
      if (withAddress) assert.equal(await page.locator('#current-user').innerText(), 'customer');
      await page.close();
    }
  } finally {
    await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
});
