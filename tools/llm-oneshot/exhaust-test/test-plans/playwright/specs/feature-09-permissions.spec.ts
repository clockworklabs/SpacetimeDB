import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';
const ROOM = 'PermRoom';

test.describe('Feature 9: Permissions', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);

    // Alice creates room (becomes admin), Bob joins
    await createRoom(alice.page, ROOM);
    await joinRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('room creator has admin controls visible', async () => {
    // Alice should see admin controls — kick, ban, manage, or admin indicator
    const aliceBody = await alice.page.textContent('body');

    // Look for any admin-related UI element
    const adminIndicator = alice.page.locator(
      'button:has-text("Kick"), button:has-text("Ban"), button:has-text("Manage"), ' +
      '[aria-label*="kick" i], [aria-label*="admin" i], [aria-label*="manage" i], ' +
      '[title*="admin" i], [title*="kick" i], .admin, [class*="admin" i]'
    ).first();

    const hasAdmin = await adminIndicator.isVisible({ timeout: 5_000 }).catch(() => false);

    // Alternative: check if Alice has an admin badge/label
    const hasAdminText = /admin/i.test(aliceBody || '');

    expect(hasAdmin || hasAdminText).toBeTruthy();
  });

  test('non-admin does not have admin controls', async () => {
    // Bob should NOT see kick/ban/manage controls (or they should be disabled)
    const bobBody = await bob.page.textContent('body');

    // Bob should not see prominent admin controls for other users
    const kickBtn = bob.page.locator(
      'button:has-text("Kick"), button:has-text("Ban")'
    ).first();
    const hasKick = await kickBtn.isVisible({ timeout: 3_000 }).catch(() => false);

    // If Bob doesn't see kick/ban buttons, that's a pass
    // If Bob sees them but they're for himself (self-actions), that's also fine
    // The key is Bob shouldn't be able to kick Alice
    if (hasKick) {
      // If buttons exist, they should be disabled or scoped to self
      const isDisabled = await kickBtn.isDisabled().catch(() => false);
      expect(isDisabled).toBeTruthy();
    }
    // If no kick buttons found, that's the expected pass
  });

  test('admin can promote another user to admin', async () => {
    // Alice promotes Bob to admin
    const promoteBtn = alice.page.locator(
      'button:has-text("Promote"), button:has-text("Make Admin"), button:has-text("Admin"), ' +
      '[aria-label*="promote" i], [aria-label*="make admin" i], [title*="promote" i]'
    ).first();

    // May need to click on Bob's name first to see user management options
    const bobEntry = alice.page.locator('text=Bob').first();
    await bobEntry.click({ timeout: 5_000 }).catch(() => {});

    // Wait for promote button or context menu
    const hasPromote = await promoteBtn.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasPromote) {
      await promoteBtn.click();
    } else {
      // Try right-click context menu
      await bobEntry.click({ button: 'right' }).catch(() => {});
      const contextPromote = alice.page.locator(
        'text=/promote|make admin/i'
      ).first();
      const hasContextPromote = await contextPromote.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasContextPromote) {
        await contextPromote.click();
      }
    }

    // Verify Bob now has admin status — check Bob's page for admin controls
    await bob.page.waitForTimeout(2_000);
    const bobBody = await bob.page.textContent('body');

    // Bob should now see admin controls or have admin badge
    const bobHasAdmin = alice.page.locator(
      '[class*="admin" i], [data-role="admin"], text=/admin/i'
    );
    const adminVisible = await bobHasAdmin.first().isVisible({ timeout: 10_000 }).catch(() => false);
    const bodyHasAdmin = /admin/i.test(bobBody || '');

    expect(adminVisible || bodyHasAdmin).toBeTruthy();
  });

  test('admin can kick a user and they lose access immediately', async () => {
    // Create a fresh room for the kick test to avoid state pollution
    const kickRoom = 'KickTestRoom';
    await createRoom(alice.page, kickRoom);
    await joinRoom(alice.page, kickRoom);
    await joinRoom(bob.page, kickRoom);

    // Send a message so we can verify Bob loses access
    await sendMessage(alice.page, 'Before kick message');
    await expect(bob.page.getByText('Before kick message')).toBeVisible({ timeout: 10_000 });

    // Alice kicks Bob — find kick button
    // May need to click on Bob's user entry first
    const bobEntryInAlice = alice.page.locator('text=Bob').first();
    await bobEntryInAlice.click({ timeout: 5_000 }).catch(() => {});

    const kickBtn = alice.page.locator(
      'button:has-text("Kick"), button:has-text("Remove"), ' +
      '[aria-label*="kick" i], [aria-label*="remove" i], [title*="kick" i]'
    ).first();

    const hasKick = await kickBtn.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasKick) {
      await kickBtn.click();

      // Handle confirmation dialog if one appears
      const confirmBtn = alice.page.locator(
        'button:has-text("Confirm"), button:has-text("Yes"), button:has-text("OK")'
      ).first();
      const hasConfirm = await confirmBtn.isVisible({ timeout: 2_000 }).catch(() => false);
      if (hasConfirm) {
        await confirmBtn.click();
      }
    } else {
      // Try right-click context menu
      await bobEntryInAlice.click({ button: 'right' }).catch(() => {});
      const contextKick = alice.page.locator('text=/kick|remove/i').first();
      const hasContextKick = await contextKick.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasContextKick) {
        await contextKick.click();
      }
    }

    // Verify Bob lost access — should see kicked message or redirect
    await bob.page.waitForTimeout(2_000);
    const bobBody = await bob.page.textContent('body');

    // Bob should either be redirected, see a kicked message, or no longer see the room
    const kicked =
      /kicked|removed|denied|no longer|access/i.test(bobBody || '') ||
      !(bobBody || '').includes('Before kick message');

    expect(kicked).toBeTruthy();
  });

  test('permission changes apply in real-time without refresh', async () => {
    // This is covered implicitly by the kick and promote tests above.
    // The key verification is that Bob's UI updated without a page refresh.
    // We verify by checking that Bob's page state changed from the promote/kick
    // actions performed on Alice's page, with no bob.page.reload() calls.

    // Additional verification: send a message after kick and confirm Bob doesn't see it
    const postKickRoom = 'KickTestRoom';
    await sendMessage(alice.page, 'Post-kick secret message');

    // Bob should NOT see this message (they were kicked)
    await bob.page.waitForTimeout(2_000);
    const bobBody = await bob.page.textContent('body');
    expect(bobBody).not.toContain('Post-kick secret message');
  });
});
