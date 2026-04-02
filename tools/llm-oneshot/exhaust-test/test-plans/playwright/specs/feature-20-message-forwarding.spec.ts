import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, createRoom, joinRoom, APP_URL, APP_URL_B } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const SOURCE_ROOM = `ForwardSource-${RUN_ID}`;
const TARGET_ROOM = `ForwardTarget-${RUN_ID}`;
const ALICE = `Alice-${RUN_ID}`;
const BOB = `Bob-${RUN_ID}`;
const FORWARD_MSG = `Message to be forwarded ${RUN_ID}`;

test.describe('Feature 20: Message Forwarding', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, ALICE, APP_URL);
    bob = await createUserContext(browser, BOB, APP_URL);

    // Create source and target rooms
    await createRoom(alice.page, SOURCE_ROOM);
    await createRoom(alice.page, TARGET_ROOM);
    await joinRoom(bob.page, SOURCE_ROOM);
    await joinRoom(bob.page, TARGET_ROOM);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('forward button opens channel picker and sends', async () => {
    // Join source room and send a message
    await joinRoom(alice.page, SOURCE_ROOM);
    await sendMessage(alice.page, FORWARD_MSG);
    await expect(alice.page.getByText(FORWARD_MSG).first()).toBeVisible();

    // Hover over the message to reveal action buttons
    const message = alice.page.locator(`text=${FORWARD_MSG}`).first();
    await message.hover();

    // Find and click the forward button
    const forwardBtn = alice.page.locator(
      'button:has-text("Forward"), [aria-label*="forward" i], ' +
      '[title*="forward" i], button:has(svg[class*="forward" i]), ' +
      'button:has-text("Share")'
    ).first();
    await forwardBtn.click({ timeout: 5_000 });

    // Channel picker should appear — select the target channel
    const channelOption = alice.page.locator(`text=${TARGET_ROOM}`).first();
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
    await joinRoom(alice.page, TARGET_ROOM);

    // The forwarded message should appear with "Forwarded from" attribution
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body).toContain(FORWARD_MSG);
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
    await joinRoom(bob.page, TARGET_ROOM);
    await expect(async () => {
      const body = await bob.page.textContent('body');
      expect(body).toContain(FORWARD_MSG);
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
    await joinRoom(alice.page, SOURCE_ROOM);

    // The original message should still be there, unchanged
    await expect(alice.page.getByText(FORWARD_MSG).first()).toBeVisible({ timeout: 5_000 });

    // The original should NOT have any forwarding attribution
    const body = await alice.page.textContent('body');
    // We check the source room doesn't have "Forwarded from" on the original message
    // Note: we need to be careful here — the original message shouldn't show forwarding info
    expect(body).toContain(FORWARD_MSG);

    // Verify the original message in source room doesn't say "Forwarded"
    // by checking that the forwarded-from indicator is absent in the source room context
    const forwardIndicators = alice.page.locator(
      '[class*="forwarded" i], :text("Forwarded from"), :text("Shared from")'
    );
    const count = await forwardIndicators.count();
    expect(count).toBe(0);
  });
});
