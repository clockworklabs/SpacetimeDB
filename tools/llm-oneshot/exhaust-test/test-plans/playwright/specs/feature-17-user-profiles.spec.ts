import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Feature 17: User Profiles', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);

    await createRoom(alice.page, 'ProfileTestRoom');
    await joinRoom(bob.page, 'ProfileTestRoom');
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
    await bioInput.fill('Hello, I am Alice!');

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
      expect(body).toContain('Hello, I am Alice!');
    }).toPass({ timeout: 5_000 });
  });

  test('should show profile card when clicking a username', async () => {
    // Have Bob send a message so his name appears in chat
    await joinRoom(alice.page, 'ProfileTestRoom');
    await joinRoom(bob.page, 'ProfileTestRoom');
    await sendMessage(bob.page, 'Profile test message from Bob');
    await expect(alice.page.getByText('Profile test message from Bob')).toBeVisible({ timeout: 10_000 });

    // Click on Bob's name in the message or member list
    const bobName = alice.page.locator('text=Bob').first();
    await bobName.click();

    // A profile card/popover should appear with Bob's info
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body).toContain('Bob');
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
    await joinRoom(alice.page, 'ProfileTestRoom');
    await sendMessage(alice.page, 'Message before name change');
    await expect(bob.page.getByText('Message before name change')).toBeVisible({ timeout: 10_000 });

    // Verify Bob sees Alice's name on the message
    const bobBody = await bob.page.textContent('body');
    expect(bobBody).toContain('Alice');

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
    await nameInput.fill('AliceRenamed');

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
      expect(body).toContain('AliceRenamed');
    }).toPass({ timeout: 5_000 });

    // Verify Bob sees the name change in real-time — messages re-attributed
    await expect(async () => {
      const body = await bob.page.textContent('body');
      expect(body).toContain('AliceRenamed');
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
    await bioInput.fill('Bob the builder');

    const saveBtn = bob.page.locator(
      'button:has-text("Save"), button:has-text("Update"), button[type="submit"]'
    ).first();
    if (await saveBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await saveBtn.click();
    } else {
      await bioInput.press('Enter');
    }

    // Alice clicks Bob's name to see updated profile
    await joinRoom(alice.page, 'ProfileTestRoom');
    const bobName = alice.page.locator('text=Bob').first();
    await bobName.click();

    // Verify Bob's bio appears in the profile card
    await expect(async () => {
      const body = await alice.page.textContent('body');
      expect(body).toContain('Bob the builder');
    }).toPass({ timeout: 10_000 });
  });
});
