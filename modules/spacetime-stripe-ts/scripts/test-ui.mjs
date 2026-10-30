/* global window, document, localStorage */
import { chromium } from 'playwright-core';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const root = new URL('../example/public/', import.meta.url);
const browser = await chromium.launch({
  channel: 'chrome',
  headless: process.env.HEADED !== '1',
});
try {
  const page = await browser.newPage();
  await page.route('**/*', route => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/api/config') return route.fulfill({ json: {} });
    if (path === '/app.js')
      return route.fulfill({ contentType: 'text/javascript', body: '' });
    if (['/', '/ui.js', '/styles.css'].includes(path))
      return route.fulfill({
        contentType:
          path === '/'
            ? 'text/html'
            : path.endsWith('.js')
              ? 'text/javascript'
              : 'text/css',
        body: readFileSync(
          new URL(path === '/' ? 'index.html' : path.slice(1), root)
        ),
      });
    return route.fulfill({ status: 404, body: '' });
  });
  await page.addInitScript(() => {
    window.checkoutCalls = [];
    window.testBilling = { purchases: 0, subscriptions: [], checkouts: [] };
    window.cancelError =
      'This checkout is complete. Check Billing for the payment result.';
    window.cancelCalls = [];
    window.stdb = {
      identity: 'buyer_one',
      ownsCheckoutSession: async () => false,
      cancelCheckoutAttempt: async args => {
        window.cancelCalls.push(args);
        if (window.cancelError) throw new Error(window.cancelError);
      },
      getBilling: async () => window.testBilling,
      validatePrice: async () => ({ valid: true }),
      createCheckoutSession: async args => {
        window.checkoutCalls.push(args);
        throw new Error('Test lost response');
      },
    };
  });
  await page.goto('http://localhost/');
  await page.waitForFunction(() =>
    document.getElementById('billingStatus').textContent.includes('0 confirmed')
  );
  async function catalog(priceId, mode = 'payment') {
    await page.evaluate(
      ({ priceId, mode }) =>
        window.dispatchEvent(
          new CustomEvent('stdb:catalog', {
            detail: {
              products: [
                {
                  id: 'product',
                  name: 'Test product',
                  description: '',
                  priceId,
                  mode,
                  priceLabel: '$1.00',
                  perks: [],
                },
              ],
            },
          })
        ),
      { priceId, mode }
    );
  }
  async function checkout() {
    await page.locator('#btnCheckoutCart').click();
    await page.getByText('Test lost response', { exact: true }).waitFor();
    return page.evaluate(() => window.checkoutCalls.at(-1).attemptId);
  }
  await catalog('price_old');
  await page.getByRole('button', { name: 'Add to cart', exact: true }).click();
  await page.locator('#btnCart').click();
  const first = await checkout();
  assert.equal(
    await checkout(),
    first,
    'unchanged cart must reuse its request'
  );
  await page.locator('#btnCartClose').click();
  await catalog('price_new');
  await page.getByRole('button', { name: 'Add to cart', exact: true }).click();
  await page.locator('#btnCart').click();
  const changed = await checkout();
  assert.notEqual(
    changed,
    first,
    'changed Stripe price must start a new request'
  );
  assert.equal(
    await checkout(),
    changed,
    'new price retry must keep its request'
  );
  await catalog('price_new', 'subscription');
  let subscription = await checkout();
  assert.notEqual(
    subscription,
    changed,
    'changed checkout mode must start a new request'
  );
  await page.evaluate(() => {
    window.stdb.identity = 'buyer_two';
  });
  const newBuyerAttempt = await checkout();
  assert.notEqual(
    newBuyerAttempt,
    subscription,
    'a new buyer must not inherit the previous checkout'
  );
  await page.evaluate(() => {
    window.stdb.identity = 'buyer_one';
  });
  assert.equal(
    await checkout(),
    subscription,
    'returning buyer must retain their pending attempt'
  );
  await page.evaluate(() => {
    window.stdb.identity = 'buyer_two';
  });
  assert.equal(await checkout(), newBuyerAttempt);
  await page.evaluate(() => {
    const base = 'stdb.premiumStore.checkoutAttempt';
    const scoped = base + '.' + window.stdb.identity;
    const attempt = JSON.parse(localStorage.getItem(scoped));
    delete attempt.owner;
    attempt.sessionId = 'cs_buyer_one';
    localStorage.setItem(base, JSON.stringify(attempt));
    localStorage.removeItem(scoped);
    window.cancelError = 'store.checkout_owner_mismatch';
  });
  subscription = await checkout();
  assert.notEqual(
    subscription,
    newBuyerAttempt,
    "another buyer's legacy checkout must not block a new attempt"
  );
  assert.equal(
    await page.evaluate(() => window.cancelCalls.length),
    0,
    'another buyer session must not be canceled'
  );
  await page.evaluate(() => {
    window.cancelCalls = [];
    window.cancelError =
      'This checkout is complete. Check Billing for the payment result.';
  });
  await page.evaluate(() => {
    const base = 'stdb.premiumStore.checkoutAttempt';
    const key = base + '.' + window.stdb.identity;
    const saved = JSON.parse(localStorage.getItem(key));
    delete saved.owner;
    delete saved.recoverable;
    localStorage.setItem(base, JSON.stringify(saved));
    localStorage.removeItem(key);
  });
  assert.equal(
    await checkout(),
    subscription,
    'recent untagged legacy attempt must reuse its request'
  );
  const beforeExpiry = await page.evaluate(() => window.checkoutCalls.length);
  await page.evaluate(() => {
    const key = 'stdb.premiumStore.checkoutAttempt.' + window.stdb.identity;
    localStorage.setItem(
      key,
      JSON.stringify({
        ...JSON.parse(localStorage.getItem(key)),
        createdAt: Date.now() - 86400001,
        recoverable: undefined,
      })
    );
  });
  await page.locator('#btnCheckoutCart').click();
  await page.getByText(/cannot be verified automatically/).waitFor();
  assert.equal(
    await page.evaluate(() => window.checkoutCalls.length),
    beforeExpiry,
    'untagged saved checkout must not be replaced'
  );
  assert.equal(
    await page.evaluate(() => window.cancelCalls.length),
    0,
    'untagged checkout cannot use metadata lookup'
  );
  await page.evaluate(() => {
    const key = 'stdb.premiumStore.checkoutAttempt.' + window.stdb.identity;
    localStorage.setItem(
      key,
      JSON.stringify({
        ...JSON.parse(localStorage.getItem(key)),
        recoverable: true,
      })
    );
  });
  await page.locator('#btnCheckoutCart').click();
  await page.getByText(/This checkout is complete/).waitFor();
  assert.equal(
    await page.evaluate(() => window.checkoutCalls.length),
    beforeExpiry,
    'completed checkout must not be replaced'
  );
  assert.equal(
    await page.evaluate(() => window.cancelCalls.at(-1).attemptId),
    subscription
  );
  await page.evaluate(() => {
    window.cancelError = '';
  });
  assert.notEqual(
    await checkout(),
    subscription,
    'retired checkout must allow a fresh attempt'
  );
  await page.evaluate(() => {
    const key = 'stdb.premiumStore.checkoutAttempt.' + window.stdb.identity;
    localStorage.setItem(
      key,
      JSON.stringify({
        ...JSON.parse(localStorage.getItem(key)),
        sessionId: 'cs_paid',
      })
    );
    window.testBilling = {
      purchases: 0,
      subscriptions: [],
      checkouts: [
        {
          id: 'cs_paid',
          mode: 'subscription',
          paymentStatus: 'paid',
          fulfilled: false,
        },
      ],
    };
    window.dispatchEvent(new Event('stdb:ready'));
  });
  await page.locator('#btnCartClose').click();
  await page.getByRole('button', { name: 'Billing', exact: true }).click();
  await page.locator('#btnRefreshBilling').click();
  await page
    .getByRole('button', { name: 'Close billing', exact: true })
    .click();
  assert.equal(
    await page.getByRole('button', { name: 'Cart (1)', exact: true }).count(),
    1,
    'paid session without a receipt must not clear the cart'
  );
  await page.evaluate(() => {
    window.testBilling.checkouts[0].fulfilled = true;
    window.testBilling.purchases = 1;
    const base = 'stdb.premiumStore.checkoutAttempt';
    const scoped = base + '.' + window.stdb.identity;
    const saved = JSON.parse(localStorage.getItem(scoped));
    delete saved.owner;
    localStorage.setItem(base, JSON.stringify(saved));
    localStorage.removeItem(scoped);
    window.stdb.ownsCheckoutSession = () =>
      new Promise(resolve => {
        window.completeOwnerCheck = resolve;
      });
    window.dispatchEvent(new Event('stdb:ready'));
  });
  await page.waitForFunction(() => window.completeOwnerCheck);
  await page.evaluate(async () => {
    const base = 'stdb.premiumStore.checkoutAttempt';
    const scoped = base + '.' + window.stdb.identity;
    localStorage.setItem(
      scoped,
      JSON.stringify({
        ...JSON.parse(localStorage.getItem(base)),
        id: 'new_attempt',
        sessionId: 'cs_new',
        owner: window.stdb.identity,
      })
    );
    window.completeOwnerCheck(true);
    await new Promise(resolve => setTimeout(resolve, 0));
  });
  assert.equal(
    await page.evaluate(
      () =>
        JSON.parse(
          localStorage.getItem(
            'stdb.premiumStore.checkoutAttempt.' + window.stdb.identity
          )
        ).id
    ),
    'new_attempt',
    'late ownership result must not overwrite a newer checkout'
  );
  await page.evaluate(() => {
    localStorage.removeItem(
      'stdb.premiumStore.checkoutAttempt.' + window.stdb.identity
    );
    window.stdb.ownsCheckoutSession = async () => true;
    window.dispatchEvent(new Event('stdb:ready'));
  });
  await page.getByRole('button', { name: 'Cart (0)', exact: true }).waitFor();
  console.log(
    'PASS: same-cart retry, changed price, changed mode, and paid-cart clearing.'
  );
} finally {
  await browser.close();
}
