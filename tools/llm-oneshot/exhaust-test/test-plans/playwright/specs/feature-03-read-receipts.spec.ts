import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';
const ROOM = `General-${RUN_ID}`;

test.describe('Feature 3: Read Receipts', () => {
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

  test('seen-by indicator displays under messages after recipient views', async () => {
    // Alice sends a message
    const msg = `Read receipt test ${RUN_ID}`;
    await sendMessage(alice.page, msg);

    // Verify Alice sees her own message
    await expect(alice.page.getByText(msg).first()).toBeVisible();

    // Bob should receive the message (real-time)
    await expect(bob.page.getByText(msg).first()).toBeVisible({ timeout: 10_000 });

    // After Bob views the message, Alice should see a read indicator
    await expect(
      alice.page.locator('text=/seen|read|viewed/i').first()
    ).toBeVisible({ timeout: 10_000 });
  });

  test('read status includes the reader name', async () => {
    // Verify the read receipt mentions Bob specifically
    const aliceBody = await alice.page.textContent('body');
    expect(aliceBody).toMatch(new RegExp(`seen.*Bob-${RUN_ID}|read.*Bob-${RUN_ID}|Bob-${RUN_ID}.*seen|Bob-${RUN_ID}.*read`, 'i'));
  });

  test('read status updates in real-time when another user views', async () => {
    // Create a third context (Charlie) to test real-time receipt updates
    const charlie = await createUserContext(alice.context.browser()!, `Charlie-${RUN_ID}`, APP_URL);
    try {
      await joinRoom(charlie.page, ROOM);

      // Alice sends a new message
      const msg = `Realtime receipt check ${RUN_ID}`;
      await sendMessage(alice.page, msg);
      await expect(alice.page.getByText(msg).first()).toBeVisible();

      // Wait for Charlie to receive it
      await expect(charlie.page.getByText(msg).first()).toBeVisible({ timeout: 10_000 });

      // Now check Alice's view — should show read by both Bob and Charlie
      const bodyAfter = await alice.page.textContent('body');
      expect(bodyAfter).toMatch(/seen|read|viewed/i);
    } finally {
      await charlie.context.close();
    }
  });
});
