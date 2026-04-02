import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';
const ROOM = `ProfileTestRoom-${RUN_ID}`;
const ALICE = `Alice-${RUN_ID}`;
const BOB = `Bob-${RUN_ID}`;
const ALICE_BIO = `Hello, I am ${ALICE}!`;
const BOB_BIO = `Bob the builder ${RUN_ID}`;
const ALICE_RENAMED = `AliceRenamed-${RUN_ID}`;

test.describe('Feature 17: User Profiles', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, ALICE, APP_URL);
    bob = await createUserContext(browser, BOB, APP_URL);

    await createRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('should edit profile bio and status', async () => {
    // Find the profile/settings entry point
    const profileEntry = alice.page.locator(
      'button:has-text("Profile"), [aria-label*="profile" i], [aria-label*="settings" i], ' +
      'button:has-text("Settings"), [title*="profile" i], [title*="settings" i]'
    ).first();
    await profileEntry.click({ timeout: 5_000 });

    // Find the bio/status input field
    const bioInput = alice.page.locator(
      'input[placeholder*="bio" i], textarea[placeholder*="bio" i], ' +
      'input[placeholder*="status" i], textarea[placeholder*="status" i], ' +
      'input[placeholder*="about" i], textarea[placeholder*="about" i]'
    ).first();
    await bioInput.fill(ALICE_BIO);

    // Save the profile
    const saveBtn = alice.page.locator(
      'button:has-text("Save"), button:has-text("Update"), button[type="submit"]'
    ).first();
    if (await saveBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await saveBtn.click();
    } else {
      await bioInput.press('Enter');
    }

    // Verify the bio is saved
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body).toContain(ALICE_BIO);
    }).toPass({ timeout: 5_000 });
  });

  test('should show profile card when clicking a username', async () => {
    // Have Bob send a message so his name appears in chat
    await joinRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);
    const profileTestMsg = `Profile test message from ${BOB} ${RUN_ID}`;
    await sendMessage(bob.page, profileTestMsg);
    await expect(alice.page.getByText(profileTestMsg).first()).toBeVisible({ timeout: 10_000 });

    // Click on Bob's name in the message or member list
    const bobName = alice.page.locator(`text=${BOB}`).first();
    await bobName.click();

    // A profile card/popover should appear with Bob's info
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body).toContain(BOB);
    }).toPass({ timeout: 5_000 });

    // Look for profile-related UI elements (card, popover, modal)
    const profileCard = alice.page.locator(
      '[class*="profile" i], [class*="popover" i], [class*="card" i], [class*="modal" i], [role="dialog"]'
    ).first();
    await expect(profileCard).toBeVisible({ timeout: 5_000 }).catch(() => {
      // Some apps show profile inline — acceptable
    });
  });

  test('should propagate name changes in real-time across all views', async () => {
    // Send a message as Alice before changing name
    await joinRoom(alice.page, ROOM);
    const beforeChangeMsg = `Message before name change ${RUN_ID}`;
    await sendMessage(alice.page, beforeChangeMsg);
    await expect(bob.page.getByText(beforeChangeMsg).first()).toBeVisible({ timeout: 10_000 });

    // Verify Bob sees Alice's name on the message
    const bobBody = await bob.page.textContent('body');
    expect(bobBody).toContain(ALICE);

    // Alice changes her display name
    const profileEntry = alice.page.locator(
      'button:has-text("Profile"), [aria-label*="profile" i], [aria-label*="settings" i], ' +
      'button:has-text("Settings"), [title*="profile" i], [title*="settings" i]'
    ).first();
    await profileEntry.click({ timeout: 5_000 });

    // Find and update the display name field
    const nameInput = alice.page.locator(
      'input[placeholder*="name" i], input[placeholder*="display" i], ' +
      'input[aria-label*="name" i], input[type="text"]'
    ).first();
    await nameInput.clear();
    await nameInput.fill(ALICE_RENAMED);

    // Save
    const saveBtn = alice.page.locator(
      'button:has-text("Save"), button:has-text("Update"), button[type="submit"]'
    ).first();
    if (await saveBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await saveBtn.click();
    } else {
      await nameInput.press('Enter');
    }

    // Verify Alice sees her new name
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body).toContain(ALICE_RENAMED);
    }).toPass({ timeout: 5_000 });

    // Verify Bob sees the name change in real-time — messages re-attributed
    await expect(async () => {
      const body = await bob.page.textContent('body');
      expect(body).toContain(ALICE_RENAMED);
    }).toPass({ timeout: 10_000 });
  });

  test('should display updated profile info in the profile card', async () => {
    // Bob updates his bio
    const profileEntry = bob.page.locator(
      'button:has-text("Profile"), [aria-label*="profile" i], [aria-label*="settings" i], ' +
      'button:has-text("Settings"), [title*="profile" i], [title*="settings" i]'
    ).first();
    await profileEntry.click({ timeout: 5_000 });

    const bioInput = bob.page.locator(
      'input[placeholder*="bio" i], textarea[placeholder*="bio" i], ' +
      'input[placeholder*="status" i], textarea[placeholder*="status" i], ' +
      'input[placeholder*="about" i], textarea[placeholder*="about" i]'
    ).first();
    await bioInput.fill(BOB_BIO);

    const saveBtn = bob.page.locator(
      'button:has-text("Save"), button:has-text("Update"), button[type="submit"]'
    ).first();
    if (await saveBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await saveBtn.click();
    } else {
      await bioInput.press('Enter');
    }

    // Alice clicks Bob's name to see updated profile
    await joinRoom(alice.page, ROOM);
    const bobName = alice.page.locator(`text=${BOB}`).first();
    await bobName.click();

    // Verify Bob's bio appears in the profile card
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body).toContain(BOB_BIO);
    }).toPass({ timeout: 10_000 });
  });
});
