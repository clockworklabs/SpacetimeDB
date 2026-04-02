import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, createRoom, joinRoom, triggerTyping } from '../fixtures';

let alice: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';
const ROOM_A = 'DraftRoomA';
const ROOM_B = 'DraftRoomB';

/** Helper to get the current value of the message input */
async function getInputValue(page: Page): Promise<string> {
  return page.evaluate(() => {
    const input = document.querySelector<HTMLInputElement | HTMLTextAreaElement>(
      'input[placeholder*="message" i], input[placeholder*="type" i], textarea'
    );
    return input?.value || '';
  });
}

/** Helper to get the message input locator */
function getMessageInput(page: Page) {
  return page.locator(
    'input[placeholder*="message" i], input[placeholder*="type" i], textarea'
  ).first();
}

test.describe('Feature 14: Draft Sync', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);

    await createRoom(alice.page, ROOM_A);
    await createRoom(alice.page, ROOM_B);
  });

  test.afterAll(async () => {
    await alice?.context.close();
  });

  test('draft is preserved when switching rooms', async () => {
    // Go to Room A and type a draft
    await joinRoom(alice.page, ROOM_A);
    const input = getMessageInput(alice.page);
    await input.fill('This is a draft...');

    // Verify the text is in the input
    const beforeSwitch = await getInputValue(alice.page);
    expect(beforeSwitch).toBe('This is a draft...');

    // Switch to Room B
    await joinRoom(alice.page, ROOM_B);
    await alice.page.waitForTimeout(1_000);

    // Input should be empty (different room)
    const roomBValue = await getInputValue(alice.page);
    expect(roomBValue).not.toContain('This is a draft...');

    // Switch back to Room A
    await joinRoom(alice.page, ROOM_A);
    await alice.page.waitForTimeout(1_000);

    // Draft should be restored
    const restored = await getInputValue(alice.page);
    expect(restored).toBe('This is a draft...');
  });

  test('different rooms maintain separate drafts', async () => {
    // Room A should still have our draft from previous test
    await joinRoom(alice.page, ROOM_A);
    await alice.page.waitForTimeout(500);
    const roomADraft = await getInputValue(alice.page);

    // Go to Room B and type a different draft
    await joinRoom(alice.page, ROOM_B);
    const input = getMessageInput(alice.page);
    await input.fill('Room B draft content');

    const roomBDraft = await getInputValue(alice.page);
    expect(roomBDraft).toBe('Room B draft content');

    // Switch back to Room A — should have its own draft
    await joinRoom(alice.page, ROOM_A);
    await alice.page.waitForTimeout(500);
    const roomARestored = await getInputValue(alice.page);
    expect(roomARestored).toContain('draft');

    // Switch to Room B — should have its draft
    await joinRoom(alice.page, ROOM_B);
    await alice.page.waitForTimeout(500);
    const roomBRestored = await getInputValue(alice.page);
    expect(roomBRestored).toBe('Room B draft content');
  });

  test('draft persists after page refresh (cross-session)', async () => {
    // Ensure Room A has a draft
    await joinRoom(alice.page, ROOM_A);
    const input = getMessageInput(alice.page);
    await input.fill('Persistent draft text');

    // Refresh the page
    await alice.page.reload();
    await alice.page.waitForSelector('input, button', { timeout: 15_000 });

    // Navigate back to Room A
    await joinRoom(alice.page, ROOM_A);
    await alice.page.waitForTimeout(2_000);

    // Draft should persist
    const afterRefresh = await getInputValue(alice.page);
    expect(afterRefresh).toBe('Persistent draft text');
  });

  test('draft clears after sending the message', async () => {
    // Go to Room A which has a draft
    await joinRoom(alice.page, ROOM_A);
    await alice.page.waitForTimeout(500);

    // Get current draft value
    let currentValue = await getInputValue(alice.page);
    if (!currentValue) {
      // Set a new draft if empty
      const input = getMessageInput(alice.page);
      await input.fill('Draft to be sent');
      currentValue = 'Draft to be sent';
    }

    // Send the message
    const input = getMessageInput(alice.page);
    await input.press('Enter');
    await alice.page.waitForTimeout(500);

    // Input should be cleared
    const afterSend = await getInputValue(alice.page);
    expect(afterSend).toBe('');

    // Switch away and back — no draft should remain
    await joinRoom(alice.page, ROOM_B);
    await alice.page.waitForTimeout(500);
    await joinRoom(alice.page, ROOM_A);
    await alice.page.waitForTimeout(500);

    const afterReturn = await getInputValue(alice.page);
    expect(afterReturn).toBe('');
  });

  test('draft syncs across sessions in real-time', async ({ browser }) => {
    // Open a second context for the same user
    const alice2 = await createUserContext(browser, 'Alice', APP_URL);

    try {
      // Set a draft in Alice's original tab
      await joinRoom(alice.page, ROOM_B);
      const input = getMessageInput(alice.page);
      await input.fill('Cross-session draft');

      // Alice2 navigates to the same room
      await joinRoom(alice2.page, ROOM_B);
      await alice2.page.waitForTimeout(3_000);

      // Check if draft synced to the second session
      const alice2Value = await getInputValue(alice2.page);
      expect(alice2Value).toBe('Cross-session draft');
    } finally {
      await alice2.context.close();
    }
  });
});
