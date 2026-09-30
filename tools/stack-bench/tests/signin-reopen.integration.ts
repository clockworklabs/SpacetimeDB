import assert from 'node:assert/strict';
import test from 'node:test';
import { chromium } from 'playwright';

import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';

// A header toggle that only opens and closes the account panel closes it when a
// failed sign-up left it open on sign-up; reopening shows sign-in. A toggle that
// opens slowly from a closed panel must still be clicked only once.
test('sign-in reopens a toggle that closed a panel left on sign-up', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    for (const layout of ['left-on-signup', 'slow-open']) {
      const page = await browser.newPage();
      await page.setContent(`<header><button id="signin-toggle">Sign in</button></header><main></main><script>
        let open = ${layout === 'left-on-signup'}, mode = 'signup', clicks = 0;
        const main = document.querySelector('main');
        const render = () => {
          main.innerHTML = !open ? '' : mode === 'signup'
            ? '<form><input id="signup-username"><button type="button" class="switch">Sign in instead</button></form>'
            : '<form id="signin"><input id="signin-username"><input id="signin-password"><button id="signin-submit">Go</button></form>';
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
      await page.close();
    }
  } finally { await browser.close(); }
});
