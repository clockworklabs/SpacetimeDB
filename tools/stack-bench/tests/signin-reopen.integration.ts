import assert from 'node:assert/strict';
import test from 'node:test';
import { chromium } from 'playwright';

import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';

// A header toggle that only opens and closes the account panel closes it when a
// failed sign-up left it open on sign-up; reopening shows sign-in. A modal left on
// sign-up covers the toggle until its overlay-close dismisses it. A toggle that
// opens slowly from a closed panel must still be clicked only once.
test('sign-in reopens a toggle that closed a panel left on sign-up', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    for (const layout of ['left-on-signup', 'modal-on-signup', 'slow-open']) {
      const page = await browser.newPage();
      await page.setContent(`<header><button id="signin-toggle">Sign in</button></header><main></main><script>
        let open = ${layout !== 'slow-open'}, mode = 'signup', clicks = 0;
        const main = document.querySelector('main');
        const modal = ${layout === 'modal-on-signup'};
        const render = () => {
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
        document.querySelector('#signin-toggle').onclick = () => {
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
      if (layout === 'modal-on-signup') assert.equal(await page.locator('#overlay-close').count(), 0);
      await page.close();
    }
  } finally { await browser.close(); }
});
