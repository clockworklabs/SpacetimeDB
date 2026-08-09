// The store's golden path: browse signed out -> sign up -> search -> open an
// item and review it -> fill a cart -> check out -> sign in as the seeded admin.
//
// One pass, one browser, one customer. Anything needing a second customer, a
// concurrent action or a refusal belongs to the scenario suites, not here.

const ADMIN_USER = 'admin';
const ADMIN_PASS = 'stackbench-admin-2026';

// Seeded items the walk relies on. `SEARCH_ONLY` is outside the opening top ten
// (the catalogue starts with no purchases, so the storefront is alphabetical),
// which is what makes searching for it prove that search covers the catalogue
// rather than filtering what is already on screen.
const CART_ITEM = 'Headphones';
const REVIEW_ITEM = 'Air Purifier';
const SEARCH_ONLY = 'Webcam';

export async function walk({ page, args, byStage, blocked, checkHook, results, uniq, tid, CHECK_TIMEOUT }) {
  const fail = (h, detail) => { results.push({ id: h.id, status: 'FAIL', detail }); };
  const pass = h => { results.push({ id: h.id, status: 'PASS' }); };

  await page.goto(args.url, { waitUntil: 'domcontentloaded', timeout: 15000 });

  // Stage: landing — the storefront and its prices are public, so everything
  // here must resolve before anyone signs in.
  let ok = true;
  for (const h of byStage('landing')) ok = (await checkHook(page, h, results)) && ok;

  if (ok) {
    // Alphanumeric on purpose. This was `lint-${uniq}`; the spec never states
    // which characters a username must accept, so an app validating them as
    // letters, digits and underscore rejected every account the linter made,
    // and the whole golden path aborted at sign-up.
    await page.locator(tid('signup-username')).first().fill(`lint${uniq}`);
    await page.locator(tid('signup-password')).first().fill(`pwlint${uniq}`);
    await page.locator(tid('signup-submit')).first().click();
  }

  // Stage: storefront — signing up turns the buying controls on.
  if (ok) {
    for (const h of byStage('storefront')) ok = (await checkHook(page, h, results)) && ok;
  } else blocked('storefront');

  // Reviews are gated on having bought the item, so the walk earns the right
  // before it exercises the form — otherwise a spec-compliant refusal would
  // read as a missing hook.
  if (ok) {
    await page.locator(tid('item-card'), { hasText: REVIEW_ITEM }).first()
      .locator(tid('buy-now')).first().click();
    await page.waitForTimeout(1200);
  }

  // Stage: search-results — search for an item the storefront is not showing.
  if (ok) {
    const search = page.locator(tid('search-input')).first();
    await search.fill(SEARCH_ONLY);
    await search.press('Enter').catch(() => {});
    for (const h of byStage('search-results')) {
      if (h.id === 'search-results') {
        const hit = page.locator(tid('search-results')).first();
        try {
          await hit.waitFor({ state: 'attached', timeout: CHECK_TIMEOUT });
          const card = hit.locator(tid('item-card'), { hasText: SEARCH_ONLY }).first();
          await card.waitFor({ state: 'visible', timeout: CHECK_TIMEOUT });
          pass(h);
        } catch {
          fail(h, `searched for "${SEARCH_ONLY}" but no ${tid('item-card')} containing it appeared inside ${tid(h.id)} — search must cover the whole catalogue, not just the items on the storefront`);
          ok = false;
        }
      } else ok = (await checkHook(page, h, results)) && ok;
    }
    await search.fill('');
    await search.press('Enter').catch(() => {});
  } else blocked('search-results');

  // Stage: item — open one item's detail view from its card.
  if (ok) {
    const card = page.locator(tid('item-card'), { hasText: REVIEW_ITEM }).first();
    try {
      await card.waitFor({ state: 'visible', timeout: CHECK_TIMEOUT });
      await card.locator(tid('item-name')).first().click();
    } catch {
      await card.click().catch(() => {});
    }
    for (const h of byStage('item')) ok = (await checkHook(page, h, results)) && ok;
  } else blocked('item');

  // Stage: item-after-review — the review must arrive without a reload.
  const probe = `lint review ${uniq}`;
  if (ok) {
    const rating = page.locator(tid('review-rating')).first();
    // A rating control can be a select, a number input or a row of buttons.
    await rating.selectOption('5')
      .catch(async () => { await rating.fill('5').catch(async () => { await rating.click().catch(() => {}); }); });
    await page.locator(tid('review-input')).first().fill(probe);
    await page.locator(tid('review-submit')).first().click();
    for (const h of byStage('item-after-review')) {
      const loc = page.locator(tid(h.id), { hasText: probe }).first();
      try {
        await loc.waitFor({ state: 'visible', timeout: CHECK_TIMEOUT });
        pass(h);
      } catch {
        fail(h, `submitted review "${probe}" but no ${tid(h.id)} containing it appeared without a reload — expected: ${h.element}`);
        ok = false;
      }
    }
  } else blocked('item-after-review');

  // Stage: cart — add a known item, then open the cart.
  if (ok) {
    const card = page.locator(tid('item-card'), { hasText: CART_ITEM }).first();
    if (!(await card.isVisible().catch(() => false))) {
      // The item detail view may still be covering the storefront.
      await page.goto(args.url, { waitUntil: 'domcontentloaded', timeout: 15000 });
    }
    await page.locator(tid('item-card'), { hasText: CART_ITEM }).first()
      .locator(tid('add-to-cart')).first().click();
    await page.locator(tid('cart-toggle')).first().click();
    for (const h of byStage('cart')) ok = (await checkHook(page, h, results)) && ok;
  } else blocked('cart');

  // Stage: after-checkout — the order must show the item that was bought.
  if (ok) {
    await page.locator(tid('checkout-submit')).first().click();
    await page.waitForTimeout(1500);
    const orders = page.locator(tid('orders-toggle')).first();
    if (await orders.isVisible().catch(() => false)) await orders.click();
    for (const h of byStage('after-checkout')) {
      if (h.id === 'order-item') {
        const loc = page.locator(tid(h.id), { hasText: CART_ITEM }).first();
        try {
          await loc.waitFor({ state: 'visible', timeout: CHECK_TIMEOUT });
          pass(h);
        } catch {
          fail(h, `checked out a cart containing "${CART_ITEM}" but no ${tid(h.id)} naming it appeared — expected: ${h.element}`);
          ok = false;
        }
      } else ok = (await checkHook(page, h, results)) && ok;
    }
  } else blocked('after-checkout');

  // Stage: admin — a separate account with its own area. Signing out and back
  // in as the seeded admin is the only way to reach it.
  if (ok) {
    await page.locator(tid('signout')).first().click();
    await page.waitForTimeout(1000);
    const toggle = page.locator(tid('signin-toggle')).first();
    if (await toggle.count()) await toggle.click().catch(() => {});
    await page.locator(tid('signin-username')).first().fill(ADMIN_USER);
    await page.locator(tid('signin-password')).first().fill(ADMIN_PASS);
    await page.locator(tid('signin-submit')).first().click();
    await page.waitForTimeout(1500);
    const link = page.locator(tid('admin-link')).first();
    if (await link.isVisible().catch(() => false)) await link.click();
    for (const h of byStage('admin')) await checkHook(page, h, results); // check all, non-blocking
  } else blocked('admin');

  for (const h of byStage('scenario')) {
    results.push({ id: h.id, status: 'SCENARIO', detail: h.note });
  }
}
