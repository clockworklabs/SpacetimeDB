import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';
const ROOM = `General-${RUN_ID}`;
const EPHEMERAL_MSG = `This message will disappear ${RUN_ID}`;

test.describe('Feature 6: Ephemeral Messages', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, `Alice-${RUN_ID}`, APP_URL);
    bob = await createUserContext(browser, `Bob-${RUN_ID}`, APP_URL);

    await joinRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('can send an ephemeral/disappearing message with duration', async () => {
    // Look for ephemeral/disappearing message toggle/button
    const ephemeralBtn = alice.page.locator(
      'button:has-text("Disappear"), button:has-text("Ephemeral"), ' +
      'button:has-text("Self-destruct"), button:has-text("Timer"), ' +
      '[aria-label*="ephemeral" i], [aria-label*="disappear" i], ' +
      '[aria-label*="timer" i], [title*="ephemeral" i], ' +
      '[title*="disappear" i]'
    ).first();

    await ephemeralBtn.click({ timeout: 5_000 });

    // Select a short duration for testing
    const durationSelect = alice.page.locator(
      'select, input[type="number"], ' +
      'input[placeholder*="second" i], input[placeholder*="duration" i]'
    ).first();

    if (await durationSelect.isVisible({ timeout: 3_000 }).catch(() => false)) {
      const tag = await durationSelect.evaluate(el => el.tagName.toLowerCase());
      if (tag === 'select') {
        const options = await durationSelect.locator('option').allTextContents();
        const shortOption = options.find(o => /\b[5-9]\b|10|15|30/.test(o)) || options[1] || options[0];
        await durationSelect.selectOption({ label: shortOption });
      } else {
        await durationSelect.fill('10');
      }
    }

    // Also try clicking a preset duration button
    const presetBtn = alice.page.locator(
      'button:has-text("5s"), button:has-text("10s"), button:has-text("30s"), ' +
      'button:has-text("5 sec"), button:has-text("10 sec"), button:has-text("30 sec"), ' +
      'button:has-text("1 min")'
    ).first();
    if (await presetBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
      await presetBtn.click();
    }

    // Send the ephemeral message
    await sendMessage(alice.page, EPHEMERAL_MSG);

    // Verify the message appears
    await expect(alice.page.getByText(EPHEMERAL_MSG).first()).toBeVisible({ timeout: 5_000 });
  });

  test('ephemeral message shows countdown or disappearing indicator', async () => {
    const bodyText = await alice.page.textContent('body');

    const hasIndicator =
      /expires|disappear|countdown|ephemeral|self.destruct|timer/i.test(bodyText || '') ||
      await alice.page.locator(
        '[class*="countdown" i], [class*="timer" i], [class*="ephemeral" i], ' +
        '[class*="disappear" i], [data-ephemeral], [data-expires]'
      ).first().isVisible({ timeout: 3_000 }).catch(() => false);

    expect(hasIndicator).toBeTruthy();
  });

  test('both users see the ephemeral message', async () => {
    await expect(
      bob.page.getByText(EPHEMERAL_MSG).first()
    ).toBeVisible({ timeout: 10_000 });
  });

  test('ephemeral message disappears after the duration expires', async () => {
    // Wait for the message to expire
    await alice.page.waitForTimeout(35_000);

    // The message text should no longer be visible
    await expect(
      alice.page.getByText(EPHEMERAL_MSG).first()
    ).not.toBeVisible({ timeout: 10_000 });

    // Bob should also no longer see it
    await expect(
      bob.page.getByText(EPHEMERAL_MSG).first()
    ).not.toBeVisible({ timeout: 5_000 });
  });
});
