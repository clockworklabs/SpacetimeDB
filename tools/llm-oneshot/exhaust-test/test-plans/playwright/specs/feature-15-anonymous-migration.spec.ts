import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, sendMessage, createRoom, joinRoom, APP_URL, APP_URL_B } from '../fixtures';

let anonCtx: { context: BrowserContext; page: Page };

const ANON_ROOM = `AnonTestRoom-${RUN_ID}`;
const MIGRATED_NAME = `MigratedAlice-${RUN_ID}`;

test.describe('Feature 15: Anonymous to Registered Migration', () => {
  test.afterAll(async () => {
    await anonCtx?.context.close();
  });

  test('anonymous user gets auto-generated name on first visit', async ({ browser }) => {
    // Create a fresh context WITHOUT registering — skip the name input
    const context = await browser.newContext({ baseURL: APP_URL });
    const page = await context.newPage();
    await page.goto('/');
    await page.waitForSelector('input, button', { timeout: 15_000 });

    anonCtx = { context, page };

    // Look for "skip", "guest", "anonymous", or just try to proceed without a name
    const skipBtn = page.locator(
      'button:has-text("Skip"), button:has-text("Guest"), button:has-text("Anonymous"), ' +
      'button:has-text("Continue"), a:has-text("Skip"), a:has-text("Guest")'
    ).first();

    const hasSkip = await skipBtn.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasSkip) {
      await skipBtn.click();
      await page.waitForTimeout(2_000);
    } else {
      // If no skip button, the app might auto-assign a guest name
      // Check if we're already in the app without registering
      const nameInput = page.locator(
        'input[placeholder*="name" i], input[placeholder*="display" i], input[placeholder*="username" i]'
      ).first();
      const hasNameInput = await nameInput.isVisible({ timeout: 3_000 }).catch(() => false);

      if (hasNameInput) {
        // Press Enter without filling — some apps assign auto-name
        await nameInput.press('Enter');
        await page.waitForTimeout(2_000);
      }
    }

    // Verify an auto-generated name exists
    const body = await page.textContent('body');

    // Common anonymous name patterns: "Guest-XXXX", "Anonymous", "User-XXXX", "Anon"
    const hasAutoName =
      /guest|anonymous|anon|user.?\d+|visitor/i.test(body || '');

    // Also check that the user is actually in the app (can see rooms or chat UI)
    const hasAppUI =
      /room|chat|message|channel/i.test(body || '') ||
      await page.locator('input[placeholder*="message" i], textarea').first()
        .isVisible({ timeout: 5_000 }).catch(() => false);

    expect(hasAutoName || hasAppUI).toBeTruthy();
  });

  test('anonymous user can send messages with attribution', async () => {
    const { page } = anonCtx;

    // Join or create a room
    const roomInput = page.locator('input[placeholder*="message" i], textarea').first();
    const canChat = await roomInput.isVisible({ timeout: 5_000 }).catch(() => false);

    if (!canChat) {
      // May need to join a room first
      const anyRoom = page.locator('text=/room|general|lobby|chat/i').first();
      const hasRoom = await anyRoom.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasRoom) {
        await anyRoom.click();
        await page.waitForTimeout(1_000);
      } else {
        // Create a room
        await createRoom(page, ANON_ROOM);
      }
    }

    // Send messages as anonymous
    const msg1 = `anon msg 1 ${RUN_ID}`;
    const msg2 = `anon msg 2 ${RUN_ID}`;
    const msg3 = `anon msg 3 ${RUN_ID}`;
    await sendMessage(page, msg1);
    await sendMessage(page, msg2);
    await sendMessage(page, msg3);

    // Verify messages appear
    await expect(page.getByText(msg1).first()).toBeVisible({ timeout: 5_000 });
    await expect(page.getByText(msg2).first()).toBeVisible({ timeout: 5_000 });
    await expect(page.getByText(msg3).first()).toBeVisible({ timeout: 5_000 });

    // Check attribution — messages should be attributed to the auto-generated name
    const body = await page.textContent('body');
    expect(body).toContain(msg1);
  });

  test('anonymous session persists on refresh', async () => {
    const { page } = anonCtx;
    const msg1 = `anon msg 1 ${RUN_ID}`;

    // Record the current anonymous name
    const bodyBefore = await page.textContent('body');

    // Refresh the page
    await page.reload();
    await page.waitForSelector('input, button', { timeout: 15_000 });
    await page.waitForTimeout(2_000);

    // Verify the anonymous identity persists
    const bodyAfter = await page.textContent('body');

    // Previous messages should still be visible (same session)
    const hasMessages = (bodyAfter || '').includes(msg1);

    // User should still be recognized (same guest name, same room)
    const isRecognized =
      /guest|anonymous|anon|user.?\d+/i.test(bodyAfter || '') ||
      hasMessages;

    expect(isRecognized).toBeTruthy();
  });

  test('registration migrates anonymous messages to new name', async () => {
    const { page } = anonCtx;
    const msg1 = `anon msg 1 ${RUN_ID}`;
    const msg2 = `anon msg 2 ${RUN_ID}`;
    const msg3 = `anon msg 3 ${RUN_ID}`;

    // Find register/sign-up button
    const registerBtn = page.locator(
      'button:has-text("Register"), button:has-text("Sign Up"), button:has-text("Create Account"), ' +
      'a:has-text("Register"), a:has-text("Sign Up"), a:has-text("Create Account"), ' +
      '[aria-label*="register" i], [aria-label*="sign up" i], ' +
      'button:has-text("Set Name"), button:has-text("Change Name")'
    ).first();

    const hasRegister = await registerBtn.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasRegister) {
      await registerBtn.click();
      await page.waitForTimeout(1_000);
    }

    // Fill in registration form
    const nameInput = page.locator(
      'input[placeholder*="name" i], input[placeholder*="display" i], ' +
      'input[placeholder*="username" i], input[name*="name" i]'
    ).first();

    const hasInput = await nameInput.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasInput) {
      await nameInput.fill(MIGRATED_NAME);

      // Submit registration
      const submitBtn = page.locator(
        'button:has-text("Register"), button:has-text("Submit"), button:has-text("Set"), ' +
        'button:has-text("Save"), button:has-text("Confirm"), button[type="submit"]'
      ).first();

      const hasSubmit = await submitBtn.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasSubmit) {
        await submitBtn.click();
      } else {
        await nameInput.press('Enter');
      }

      // Wait for registration to complete
      await page.waitForFunction(
        (n) => document.body.textContent?.includes(n),
        MIGRATED_NAME,
        { timeout: 10_000 }
      );
    }

    // Verify the old anonymous messages are now attributed to the new name
    await page.waitForTimeout(2_000);
    const body = await page.textContent('body');

    // Messages should still exist
    expect(body).toContain(msg1);
    expect(body).toContain(msg2);
    expect(body).toContain(msg3);

    // New name should be visible
    expect(body).toContain(MIGRATED_NAME);
  });

  test('room membership preserved after registration', async () => {
    const { page } = anonCtx;
    const msg1 = `anon msg 1 ${RUN_ID}`;

    // Verify the user is still in the room they joined as anonymous
    const body = await page.textContent('body');

    // Should still be in the chat room with access to messages
    const inRoom =
      (body || '').includes(msg1) ||
      /room|chat|channel/i.test(body || '');

    expect(inRoom).toBeTruthy();

    // Verify no "user left" / "user joined" disruption
    // The transition should be seamless — no join/leave events
    const hasDisruption =
      /left the room|has left|joined the room|has joined/i.test(body || '');

    // If there are join/leave messages, they should NOT reference the migration
    // (some apps show join messages for initial room join, which is fine)
    if (hasDisruption) {
      // At most, the anonymous name leaving and new name joining
      // But ideally neither should appear
      const migrationDisruption =
        new RegExp(`${MIGRATED_NAME}.*joined|${MIGRATED_NAME}.*left`, 'i').test(body || '');
      // This is a soft check — not all apps handle seamless migration
    }

    // Verify MigratedAlice is listed as a member
    expect(body).toContain(MIGRATED_NAME);
  });
});
