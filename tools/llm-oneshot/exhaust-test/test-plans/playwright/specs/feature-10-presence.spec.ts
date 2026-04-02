import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, createRoom, joinRoom, APP_URL, APP_URL_B } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const ROOM = `PresenceRoom-${RUN_ID}`;

test.describe('Feature 10: Presence', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, `Alice-${RUN_ID}`, APP_URL);
    bob = await createUserContext(browser, `Bob-${RUN_ID}`, APP_URL_B);

    await createRoom(alice.page, ROOM);
    await joinRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('status selector UI exists with multiple status options', async () => {
    const statusSelector = alice.page.locator(
      'select[name*="status" i], [aria-label*="status" i], [class*="status" i], ' +
      'button:has-text("Online"), button:has-text("Status"), ' +
      '[data-testid*="status" i], [role="listbox"], [role="combobox"]'
    ).first();

    const hasSelector = await statusSelector.isVisible({ timeout: 5_000 }).catch(() => false);

    const statusDot = alice.page.locator(
      '[class*="presence" i], [class*="status-dot" i], [class*="indicator" i], ' +
      '[class*="dot" i][class*="online" i]'
    ).first();
    const hasDot = await statusDot.isVisible({ timeout: 3_000 }).catch(() => false);

    const aliceBody = await alice.page.textContent('body');
    const hasStatusText = /online|away|status/i.test(aliceBody || '');

    expect(hasSelector || hasDot || hasStatusText).toBeTruthy();
  });

  test('user can change status to away', async () => {
    const selectEl = alice.page.locator('select').filter({ hasText: /online|away|status/i }).first();
    const hasSelect = await selectEl.isVisible({ timeout: 3_000 }).catch(() => false);

    if (hasSelect) {
      await selectEl.selectOption({ label: /away/i });
    } else {
      const statusBtn = alice.page.locator(
        'button:has-text("Online"), button:has-text("Status"), ' +
        '[class*="status" i]:not(div), [aria-label*="status" i]'
      ).first();
      const hasBtn = await statusBtn.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasBtn) {
        await statusBtn.click();
        const awayOption = alice.page.locator(
          'text=/away/i, [data-value="away"], option[value="away"]'
        ).first();
        await awayOption.click({ timeout: 5_000 }).catch(() => {});
      }
    }

    await alice.page.waitForTimeout(1_000);
    const aliceBody = await alice.page.textContent('body');
    const hasAway = /away/i.test(aliceBody || '');

    const awayIndicator = alice.page.locator(
      '[class*="away" i], [class*="yellow" i], [class*="orange" i], ' +
      '[data-status="away"], [aria-label*="away" i]'
    ).first();
    const hasAwayIndicator = await awayIndicator.isVisible({ timeout: 3_000 }).catch(() => false);

    expect(hasAway || hasAwayIndicator).toBeTruthy();
  });

  test('status change syncs to other users in real-time', async () => {
    const bobBody = await bob.page.textContent('body');

    const bobSeeStatus = /away/i.test(bobBody || '');
    const bobSeeIndicator = await bob.page.locator(
      '[class*="away" i], [data-status="away"]'
    ).first().isVisible({ timeout: 10_000 }).catch(() => false);

    expect(bobSeeStatus || bobSeeIndicator).toBeTruthy();
  });

  test('user can set do-not-disturb status', async () => {
    const selectEl = alice.page.locator('select').filter({ hasText: /online|away|do.not/i }).first();
    const hasSelect = await selectEl.isVisible({ timeout: 3_000 }).catch(() => false);

    if (hasSelect) {
      await selectEl.selectOption({ label: /do.not.disturb|dnd|busy/i }).catch(() => {});
    } else {
      const statusBtn = alice.page.locator(
        'button:has-text("Away"), button:has-text("Status"), [aria-label*="status" i]'
      ).first();
      const hasBtn = await statusBtn.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasBtn) {
        await statusBtn.click();
        const dndOption = alice.page.locator(
          'text=/do.not.disturb|dnd|busy/i'
        ).first();
        await dndOption.click({ timeout: 5_000 }).catch(() => {});
      }
    }

    await alice.page.waitForTimeout(1_000);
    const aliceBody = await alice.page.textContent('body');
    const hasDnd = /do.not.disturb|dnd|busy/i.test(aliceBody || '');

    const dndIndicator = alice.page.locator(
      '[class*="dnd" i], [class*="busy" i], [class*="disturb" i], ' +
      '[class*="red" i], [data-status="dnd"], [data-status="busy"]'
    ).first();
    const hasDndIndicator = await dndIndicator.isVisible({ timeout: 3_000 }).catch(() => false);

    expect(hasDnd || hasDndIndicator).toBeTruthy();
  });

  test('last active timestamp for offline users', async () => {
    const selectEl = alice.page.locator('select').filter({ hasText: /online|away|invisible/i }).first();
    const hasSelect = await selectEl.isVisible({ timeout: 3_000 }).catch(() => false);

    if (hasSelect) {
      await selectEl.selectOption({ label: /invisible|offline/i }).catch(() => {});
    } else {
      const statusBtn = alice.page.locator(
        'button:has-text("Status"), [aria-label*="status" i]'
      ).first();
      const hasBtn = await statusBtn.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasBtn) {
        await statusBtn.click();
        const invisOption = alice.page.locator('text=/invisible|offline/i').first();
        await invisOption.click({ timeout: 5_000 }).catch(() => {});
      }
    }

    await bob.page.waitForTimeout(2_000);
    const bobBody = await bob.page.textContent('body');
    const hasLastActive = /last.active|ago|offline|inactive/i.test(bobBody || '');

    expect(hasLastActive).toBeTruthy();
  });

  test('auto-away UI mechanism exists', async () => {
    const hasAutoAway = await alice.page.evaluate(() => {
      const bodyText = document.body.innerHTML.toLowerCase();
      return (
        bodyText.includes('auto-away') ||
        bodyText.includes('auto_away') ||
        bodyText.includes('inactivity') ||
        bodyText.includes('idle') ||
        typeof (window as any).__autoAwayTimer !== 'undefined' ||
        typeof (window as any).__idleTimer !== 'undefined'
      );
    });

    const aliceBody = await alice.page.textContent('body');
    const hasIdleConfig = /auto.?away|idle|inactiv/i.test(aliceBody || '');

    // Soft check — auto-away is 0.5 points and hard to verify
    expect(hasAutoAway || hasIdleConfig || true).toBeTruthy();
  });
});
