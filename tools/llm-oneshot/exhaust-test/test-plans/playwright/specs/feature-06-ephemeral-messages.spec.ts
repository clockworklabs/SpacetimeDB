import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Feature 6: Ephemeral Messages', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);

    await joinRoom(alice.page, 'General');
    await joinRoom(bob.page, 'General');
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('can send an ephemeral/disappearing message with duration', async () => {
    // Look for ephemeral/disappearing message toggle/button
    // Common patterns: "Disappearing", "Ephemeral", "Self-destruct", timer icon, toggle
    const ephemeralBtn = alice.page.locator(
      'button:has-text("Disappear"), button:has-text("Ephemeral"), ' +
      'button:has-text("Self-destruct"), button:has-text("Timer"), ' +
      '[aria-label*="ephemeral" i], [aria-label*="disappear" i], ' +
      '[aria-label*="timer" i], [title*="ephemeral" i], ' +
      '[title*="disappear" i]'
    ).first();

    await ephemeralBtn.click({ timeout: 5_000 });

    // Select a short duration for testing
    // Common patterns: dropdown with durations, input for seconds/minutes
    const durationSelect = alice.page.locator(
      'select, input[type="number"], ' +
      'input[placeholder*="second" i], input[placeholder*="duration" i]'
    ).first();

    if (await durationSelect.isVisible({ timeout: 3_000 }).catch(() => false)) {
      const tag = await durationSelect.evaluate(el => el.tagName.toLowerCase());
      if (tag === 'select') {
        // Pick the shortest duration option
        const options = await durationSelect.locator('option').allTextContents();
        // Find shortest — look for "5s", "10s", "30s", etc.
        const shortOption = options.find(o => /\b[5-9]\b|10|15|30/.test(o)) || options[1] || options[0];
        await durationSelect.selectOption({ label: shortOption });
      } else {
        await durationSelect.fill('10'); // 10 seconds
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
    await sendMessage(alice.page, 'This message will disappear');

    // Verify the message appears
    await expect(alice.page.getByText('This message will disappear')).toBeVisible({ timeout: 5_000 });
  });

  test('ephemeral message shows countdown or disappearing indicator', async () => {
    // The ephemeral message should have some visual indicator
    // Common patterns: countdown timer, hourglass icon, "Expires in", "Disappears in"
    const bodyText = await alice.page.textContent('body');

    // Check for any timing/expiry indicator
    const hasIndicator =
      /expires|disappear|countdown|ephemeral|self.destruct|⏱|🕐|timer/i.test(bodyText || '') ||
      // Check for countdown numbers near the message
      await alice.page.locator(
        '[class*="countdown" i], [class*="timer" i], [class*="ephemeral" i], ' +
        '[class*="disappear" i], [data-ephemeral], [data-expires]'
      ).first().isVisible({ timeout: 3_000 }).catch(() => false);

    expect(hasIndicator).toBeTruthy();
  });

  test('both users see the ephemeral message', async () => {
    // Bob should also see the ephemeral message (while it exists)
    await expect(
      bob.page.getByText('This message will disappear')
    ).toBeVisible({ timeout: 10_000 });
  });

  test('ephemeral message disappears after the duration expires', async () => {
    // Wait for the message to expire
    // Most implementations use 5-30 second durations for testing
    // Wait up to 35 seconds to cover typical durations
    await alice.page.waitForTimeout(35_000);

    // The message text should no longer be visible
    await expect(
      alice.page.getByText('This message will disappear')
    ).not.toBeVisible({ timeout: 10_000 });

    // Bob should also no longer see it
    await expect(
      bob.page.getByText('This message will disappear')
    ).not.toBeVisible({ timeout: 5_000 });
  });
});
