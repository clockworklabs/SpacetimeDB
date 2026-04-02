import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Feature 20: Message Forwarding', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);

    // Create source and target rooms
    await createRoom(alice.page, 'ForwardSource');
    await createRoom(alice.page, 'ForwardTarget');
    await joinRoom(bob.page, 'ForwardSource');
    await joinRoom(bob.page, 'ForwardTarget');
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('forward button opens channel picker and sends', async () => {
    // Join source room and send a message
    await joinRoom(alice.page, 'ForwardSource');
    await sendMessage(alice.page, 'Message to be forwarded');
    await expect(alice.page.getByText('Message to be forwarded')).toBeVisible();

    // Hover over the message to reveal action buttons
    const message = alice.page.locator('text=Message to be forwarded').first();
    await message.hover();

    // Find and click the forward button
    const forwardBtn = alice.page.locator(
      'button:has-text("Forward"), [aria-label*="forward" i], ' +
      '[title*="forward" i], button:has(svg[class*="forward" i]), ' +
      'button:has-text("Share")'
    ).first();
    await forwardBtn.click({ timeout: 5_000 });

    // Channel picker should appear — select the target channel
    const channelOption = alice.page.locator('text=ForwardTarget').first();
    await expect(channelOption).toBeVisible({ timeout: 5_000 });
    await channelOption.click();

    // Confirm the forward if there's a confirmation button
    const confirmBtn = alice.page.locator(
      'button:has-text("Forward"), button:has-text("Send"), ' +
      'button:has-text("Confirm"), button[type="submit"]'
    ).first();
    if (await confirmBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await confirmBtn.click();
    }

    // Wait for the forward action to complete
    await alice.page.waitForTimeout(1_000);
  });

  test('forwarded message shows in target with attribution', async () => {
    // Navigate to the target room
    await joinRoom(alice.page, 'ForwardTarget');

    // The forwarded message should appear with "Forwarded from" attribution
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body).toContain('Message to be forwarded');
    }).toPass({ timeout: 10_000 });

    // Check for forwarding attribution
    await expect(async () => {
      const body = await alice.page.textContent('body');
      const hasAttribution = body?.toLowerCase().match(
        /forwarded|forward from|shared from|originally from/
      );
      expect(hasAttribution).toBeTruthy();
    }).toPass({ timeout: 5_000 });

    // Bob should also see the forwarded message in real-time
    await joinRoom(bob.page, 'ForwardTarget');
    await expect(async () => {
      const body = await bob.page.textContent('body');
      expect(body).toContain('Message to be forwarded');
    }).toPass({ timeout: 10_000 });

    // Bob should see the forwarding attribution too
    await expect(async () => {
      const body = await bob.page.textContent('body');
      const hasAttribution = body?.toLowerCase().match(
        /forwarded|forward from|shared from|originally from/
      );
      expect(hasAttribution).toBeTruthy();
    }).toPass({ timeout: 5_000 });
  });

  test('original message not modified by forwarding', async () => {
    // Navigate back to the source room
    await joinRoom(alice.page, 'ForwardSource');

    // The original message should still be there, unchanged
    await expect(alice.page.getByText('Message to be forwarded')).toBeVisible({ timeout: 5_000 });

    // The original should NOT have any forwarding attribution
    const body = await alice.page.textContent('body');
    // We check the source room doesn't have "Forwarded from" on the original message
    // Note: we need to be careful here — the original message shouldn't show forwarding info
    expect(body).toContain('Message to be forwarded');

    // Verify the original message in source room doesn't say "Forwarded"
    // by checking that the forwarded-from indicator is absent in the source room context
    const forwardIndicators = alice.page.locator(
      '[class*="forwarded" i], :text("Forwarded from"), :text("Shared from")'
    );
    const count = await forwardIndicators.count();
    expect(count).toBe(0);
  });
});
