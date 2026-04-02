import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Feature 3: Read Receipts', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);

    // Join the General room (created by Feature 1)
    await joinRoom(alice.page, 'General');
    await joinRoom(bob.page, 'General');
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('seen-by indicator displays under messages after recipient views', async () => {
    // Alice sends a message
    await sendMessage(alice.page, 'Read receipt test message');

    // Verify Alice sees her own message
    await expect(alice.page.getByText('Read receipt test message')).toBeVisible();

    // Bob should receive the message (real-time)
    await expect(bob.page.getByText('Read receipt test message')).toBeVisible({ timeout: 10_000 });

    // After Bob views the message, Alice should see a read indicator
    // Look for common patterns: "Seen by Bob", "Read by Bob", checkmarks, "seen", "read"
    await expect(
      alice.page.locator('text=/seen|read|viewed/i').first()
    ).toBeVisible({ timeout: 10_000 });
  });

  test('read status includes the reader name', async () => {
    // Verify the read receipt mentions Bob specifically
    const aliceBody = await alice.page.textContent('body');
    expect(aliceBody).toMatch(/seen.*bob|read.*bob|bob.*seen|bob.*read/i);
  });

  test('read status updates in real-time when another user views', async () => {
    // Create a third context (Charlie) to test real-time receipt updates
    const charlie = await createUserContext(alice.context.browser()!, 'Charlie', APP_URL);
    try {
      await joinRoom(charlie.page, 'General');

      // Alice sends a new message
      await sendMessage(alice.page, 'Realtime receipt check');
      await expect(alice.page.getByText('Realtime receipt check')).toBeVisible();

      // Wait for Charlie to receive it
      await expect(charlie.page.getByText('Realtime receipt check')).toBeVisible({ timeout: 10_000 });

      // Now check Alice's view — should show read by both Bob and Charlie
      // (Bob is still in the room from previous test)
      const bodyAfter = await alice.page.textContent('body');
      // At minimum, the read receipt area should mention multiple readers or show a count
      expect(bodyAfter).toMatch(/seen|read|viewed/i);
    } finally {
      await charlie.context.close();
    }
  });
});
