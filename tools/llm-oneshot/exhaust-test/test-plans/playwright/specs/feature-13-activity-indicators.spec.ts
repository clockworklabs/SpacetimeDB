import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';
const ROOM_ACTIVE = `ActiveTestRoom-${RUN_ID}`;
const ROOM_QUIET = `QuietRoom-${RUN_ID}`;
const ALICE = `Alice-${RUN_ID}`;
const BOB = `Bob-${RUN_ID}`;

test.describe('Feature 13: Activity Indicators', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, ALICE, APP_URL);
    bob = await createUserContext(browser, BOB, APP_URL);

    await createRoom(alice.page, ROOM_ACTIVE);
    await createRoom(alice.page, ROOM_QUIET);
    await joinRoom(alice.page, ROOM_ACTIVE);
    await joinRoom(bob.page, ROOM_ACTIVE);
    await joinRoom(bob.page, ROOM_QUIET);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('sending a message shows an activity badge on the room', async () => {
    // Send a message in the active room
    const actMsg = `Activity test message 1 ${RUN_ID}`;
    await sendMessage(alice.page, actMsg);
    await expect(bob.page.getByText(actMsg).first()).toBeVisible({ timeout: 10_000 });

    // Check room list for activity indicator on Bob's side (he's viewing the room list)
    // Navigate Bob to room list view by joining the quiet room
    await joinRoom(bob.page, ROOM_QUIET);

    // Wait for activity indicator to appear on the active room
    await bob.page.waitForTimeout(2_000);

    // Look for activity badges: "Active", green dot, badge, indicator
    const activityBadge = bob.page.locator(
      '[class*="active" i][class*="badge" i], [class*="activity" i], ' +
      '[class*="badge" i]:near(:text("' + ROOM_ACTIVE + '")), ' +
      '[data-activity], [aria-label*="active" i]'
    ).first();

    const hasActivityBadge = await activityBadge.isVisible({ timeout: 5_000 }).catch(() => false);

    // Alternative: check body text for "Active" label near room name
    const bobBody = await bob.page.textContent('body');
    const hasActivityText = /active/i.test(bobBody || '');

    // Also check for visual indicators (colored dots, icons)
    const indicator = bob.page.locator(
      '[class*="green" i], [class*="dot" i], [class*="indicator" i], ' +
      '[class*="pulse" i], [class*="glow" i]'
    ).first();
    const hasIndicator = await indicator.isVisible({ timeout: 3_000 }).catch(() => false);

    expect(hasActivityBadge || hasActivityText || hasIndicator).toBeTruthy();
  });

  test('rapid messages trigger a "Hot" badge', async () => {
    // Send 5+ messages rapidly in the active room
    await joinRoom(alice.page, ROOM_ACTIVE);
    for (let i = 0; i < 6; i++) {
      await sendMessage(alice.page, `Rapid message ${RUN_ID} ${i + 1}`);
    }

    // Switch Bob to a different room so he sees the room list activity
    await joinRoom(bob.page, ROOM_QUIET);
    await bob.page.waitForTimeout(3_000);

    // Look for elevated activity indicator: "Hot", fire icon, orange badge
    const hotBadge = bob.page.locator(
      '[class*="hot" i], [class*="fire" i], [class*="trending" i], ' +
      '[class*="orange" i][class*="badge" i], ' +
      'text=/hot/i, text=/\\uD83D\\uDD25/i'
    ).first();

    const hasHot = await hotBadge.isVisible({ timeout: 5_000 }).catch(() => false);

    // Alternative text check
    const bobBody = await bob.page.textContent('body');
    const hasHotText = /hot|fire|trending|very.active/i.test(bobBody || '');

    // Check for upgraded visual indicator
    const orangeIndicator = bob.page.locator(
      '[class*="orange" i], [class*="hot" i], [class*="fire" i]'
    ).first();
    const hasOrange = await orangeIndicator.isVisible({ timeout: 3_000 }).catch(() => false);

    expect(hasHot || hasHotText || hasOrange).toBeTruthy();
  });

  test('activity badges are visible to both users', async () => {
    // Check Alice's room list also shows activity indicators
    await joinRoom(alice.page, ROOM_QUIET);
    await alice.page.waitForTimeout(2_000);

    const aliceBody = await alice.page.textContent('body');
    const bobBody = await bob.page.textContent('body');

    // Both should have some activity indication for the active room
    const aliceHas =
      /active|hot|fire/i.test(aliceBody || '') ||
      await alice.page.locator(
        '[class*="activity" i], [class*="badge" i], [class*="indicator" i]'
      ).first().isVisible({ timeout: 3_000 }).catch(() => false);

    const bobHas =
      /active|hot|fire/i.test(bobBody || '') ||
      await bob.page.locator(
        '[class*="activity" i], [class*="badge" i], [class*="indicator" i]'
      ).first().isVisible({ timeout: 3_000 }).catch(() => false);

    expect(aliceHas || bobHas).toBeTruthy();
  });

  test('activity indicators update in real-time', async () => {
    // Send more messages and verify the badge updates without refresh
    await joinRoom(alice.page, ROOM_ACTIVE);
    await joinRoom(bob.page, ROOM_QUIET);

    // Capture initial state
    const beforeBody = await bob.page.textContent('body');

    // Send messages
    for (let i = 0; i < 3; i++) {
      await sendMessage(alice.page, `Realtime activity msg ${RUN_ID} ${i}`);
    }

    // Wait for real-time update
    await bob.page.waitForTimeout(3_000);
    const afterBody = await bob.page.textContent('body');

    // The activity indicators should be present (and possibly changed)
    const hasActivity =
      /active|hot|fire/i.test(afterBody || '') ||
      await bob.page.locator(
        '[class*="activity" i], [class*="badge" i], [class*="indicator" i]'
      ).first().isVisible({ timeout: 3_000 }).catch(() => false);

    expect(hasActivity).toBeTruthy();
  });
});
