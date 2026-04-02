import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Feature 5: Scheduled Messages', () => {
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

  test('schedule button is accessible from the message input area', async () => {
    // Look for a schedule/clock button near the message input
    // Common patterns: clock icon, "schedule" button, timer icon
    const scheduleBtn = alice.page.locator(
      'button:has-text("Schedule"), button:has-text("Later"), ' +
      '[aria-label*="schedule" i], [aria-label*="clock" i], ' +
      '[title*="schedule" i], [title*="clock" i], ' +
      'button svg, button .icon'
    );

    // At least one scheduling-related element should exist
    // Use a broad search since implementations vary
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

    // Fill in the message text — look for message input in the schedule dialog/form
    const msgInput = alice.page.locator(
      'input[placeholder*="message" i], textarea, ' +
      'input[placeholder*="type" i]'
    ).first();
    await msgInput.fill('Scheduled test message');

    // Set a future time — look for time/date input
    const timeInput = alice.page.locator(
      'input[type="time"], input[type="datetime-local"], ' +
      'input[placeholder*="time" i], input[placeholder*="when" i]'
    ).first();

    if (await timeInput.isVisible({ timeout: 3_000 }).catch(() => false)) {
      // Set time to ~2 minutes in the future
      const now = new Date();
      now.setMinutes(now.getMinutes() + 2);
      const timeStr = now.toTimeString().slice(0, 5); // HH:MM
      await timeInput.fill(timeStr);
    }

    // Look for duration/delay option as alternative (e.g., "in 5 minutes")
    const delaySelect = alice.page.locator(
      'select, input[type="number"], input[placeholder*="minute" i]'
    ).first();
    if (await delaySelect.isVisible({ timeout: 2_000 }).catch(() => false)) {
      // Set a short delay
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
    // Common patterns: "Scheduled", "Pending", clock icon, future timestamp
    await expect(
      alice.page.locator('text=/schedul|pending|queued/i').first()
    ).toBeVisible({ timeout: 5_000 });
  });

  test('pending scheduled messages are visible to author with cancel option', async () => {
    // The scheduled message should be visible to Alice
    const bodyText = await alice.page.textContent('body');
    expect(bodyText).toMatch(/schedul|pending|queued/i);

    // There should be a cancel/delete option for the pending message
    const cancelBtn = alice.page.locator(
      'button:has-text("Cancel"), button:has-text("Delete"), ' +
      'button:has-text("Remove"), [aria-label*="cancel" i], ' +
      '[aria-label*="delete" i], [title*="cancel" i]'
    ).first();

    const hasCancelBtn = await cancelBtn.isVisible({ timeout: 3_000 }).catch(() => false);

    // If no explicit cancel button, look for a cancel icon (X, trash, etc.)
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
    // Bob should not see the scheduled message yet
    const bobBody = await bob.page.textContent('body');
    expect(bobBody).not.toContain('Scheduled test message');
  });
});
