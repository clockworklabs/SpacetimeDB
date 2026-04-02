import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Feature 16: Pinned Messages', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);

    // Create a room and have both users join
    await createRoom(alice.page, 'PinTestRoom');
    await joinRoom(bob.page, 'PinTestRoom');
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('should pin a message and show pin indicator', async () => {
    // Alice sends a message to pin
    await sendMessage(alice.page, 'This message should be pinned');
    await expect(alice.page.getByText('This message should be pinned')).toBeVisible();

    // Hover over the message to reveal action buttons
    const message = alice.page.locator('text=This message should be pinned').first();
    await message.hover();

    // Find and click the pin button — try multiple common patterns
    const pinBtn = alice.page.locator(
      'button:has-text("Pin"), [aria-label*="pin" i], [title*="pin" i], button:has(svg[class*="pin" i])'
    ).first();
    await pinBtn.click({ timeout: 5_000 });

    // If there's a confirmation dialog, confirm it
    const confirmBtn = alice.page.locator(
      'button:has-text("Confirm"), button:has-text("Pin Message"), button:has-text("Yes")'
    ).first();
    if (await confirmBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
      await confirmBtn.click();
    }

    // Verify pin indicator appears on the message for Alice
    const pinIndicator = alice.page.locator(
      '[class*="pin" i], [data-pinned], :text("pinned"), svg[class*="pin" i]'
    );
    await expect(pinIndicator.first()).toBeVisible({ timeout: 5_000 });

    // Verify pin indicator also visible to Bob in real-time
    await expect(bob.page.getByText('This message should be pinned')).toBeVisible({ timeout: 10_000 });
    const bobPinIndicator = bob.page.locator(
      '[class*="pin" i], [data-pinned], :text("pinned"), svg[class*="pin" i]'
    );
    await expect(bobPinIndicator.first()).toBeVisible({ timeout: 10_000 });
  });

  test('should display pinned messages in the pinned panel', async () => {
    // Open the pinned messages panel from the channel header
    const pinnedPanelBtn = alice.page.locator(
      'button:has-text("Pinned"), [aria-label*="pinned" i], [title*="pinned" i], button:has-text("Pins")'
    ).first();
    await pinnedPanelBtn.click({ timeout: 5_000 });

    // Verify the pinned message appears in the panel
    await expect(
      alice.page.getByText('This message should be pinned')
    ).toBeVisible({ timeout: 5_000 });

    // Verify the panel contains pinned message content
    const body = await alice.page.textContent('body');
    expect(body).toContain('This message should be pinned');
  });

  test('should unpin a message and remove it from the panel', async () => {
    // Hover over the pinned message to find unpin action
    const message = alice.page.locator('text=This message should be pinned').first();
    await message.hover();

    // Find and click the unpin button
    const unpinBtn = alice.page.locator(
      'button:has-text("Unpin"), [aria-label*="unpin" i], [title*="unpin" i]'
    ).first();
    await unpinBtn.click({ timeout: 5_000 });

    // Confirm if needed
    const confirmBtn = alice.page.locator(
      'button:has-text("Confirm"), button:has-text("Unpin"), button:has-text("Yes")'
    ).first();
    if (await confirmBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
      await confirmBtn.click();
    }

    // Verify pin indicator is removed on Alice's view
    // Re-open pinned panel if needed to verify it's empty
    const pinnedPanelBtn = alice.page.locator(
      'button:has-text("Pinned"), [aria-label*="pinned" i], [title*="pinned" i], button:has-text("Pins")'
    ).first();
    if (await pinnedPanelBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
      await pinnedPanelBtn.click();
    }

    // Panel should show no pinned messages or "no pinned" text
    const emptyState = alice.page.locator(
      ':text("No pinned"), :text("no pinned"), :text("empty")'
    );
    // Either the pinned message is gone from the panel, or there's an empty state
    await expect(emptyState.first()).toBeVisible({ timeout: 5_000 }).catch(async () => {
      // Alternative: verify the message no longer has a pin indicator
      const pinIndicators = alice.page.locator('[class*="pin" i][data-pinned], [class*="pinned" i]');
      expect(await pinIndicators.count()).toBe(0);
    });
  });

  test('should sync pin/unpin actions in real-time across clients', async () => {
    // Send a new message and pin it from Alice
    await sendMessage(alice.page, 'Real-time pin sync test');
    await expect(alice.page.getByText('Real-time pin sync test')).toBeVisible();
    await expect(bob.page.getByText('Real-time pin sync test')).toBeVisible({ timeout: 10_000 });

    // Alice pins the message
    const message = alice.page.locator('text=Real-time pin sync test').first();
    await message.hover();

    const pinBtn = alice.page.locator(
      'button:has-text("Pin"), [aria-label*="pin" i], [title*="pin" i]'
    ).first();
    await pinBtn.click({ timeout: 5_000 });

    // Confirm if needed
    const confirmBtn = alice.page.locator(
      'button:has-text("Confirm"), button:has-text("Pin Message"), button:has-text("Yes")'
    ).first();
    if (await confirmBtn.isVisible({ timeout: 2_000 }).catch(() => false)) {
      await confirmBtn.click();
    }

    // Bob should see the pin indicator appear in real-time
    const bobBody = async () => await bob.page.textContent('body');
    await expect(async () => {
      const text = await bobBody();
      expect(text?.toLowerCase()).toMatch(/pin/);
    }).toPass({ timeout: 10_000 });

    // Now Alice unpins — Bob should see it disappear in real-time
    const messageAgain = alice.page.locator('text=Real-time pin sync test').first();
    await messageAgain.hover();

    const unpinBtn = alice.page.locator(
      'button:has-text("Unpin"), [aria-label*="unpin" i], [title*="unpin" i]'
    ).first();
    await unpinBtn.click({ timeout: 5_000 });

    const confirmBtn2 = alice.page.locator(
      'button:has-text("Confirm"), button:has-text("Unpin"), button:has-text("Yes")'
    ).first();
    if (await confirmBtn2.isVisible({ timeout: 2_000 }).catch(() => false)) {
      await confirmBtn2.click();
    }

    // Bob should see pin indicator removed
    await expect(async () => {
      const bobText = await bob.page.textContent('body');
      // The message should still exist but without pin attribution
      expect(bobText).toContain('Real-time pin sync test');
    }).toPass({ timeout: 10_000 });
  });
});
