import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, createRoom, joinRoom, APP_URL, APP_URL_B } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const ROOM = `General-${RUN_ID}`;
const ROOM2 = `UnreadTest-${RUN_ID}`;

test.describe('Feature 4: Unread Counts', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, `Alice-${RUN_ID}`, APP_URL);
    bob = await createUserContext(browser, `Bob-${RUN_ID}`, APP_URL_B);

    // Create a second room so Bob can be "away" from the main room
    await createRoom(alice.page, ROOM2);

    // Both join both rooms
    await joinRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);
    await joinRoom(bob.page, ROOM2);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('unread count badge appears when messages arrive in another room', async () => {
    // Move Bob to the second room so he is NOT viewing the main room
    await joinRoom(bob.page, ROOM2);

    // Alice sends messages in the main room
    await joinRoom(alice.page, ROOM);
    await sendMessage(alice.page, `Unread msg 1 ${RUN_ID}`);
    await sendMessage(alice.page, `Unread msg 2 ${RUN_ID}`);

    // Bob should see a badge/count on the main room in the sidebar
    await expect(
      bob.page.locator('text=/\\b[1-9]\\d*\\b/').first()
    ).toBeVisible({ timeout: 10_000 });

    // More specific: look for an element containing a count near the room name
    const sidebar = await bob.page.textContent('body');
    const roomEscaped = ROOM.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    expect(sidebar).toMatch(new RegExp(`${roomEscaped}[\\s\\S]{0,50}[1-9]|[1-9][\\s\\S]{0,50}${roomEscaped}`));
  });

  test('badge clears when room is opened', async () => {
    // Bob clicks on the main room to view it
    await joinRoom(bob.page, ROOM);

    // Wait a moment for the unread state to clear
    await bob.page.waitForTimeout(1_000);

    // The unread messages should now be visible
    await expect(bob.page.getByText(`Unread msg 1 ${RUN_ID}`).first()).toBeVisible();
    await expect(bob.page.getByText(`Unread msg 2 ${RUN_ID}`).first()).toBeVisible();

    // Now navigate away and back — no badge should appear since we read them
    await joinRoom(bob.page, ROOM2);
    await bob.page.waitForTimeout(500);

    // Check that there's no unread count for the main room anymore
    const sidebarText = await bob.page.textContent('body');
    const roomEscaped = ROOM.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const generalSection = sidebarText?.match(new RegExp(`${roomEscaped}[\\s\\S]{0,30}`))?.[0] || '';
    expect(generalSection).not.toMatch(/\b[1-9]\d*\b/);
  });

  test('counts are per-user — Alice does not see unread for her own messages', async () => {
    // Alice is in the main room where she sent messages — she should NOT see unread badge
    await joinRoom(alice.page, ROOM);
    await alice.page.waitForTimeout(500);

    // Navigate away
    await joinRoom(alice.page, ROOM2);
    await alice.page.waitForTimeout(500);

    // Main room should not show unread for Alice (she sent those messages)
    const aliceSidebar = await alice.page.textContent('body');
    const roomEscaped = ROOM.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const aliceGeneralSection = aliceSidebar?.match(new RegExp(`${roomEscaped}[\\s\\S]{0,30}`))?.[0] || '';
    expect(aliceGeneralSection).not.toMatch(/\b[1-9]\d*\b/);
  });
});
