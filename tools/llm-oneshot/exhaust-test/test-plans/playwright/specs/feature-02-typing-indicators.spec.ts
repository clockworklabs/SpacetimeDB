import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, triggerTyping, joinRoom, APP_URL, APP_URL_B } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const ROOM = `General-${RUN_ID}`;

test.describe('Feature 2: Typing Indicators', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, `Alice-${RUN_ID}`, APP_URL);
    bob = await createUserContext(browser, `Bob-${RUN_ID}`, APP_URL_B);

    // Both users need to be in the same room
    await joinRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('typing state broadcasts to other users', async () => {
    await triggerTyping(bob.page, 'hello...');

    // Alice should see typing indicator within a few seconds
    await expect(
      alice.page.getByText(/typing/i).first()
    ).toBeVisible({ timeout: 5_000 });
  });

  test('typing indicator auto-expires after inactivity', async () => {
    await triggerTyping(bob.page, 'still typing...');

    // Verify it appears
    await expect(alice.page.getByText(/typing/i).first()).toBeVisible({ timeout: 5_000 });

    // Wait for auto-expiry (typically 3-5 seconds)
    await alice.page.waitForTimeout(6_000);

    // Typing indicator should be gone
    await expect(alice.page.getByText(/typing/i).first()).not.toBeVisible({ timeout: 3_000 });
  });

  test('typing indicator displays correctly in UI', async () => {
    await triggerTyping(bob.page, 'test display...');

    const bodyText = await alice.page.textContent('body');
    // Should mention who is typing (using RUN_ID-suffixed name)
    expect(bodyText).toMatch(new RegExp(`Bob-${RUN_ID}.*typing|typing.*Bob-${RUN_ID}`, 'i'));
  });
});
