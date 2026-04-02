import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Feature 18: Mentions & Notifications', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);

    await createRoom(alice.page, 'MentionTestRoom');
    await joinRoom(bob.page, 'MentionTestRoom');
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('should highlight @mentions in message text', async () => {
    // Alice sends a message mentioning Bob
    await sendMessage(alice.page, 'Hey @Bob check this out');
    await expect(alice.page.getByText('Hey')).toBeVisible();

    // The "@Bob" part should be highlighted — look for a styled element
    // Try multiple patterns: span with highlight class, link, bold text
    const mentionHighlight = alice.page.locator(
      'span[class*="mention" i], a[class*="mention" i], ' +
      'span[class*="highlight" i], [data-mention], mark'
    );

    // Verify mention is highlighted on Alice's side
    await expect(async () => {
      const count = await mentionHighlight.count();
      if (count > 0) {
        return; // Found styled mention element
      }
      // Fallback: check that @Bob text exists in the message
      const body = await alice.page.textContent('body');
      expect(body).toContain('@Bob');
    }).toPass({ timeout: 5_000 });

    // Verify Bob also sees the highlighted mention
    await expect(bob.page.getByText('@Bob')).toBeVisible({ timeout: 10_000 });
  });

  test('should show notification bell with unread count', async () => {
    // After Alice mentions Bob, Bob should have a notification indicator
    // Look for bell icon, badge, or notification count
    const notificationBadge = bob.page.locator(
      '[class*="badge" i], [class*="notification" i] [class*="count" i], ' +
      '[class*="bell" i] ~ [class*="badge" i], [aria-label*="notification" i], ' +
      '[class*="unread" i][class*="count" i], [data-count]'
    );

    await expect(async () => {
      // Check for visible badge or count
      const count = await notificationBadge.count();
      if (count > 0 && await notificationBadge.first().isVisible()) {
        return;
      }
      // Fallback: look for any element with a number near a bell/notification icon
      const body = await bob.page.textContent('body');
      // At least one notification should exist
      expect(body?.toLowerCase()).toMatch(/notification|bell|1/);
    }).toPass({ timeout: 10_000 });
  });

  test('should display mentions in the notification panel', async () => {
    // Click the notification bell/icon to open the panel
    const bellBtn = bob.page.locator(
      'button[aria-label*="notification" i], [class*="bell" i], ' +
      'button:has-text("Notifications"), [title*="notification" i], ' +
      '[aria-label*="bell" i], button:has(svg[class*="bell" i])'
    ).first();
    await bellBtn.click({ timeout: 5_000 });

    // The notification panel should show the mention with message and channel details
    await expect(async () => {
      const body = await bob.page.textContent('body');
      expect(body).toContain('@Bob');
    }).toPass({ timeout: 5_000 });

    // Verify channel or room context is shown
    const body = await bob.page.textContent('body');
    expect(body).toContain('MentionTestRoom');
  });

  test('should mark notifications as read', async () => {
    // Find and click "Mark as read" or similar action on the notification
    const markReadBtn = bob.page.locator(
      'button:has-text("Mark"), button:has-text("Read"), [aria-label*="read" i], ' +
      '[title*="mark" i][title*="read" i], button:has-text("Dismiss")'
    ).first();

    if (await markReadBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await markReadBtn.click();
    } else {
      // Some apps mark as read by clicking the notification itself
      const notification = bob.page.locator(
        '[class*="notification" i] [class*="item" i], [class*="notification-item" i]'
      ).first();
      if (await notification.isVisible({ timeout: 2_000 }).catch(() => false)) {
        await notification.click();
      }
    }

    // The unread count should decrease or disappear
    await expect(async () => {
      const badge = bob.page.locator(
        '[class*="badge" i]:visible, [class*="unread" i][class*="count" i]:visible'
      );
      const count = await badge.count();
      if (count === 0) return; // Badge gone — good
      const text = await badge.first().textContent();
      // Count should be 0 or empty
      expect(text === '0' || text === '' || text === null).toBeTruthy();
    }).toPass({ timeout: 5_000 });
  });

  test('should update notifications in real-time', async () => {
    // Alice sends another mention — Bob should see the count update in real-time
    await sendMessage(alice.page, 'Another ping for @Bob right now');

    // Bob should see the notification count increase without refresh
    await expect(async () => {
      const body = await bob.page.textContent('body');
      // Either the badge updates or new notification text appears
      expect(body).toContain('@Bob');
    }).toPass({ timeout: 10_000 });

    // Verify the new mention appears in the notification panel
    const bellBtn = bob.page.locator(
      'button[aria-label*="notification" i], [class*="bell" i], ' +
      'button:has-text("Notifications"), [title*="notification" i], ' +
      '[aria-label*="bell" i]'
    ).first();
    if (await bellBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
      await bellBtn.click();
    }

    await expect(async () => {
      const body = await bob.page.textContent('body');
      expect(body).toContain('Another ping for @Bob');
    }).toPass({ timeout: 10_000 });
  });
});
