import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, joinRoom, APP_URL, APP_URL_B } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const ROOM = `General-${RUN_ID}`;

test.describe('Feature 5: Scheduled Messages', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, `Alice-${RUN_ID}`, APP_URL);
    bob = await createUserContext(browser, `Bob-${RUN_ID}`, APP_URL_B);

    await joinRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('schedule button is accessible from the message input area', async () => {
    // Look for a schedule/clock button near the message input
    const found = await alice.page.locator(
      '[aria-label*="schedule" i], [aria-label*="clock" i], ' +
      '[title*="schedule" i], button:has-text("Schedule"), ' +
      'button:has-text("Later")'
    ).first().isVisible({ timeout: 5_000 }).catch(() => false);

    // If no explicit schedule button, check for the feature in the UI text
    if (!found) {
      const bodyText = await alice.page.textContent('body');
      expect(bodyText).toMatch(/schedul|later|timer|clock/i);
    }
  });

  test('can schedule a message for the future', async () => {
    // Find and click the schedule button
    const scheduleBtn = alice.page.locator(
      'button:has-text("Schedule"), [aria-label*="schedule" i], ' +
      '[aria-label*="clock" i], [title*="schedule" i], ' +
      'button:has-text("Later")'
    ).first();

    await scheduleBtn.click({ timeout: 5_000 });

    // Fill in the message text
    const msgInput = alice.page.locator(
      'input[placeholder*="message" i], textarea, ' +
      'input[placeholder*="type" i]'
    ).first();
    const scheduledMsg = `Scheduled test ${RUN_ID}`;
    await msgInput.fill(scheduledMsg);

    // Set a future time
    const timeInput = alice.page.locator(
      'input[type="time"], input[type="datetime-local"], ' +
      'input[placeholder*="time" i], input[placeholder*="when" i]'
    ).first();

    if (await timeInput.isVisible({ timeout: 3_000 }).catch(() => false)) {
      const now = new Date();
      now.setMinutes(now.getMinutes() + 2);
      const timeStr = now.toTimeString().slice(0, 5);
      await timeInput.fill(timeStr);
    }

    // Look for duration/delay option as alternative
    const delaySelect = alice.page.locator(
      'select, input[type="number"], input[placeholder*="minute" i]'
    ).first();
    if (await delaySelect.isVisible({ timeout: 2_000 }).catch(() => false)) {
      await delaySelect.fill('2');
    }

    // Submit the scheduled message
    const submitBtn = alice.page.locator(
      'button:has-text("Schedule"), button:has-text("Confirm"), ' +
      'button:has-text("Set"), button:has-text("Save"), ' +
      'button[type="submit"]'
    ).first();
    await submitBtn.click({ timeout: 3_000 });

    // Verify the scheduled message appears as pending
    await expect(
      alice.page.locator('text=/schedul|pending|queued/i').first()
    ).toBeVisible({ timeout: 5_000 });
  });

  test('pending scheduled messages are visible to author with cancel option', async () => {
    const bodyText = await alice.page.textContent('body');
    expect(bodyText).toMatch(/schedul|pending|queued/i);

    // There should be a cancel/delete option for the pending message
    const cancelBtn = alice.page.locator(
      'button:has-text("Cancel"), button:has-text("Delete"), ' +
      'button:has-text("Remove"), [aria-label*="cancel" i], ' +
      '[aria-label*="delete" i], [title*="cancel" i]'
    ).first();

    const hasCancelBtn = await cancelBtn.isVisible({ timeout: 3_000 }).catch(() => false);

    if (!hasCancelBtn) {
      const cancelIcon = alice.page.locator(
        'button svg, .cancel, .delete, .remove'
      ).first();
      expect(
        await cancelIcon.isVisible({ timeout: 2_000 }).catch(() => false) || hasCancelBtn
      ).toBeTruthy();
    }
  });

  test('scheduled message is NOT visible to other users before delivery time', async () => {
    const scheduledMsg = `Scheduled test ${RUN_ID}`;
    const bobBody = await bob.page.textContent('body');
    expect(bobBody).not.toContain(scheduledMsg);
  });
});
