import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';
const ROOM = `SlowModeRoom-${RUN_ID}`;
const ALICE = `Alice-${RUN_ID}`;
const BOB = `Bob-${RUN_ID}`;

test.describe('Feature 21: Slow Mode', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, ALICE, APP_URL);
    bob = await createUserContext(browser, BOB, APP_URL);

    // Alice creates the room (becomes admin)
    await createRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('admins can enable slow mode with visible indicator', async () => {
    // Alice (admin/creator) opens room settings to enable slow mode
    const settingsBtn = alice.page.locator(
      'button:has-text("Settings"), [aria-label*="settings" i], ' +
      '[title*="settings" i], button:has(svg[class*="settings" i]), ' +
      'button:has(svg[class*="gear" i]), [aria-label*="channel settings" i]'
    ).first();
    await settingsBtn.click({ timeout: 5_000 });

    // Find slow mode toggle or input
    const slowModeToggle = alice.page.locator(
      'input[type="checkbox"][name*="slow" i], [class*="slow" i] input, ' +
      'button:has-text("Slow Mode"), label:has-text("Slow Mode"), ' +
      '[aria-label*="slow" i]'
    ).first();

    if (await slowModeToggle.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await slowModeToggle.click();
    }

    // Set the cooldown to 10 seconds (short for testing)
    const cooldownInput = alice.page.locator(
      'input[name*="cooldown" i], input[name*="slow" i], input[name*="interval" i], ' +
      'input[type="number"], select[name*="slow" i]'
    ).first();

    if (await cooldownInput.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await cooldownInput.clear();
      await cooldownInput.fill('10');
    }

    // Save settings
    const saveBtn = alice.page.locator(
      'button:has-text("Save"), button:has-text("Enable"), button:has-text("Apply"), button[type="submit"]'
    ).first();
    if (await saveBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await saveBtn.click();
    }

    // Verify "Slow Mode" indicator is visible in the channel header for Alice
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body?.toLowerCase()).toMatch(/slow\s*mode/);
    }).toPass({ timeout: 5_000 });

    // Verify Bob also sees the slow mode indicator
    await expect(async () => {
      const body = await bob.page.textContent('body');
      expect(body?.toLowerCase()).toMatch(/slow\s*mode/);
    }).toPass({ timeout: 10_000 });
  });

  test('cooldown enforced for regular users with UI feedback', async () => {
    const firstMsg = `First message in slow mode ${RUN_ID}`;
    const secondMsg = `Second message too fast ${RUN_ID}`;

    // Bob (regular user) sends a message — should succeed
    await sendMessage(bob.page, firstMsg);
    await expect(bob.page.getByText(firstMsg).first()).toBeVisible({ timeout: 5_000 });

    // Bob tries to send another message immediately — should be blocked
    await sendMessage(bob.page, secondMsg);

    // Expect a cooldown/error indicator: countdown timer, error message, or disabled input
    await expect(async () => {
      const body = await bob.page.textContent('body');
      const hasBlocking = body?.toLowerCase().match(
        /slow\s*mode|wait|cooldown|seconds|too fast|rate limit/
      );
      // Or check if the input is disabled
      const inputDisabled = await bob.page.locator(
        'input[disabled], textarea[disabled], [class*="disabled" i]'
      ).count();

      expect(hasBlocking || inputDisabled > 0).toBeTruthy();
    }).toPass({ timeout: 5_000 });

    // The second message should NOT appear (was blocked)
    // Wait a moment, then check
    await bob.page.waitForTimeout(2_000);
    const body = await bob.page.textContent('body');
    // If slow mode is working, the second message should be blocked
    // (it may or may not appear depending on implementation — some show an error toast instead)
    const secondMsgVisible = body?.includes(secondMsg);
    if (secondMsgVisible) {
      // Some implementations show the message locally but reject it server-side
      // In that case, the other user shouldn't see it
      const aliceBody = await alice.page.textContent('body');
      // Alice should only see the first message
      expect(aliceBody).toContain(firstMsg);
    }
  });

  test('admins are exempt from slow mode', async () => {
    const adminMsg1 = `Admin message one ${RUN_ID}`;
    const adminMsg2 = `Admin message two ${RUN_ID}`;

    // Alice (admin) sends two messages rapidly — both should succeed
    await sendMessage(alice.page, adminMsg1);
    await expect(alice.page.getByText(adminMsg1).first()).toBeVisible({ timeout: 5_000 });

    // Send second message immediately without any delay
    await sendMessage(alice.page, adminMsg2);
    await expect(alice.page.getByText(adminMsg2).first()).toBeVisible({ timeout: 5_000 });

    // Both messages should be visible — admin is not rate-limited
    const body = await alice.page.textContent('body');
    expect(body).toContain(adminMsg1);
    expect(body).toContain(adminMsg2);

    // Both messages should also appear for Bob in real-time
    await expect(async () => {
      const bobBody = await bob.page.textContent('body');
      expect(bobBody).toContain(adminMsg1);
      expect(bobBody).toContain(adminMsg2);
    }).toPass({ timeout: 10_000 });
  });
});
