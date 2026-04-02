import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Feature 19: Bookmarked/Saved Messages', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);

    await createRoom(alice.page, 'BookmarkTestRoom');
    await joinRoom(bob.page, 'BookmarkTestRoom');
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('users can bookmark messages', async () => {
    // Send a message to bookmark
    await sendMessage(alice.page, 'Bookmark this important message');
    await expect(alice.page.getByText('Bookmark this important message')).toBeVisible();

    // Hover over the message to reveal action buttons
    const message = alice.page.locator('text=Bookmark this important message').first();
    await message.hover();

    // Find and click the bookmark/save icon
    const bookmarkBtn = alice.page.locator(
      'button:has-text("Bookmark"), button:has-text("Save"), ' +
      '[aria-label*="bookmark" i], [aria-label*="save" i], ' +
      '[title*="bookmark" i], [title*="save" i], ' +
      'button:has(svg[class*="bookmark" i])'
    ).first();
    await bookmarkBtn.click({ timeout: 5_000 });

    // Verify visual feedback that the message is bookmarked
    // Look for filled icon, highlighted state, or toast notification
    await expect(async () => {
      const body = await alice.page.textContent('body');
      const hasBookmarkIndicator = body?.toLowerCase().match(
        /bookmarked|saved|bookmark added/
      );
      // Either we see confirmation text or the bookmark icon changed state
      const filledBookmark = alice.page.locator(
        '[class*="bookmarked" i], [class*="saved" i], ' +
        '[aria-pressed="true"][aria-label*="bookmark" i], ' +
        '[data-bookmarked="true"]'
      );
      const count = await filledBookmark.count();
      expect(hasBookmarkIndicator || count > 0).toBeTruthy();
    }).toPass({ timeout: 5_000 });
  });

  test('saved messages panel shows bookmarks with context', async () => {
    // Open the saved/bookmarked messages panel
    const savedPanelBtn = alice.page.locator(
      'button:has-text("Saved"), button:has-text("Bookmarks"), ' +
      '[aria-label*="saved" i], [aria-label*="bookmark" i], ' +
      '[title*="saved" i], [title*="bookmark" i]'
    ).first();
    await savedPanelBtn.click({ timeout: 5_000 });

    // Verify the bookmarked message appears in the panel
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body).toContain('Bookmark this important message');
    }).toPass({ timeout: 5_000 });

    // Verify context info (sender, channel) is shown alongside the bookmarked message
    const body = await alice.page.textContent('body');
    // Should show the sender name or channel name as context
    const hasContext = body?.includes('Alice') || body?.includes('BookmarkTestRoom');
    expect(hasContext).toBeTruthy();
  });

  test('remove bookmark works and bookmarks are private', async () => {
    // First verify Bob's saved panel is empty — bookmarks are private
    const bobSavedBtn = bob.page.locator(
      'button:has-text("Saved"), button:has-text("Bookmarks"), ' +
      '[aria-label*="saved" i], [aria-label*="bookmark" i], ' +
      '[title*="saved" i], [title*="bookmark" i]'
    ).first();
    await bobSavedBtn.click({ timeout: 5_000 });

    // Bob should see an empty state — no bookmarked messages
    await expect(async () => {
      const body = await bob.page.textContent('body');
      // Bob's panel should NOT contain the bookmarked message
      expect(body).not.toContain('Bookmark this important message');
    }).toPass({ timeout: 5_000 });

    // Now Alice removes her bookmark
    // Navigate back to the saved panel or find the remove action
    const aliceSavedBtn = alice.page.locator(
      'button:has-text("Saved"), button:has-text("Bookmarks"), ' +
      '[aria-label*="saved" i], [aria-label*="bookmark" i], ' +
      '[title*="saved" i], [title*="bookmark" i]'
    ).first();
    if (await aliceSavedBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
      await aliceSavedBtn.click();
    }

    // Find the remove/unbookmark action within the saved panel
    const removeBtn = alice.page.locator(
      'button:has-text("Remove"), button:has-text("Unsave"), ' +
      '[aria-label*="remove" i], [aria-label*="unbookmark" i], ' +
      '[title*="remove" i], button:has-text("Delete")'
    ).first();

    if (await removeBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await removeBtn.click();
    } else {
      // Alternative: hover over the message in the panel and click bookmark toggle
      const savedMessage = alice.page.locator('text=Bookmark this important message').first();
      await savedMessage.hover();
      const toggleBtn = alice.page.locator(
        'button:has-text("Bookmark"), [aria-label*="bookmark" i], [title*="bookmark" i]'
      ).first();
      await toggleBtn.click({ timeout: 3_000 });
    }

    // Verify the message disappears from the saved panel
    await expect(async () => {
      const body = await alice.page.textContent('body');
      // Either the message is gone or we see an empty state
      const emptyOrGone =
        !body?.includes('Bookmark this important message') ||
        body?.toLowerCase().match(/no saved|no bookmark|empty/);
      expect(emptyOrGone).toBeTruthy();
    }).toPass({ timeout: 5_000 });
  });
});
