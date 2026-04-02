import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, triggerTyping } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Feature 2: Typing Indicators', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);

    // Both users need to be in the same room
    // Assumes Feature 1 room "General" exists; join it
    await alice.page.getByText('General').click();
    await bob.page.getByText('General').click();

    // Click Join if needed
    for (const user of [alice, bob]) {
      const joinBtn = user.page.locator('button:has-text("Join")').first();
      if (await joinBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
        await joinBtn.click();
      }
    }
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('typing state broadcasts to other users', async () => {
    await triggerTyping(bob.page, 'hello...');

    // Alice should see typing indicator within a few seconds
    await expect(
      alice.page.getByText(/typing/i)
    ).toBeVisible({ timeout: 5_000 });
  });

  test('typing indicator auto-expires after inactivity', async () => {
    await triggerTyping(bob.page, 'still typing...');

    // Verify it appears
    await expect(alice.page.getByText(/typing/i)).toBeVisible({ timeout: 5_000 });

    // Wait for auto-expiry (typically 3-5 seconds)
    await alice.page.waitForTimeout(6_000);

    // Typing indicator should be gone
    await expect(alice.page.getByText(/typing/i)).not.toBeVisible({ timeout: 3_000 });
  });

  test('typing indicator displays correctly in UI', async () => {
    await triggerTyping(bob.page, 'test display...');

    const bodyText = await alice.page.textContent('body');
    // Should mention who is typing
    expect(bodyText).toMatch(/bob.*typing|typing.*bob/i);
  });
});
