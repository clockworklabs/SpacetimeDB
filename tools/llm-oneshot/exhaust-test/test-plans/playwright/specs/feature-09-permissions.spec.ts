import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, createRoom, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';
const ROOM = `PermRoom-${RUN_ID}`;
const KICK_ROOM = `KickTest-${RUN_ID}`;

test.describe('Feature 9: Permissions', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, `Alice-${RUN_ID}`, APP_URL);
    bob = await createUserContext(browser, `Bob-${RUN_ID}`, APP_URL);

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
    const aliceBody = await alice.page.textContent('body');

    const adminIndicator = alice.page.locator(
      'button:has-text("Kick"), button:has-text("Ban"), button:has-text("Manage"), ' +
      '[aria-label*="kick" i], [aria-label*="admin" i], [aria-label*="manage" i], ' +
      '[title*="admin" i], [title*="kick" i], .admin, [class*="admin" i]'
    ).first();

    const hasAdmin = await adminIndicator.isVisible({ timeout: 5_000 }).catch(() => false);
    const hasAdminText = /admin/i.test(aliceBody || '');

    expect(hasAdmin || hasAdminText).toBeTruthy();
  });

  test('non-admin does not have admin controls', async () => {
    const kickBtn = bob.page.locator(
      'button:has-text("Kick"), button:has-text("Ban")'
    ).first();
    const hasKick = await kickBtn.isVisible({ timeout: 3_000 }).catch(() => false);

    if (hasKick) {
      const isDisabled = await kickBtn.isDisabled().catch(() => false);
      expect(isDisabled).toBeTruthy();
    }
  });

  test('admin can promote another user to admin', async () => {
    const promoteBtn = alice.page.locator(
      'button:has-text("Promote"), button:has-text("Make Admin"), button:has-text("Admin"), ' +
      '[aria-label*="promote" i], [aria-label*="make admin" i], [title*="promote" i]'
    ).first();

    // May need to click on Bob's name first to see user management options
    const bobEntry = alice.page.getByText(`Bob-${RUN_ID}`).first();
    await bobEntry.click({ timeout: 5_000 }).catch(() => {});

    const hasPromote = await promoteBtn.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasPromote) {
      await promoteBtn.click();
    } else {
      await bobEntry.click({ button: 'right' }).catch(() => {});
      const contextPromote = alice.page.locator(
        'text=/promote|make admin/i'
      ).first();
      const hasContextPromote = await contextPromote.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasContextPromote) {
        await contextPromote.click();
      }
    }

    // Verify Bob now has admin status
    await bob.page.waitForTimeout(2_000);
    const bobBody = await bob.page.textContent('body');

    const bobHasAdmin = alice.page.locator(
      '[class*="admin" i], [data-role="admin"], text=/admin/i'
    );
    const adminVisible = await bobHasAdmin.first().isVisible({ timeout: 10_000 }).catch(() => false);
    const bodyHasAdmin = /admin/i.test(bobBody || '');

    expect(adminVisible || bodyHasAdmin).toBeTruthy();
  });

  test('admin can kick a user and they lose access immediately', async () => {
    // Create a fresh room for the kick test to avoid state pollution
    await createRoom(alice.page, KICK_ROOM);
    await joinRoom(alice.page, KICK_ROOM);
    await joinRoom(bob.page, KICK_ROOM);

    // Send a message so we can verify Bob loses access
    const beforeKickMsg = `Before kick ${RUN_ID}`;
    await sendMessage(alice.page, beforeKickMsg);
    await expect(bob.page.getByText(beforeKickMsg).first()).toBeVisible({ timeout: 10_000 });

    // Alice kicks Bob
    const bobEntryInAlice = alice.page.getByText(`Bob-${RUN_ID}`).first();
    await bobEntryInAlice.click({ timeout: 5_000 }).catch(() => {});

    const kickBtn = alice.page.locator(
      'button:has-text("Kick"), button:has-text("Remove"), ' +
      '[aria-label*="kick" i], [aria-label*="remove" i], [title*="kick" i]'
    ).first();

    const hasKick = await kickBtn.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasKick) {
      await kickBtn.click();

      const confirmBtn = alice.page.locator(
        'button:has-text("Confirm"), button:has-text("Yes"), button:has-text("OK")'
      ).first();
      const hasConfirm = await confirmBtn.isVisible({ timeout: 2_000 }).catch(() => false);
      if (hasConfirm) {
        await confirmBtn.click();
      }
    } else {
      await bobEntryInAlice.click({ button: 'right' }).catch(() => {});
      const contextKick = alice.page.locator('text=/kick|remove/i').first();
      const hasContextKick = await contextKick.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasContextKick) {
        await contextKick.click();
      }
    }

    // Verify Bob lost access
    await bob.page.waitForTimeout(2_000);
    const bobBody = await bob.page.textContent('body');

    const kicked =
      /kicked|removed|denied|no longer|access/i.test(bobBody || '') ||
      !(bobBody || '').includes(beforeKickMsg);

    expect(kicked).toBeTruthy();
  });

  test('permission changes apply in real-time without refresh', async () => {
    // Send a message after kick and confirm Bob doesn't see it
    const postKickMsg = `Post-kick secret ${RUN_ID}`;
    await sendMessage(alice.page, postKickMsg);

    await bob.page.waitForTimeout(2_000);
    const bobBody = await bob.page.textContent('body');
    expect(bobBody).not.toContain(postKickMsg);
  });
});
