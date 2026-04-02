import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Feature 4: Unread Counts', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);

    // Create a second room so Bob can be "away" from General
    await createRoom(alice.page, 'Unread-Test');

    // Both join both rooms
    await joinRoom(alice.page, 'General');
    await joinRoom(bob.page, 'General');
    await joinRoom(bob.page, 'Unread-Test');
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('unread count badge appears when messages arrive in another room', async () => {
    // Move Bob to the Unread-Test room so he is NOT viewing General
    await joinRoom(bob.page, 'Unread-Test');

    // Alice sends messages in General
    await joinRoom(alice.page, 'General');
    await sendMessage(alice.page, 'Unread msg 1');
    await sendMessage(alice.page, 'Unread msg 2');

    // Bob should see a badge/count on the General room in the sidebar
    // Common patterns: "(2)", badge with number, notification dot
    // Look for a number near the room name in the sidebar
    await expect(
      bob.page.locator('text=/\\b[1-9]\\d*\\b/').first()
    ).toBeVisible({ timeout: 10_000 });

    // More specific: look for an element containing a count near "General"
    const sidebar = await bob.page.textContent('body');
    // Should have some numeric indicator — at least "1" or "2"
    expect(sidebar).toMatch(/General[\s\S]{0,50}[1-9]|[1-9][\s\S]{0,50}General/);
  });

  test('badge clears when room is opened', async () => {
    // Bob clicks on General to view it
    await joinRoom(bob.page, 'General');

    // Wait a moment for the unread state to clear
    await bob.page.waitForTimeout(1_000);

    // The unread messages should now be visible
    await expect(bob.page.getByText('Unread msg 1')).toBeVisible();
    await expect(bob.page.getByText('Unread msg 2')).toBeVisible();

    // Now navigate away and back — no badge should appear since we read them
    await joinRoom(bob.page, 'Unread-Test');
    await bob.page.waitForTimeout(500);

    // Check that there's no unread count for General anymore
    // The number badge near General should be gone
    const sidebarText = await bob.page.textContent('body');
    // After reading, should NOT have a numeric badge adjacent to General
    // This is a soft check — if the badge text pattern is gone, we pass
    const generalSection = sidebarText?.match(/General[\s\S]{0,30}/)?.[0] || '';
    // Should not contain a standalone digit (unread count)
    expect(generalSection).not.toMatch(/\b[1-9]\d*\b/);
  });

  test('counts are per-user — Alice does not see unread for her own messages', async () => {
    // Alice is in General where she sent messages — she should NOT see unread badge
    await joinRoom(alice.page, 'General');
    await alice.page.waitForTimeout(500);

    // Navigate away
    await joinRoom(alice.page, 'Unread-Test');
    await alice.page.waitForTimeout(500);

    // General should not show unread for Alice (she sent those messages)
    const aliceSidebar = await alice.page.textContent('body');
    const aliceGeneralSection = aliceSidebar?.match(/General[\s\S]{0,30}/)?.[0] || '';
    expect(aliceGeneralSection).not.toMatch(/\b[1-9]\d*\b/);
  });
});
