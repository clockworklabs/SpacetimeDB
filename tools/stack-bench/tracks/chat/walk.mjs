// The chat app's golden path: sign up -> create a room -> enter it -> send a
// message, checking each stage's contract hooks along the way.
//
// Extracted from lint.mjs unchanged when the harness gained tracks; the order of
// operations and every message is the same, because the contract linter's output
// is the record every existing result was measured against.

export async function walk({ page, args, hooks, byStage, blocked, checkHook, results, uniq, tid, CHECK_TIMEOUT }) {
  await page.goto(args.url, { waitUntil: 'domcontentloaded', timeout: 15000 });

  // Stage: landing (fresh identity -> registration UI must be shown)
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

  // Stage: main (registered -> rooms + online users)
  if (ok) {
    for (const h of byStage('main')) ok = (await checkHook(page, h, results)) && ok;
  } else blocked('main');

  // Create a room, then check hooks that need one to exist. The created room
  // must arrive in the list via the app's live update path (no reload) — this
  // is itself a contract requirement, so we wait for OUR room, not any room.
  const ourRoom = page.locator(tid('room-item'), { hasText: `lint-room-${uniq}` }).first();
  if (ok) {
    const nameInput = page.locator(tid('room-name-input')).first();
    if (!(await nameInput.isVisible().catch(() => false))) {
      await page.locator(tid('room-create')).first().click();
    }
    await nameInput.fill(`lint-room-${uniq}`);
    await page.locator(tid('room-name-submit')).first().click();
    for (const h of byStage('main-after-create')) {
      if (h.id === 'room-item') {
        try {
          await ourRoom.waitFor({ state: 'visible', timeout: CHECK_TIMEOUT });
          results.push({ id: h.id, status: 'PASS' });
        } catch {
          results.push({
            id: h.id, status: 'FAIL',
            detail: `created room "lint-room-${uniq}" but no ${tid(h.id)} containing it appeared without a reload`,
          });
          ok = false;
        }
      } else ok = (await checkHook(page, h, results)) && ok;
    }
  } else blocked('main-after-create');

  // Stage: room (enter our room, send a probe message via Enter).
  // Apps may implement click-to-join then click-to-enter; allow a second click.
  if (ok) {
    await ourRoom.click();
    const msgInput = page.locator(tid('message-input')).first();
    if (!(await msgInput.isVisible().catch(() => false))) {
      await page.waitForTimeout(750);
      if (!(await msgInput.isVisible().catch(() => false))) await ourRoom.click();
    }
    for (const h of byStage('room')) await checkHook(page, h, results); // non-blocking: check all
    ok = !results.some(r => r.status === 'FAIL' && hooks.find(h => h.id === r.id)?.stage === 'room');
  } else blocked('room');

  if (ok && !results.some(r => r.id === 'message-input' && r.status !== 'PASS')) {
    const probe = `lint probe ${uniq}`;
    await page.locator(tid('message-input')).first().fill(probe);
    await page.locator(tid('message-input')).first().press('Enter');
    for (const h of byStage('room-after-send')) {
      const loc = page.locator(tid(h.id), { hasText: probe }).first();
      try {
        await loc.waitFor({ state: 'visible', timeout: CHECK_TIMEOUT });
        results.push({ id: h.id, status: 'PASS' });
      } catch {
        results.push({
          id: h.id, status: 'FAIL',
          detail: `sent "${probe}" via Enter but no ${tid(h.id)} containing it appeared — expected: ${h.element}`,
        });
      }
    }
  } else blocked('room-after-send');

  for (const h of byStage('scenario')) {
    results.push({ id: h.id, status: 'SCENARIO', detail: h.note });
  }
}
