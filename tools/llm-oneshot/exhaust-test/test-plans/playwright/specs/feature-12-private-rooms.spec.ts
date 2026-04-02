import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, createRoom, joinRoom, APP_URL, APP_URL_B } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };
let charlie: { context: BrowserContext; page: Page };

const PUBLIC_ROOM = `PublicRoom-${RUN_ID}`;
const SECRET_ROOM = `SecretRoom-${RUN_ID}`;
const ALICE = `Alice-${RUN_ID}`;
const BOB = `Bob-${RUN_ID}`;
const CHARLIE = `Charlie-${RUN_ID}`;

test.describe('Feature 12: Private Rooms & Direct Messages', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, ALICE, APP_URL);
    bob = await createUserContext(browser, BOB, APP_URL);
    charlie = await createUserContext(browser, CHARLIE, APP_URL);

    // Create a public room for baseline
    await createRoom(alice.page, PUBLIC_ROOM);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
    await charlie?.context.close();
  });

  test('can create a private room with privacy toggle', async () => {
    // Open create room dialog
    const createBtn = alice.page.locator(
      'button:has-text("Create"), button:has-text("New Room"), button:has-text("+"), [aria-label*="create" i]'
    ).first();
    await createBtn.click();

    // Fill room name
    const roomInput = alice.page.locator(
      'input[placeholder*="room" i], input[placeholder*="name" i]'
    ).first();
    await roomInput.fill(SECRET_ROOM);

    // Find and toggle private checkbox/switch
    const privateToggle = alice.page.locator(
      'input[type="checkbox"][name*="private" i], ' +
      'input[type="checkbox"][id*="private" i], ' +
      'label:has-text("Private"), label:has-text("Invite Only"), ' +
      '[class*="private" i] input[type="checkbox"], ' +
      '[aria-label*="private" i], [role="switch"]'
    ).first();

    const hasToggle = await privateToggle.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasToggle) {
      await privateToggle.click();
    } else {
      // Try finding a select/dropdown for room type
      const typeSelect = alice.page.locator(
        'select[name*="type" i], select[name*="visibility" i]'
      ).first();
      const hasSelect = await typeSelect.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasSelect) {
        await typeSelect.selectOption({ label: /private|invite/i });
      }
    }

    // Submit
    const submitBtn = alice.page.locator(
      'button:has-text("Create"), button[type="submit"]'
    ).first();
    if (await submitBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await submitBtn.click();
    } else {
      await roomInput.press('Enter');
    }

    // Wait for room creation
    await alice.page.waitForFunction(
      (name) => document.body.textContent?.includes(name),
      SECRET_ROOM,
      { timeout: 10_000 }
    );

    // Verify Alice sees the room
    await expect(alice.page.getByText(SECRET_ROOM).first()).toBeVisible();
  });

  test('private room is hidden from non-members', async () => {
    // Bob should NOT see SecretRoom in his room list
    await bob.page.waitForTimeout(2_000);
    const bobBody = await bob.page.textContent('body');
    expect(bobBody).not.toContain(SECRET_ROOM);

    // Charlie should also not see it
    const charlieBody = await charlie.page.textContent('body');
    expect(charlieBody).not.toContain(SECRET_ROOM);
  });

  test('can invite a user to private room', async () => {
    // Alice is in SecretRoom — look for invite button
    await joinRoom(alice.page, SECRET_ROOM);

    const inviteBtn = alice.page.locator(
      'button:has-text("Invite"), [aria-label*="invite" i], ' +
      '[title*="invite" i], button:has-text("Add Member"), ' +
      'button:has-text("Add User")'
    ).first();

    const hasInvite = await inviteBtn.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasInvite) {
      await inviteBtn.click();

      // Fill Bob's name in invite input
      const inviteInput = alice.page.locator(
        'input[placeholder*="user" i], input[placeholder*="invite" i], ' +
        'input[placeholder*="name" i], input[placeholder*="search" i]'
      ).first();
      const hasInput = await inviteInput.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasInput) {
        await inviteInput.fill(BOB);
        // Select Bob from results or press Enter
        const bobOption = alice.page.locator(`text=${BOB}`).first();
        await bobOption.click({ timeout: 3_000 }).catch(async () => {
          await inviteInput.press('Enter');
        });
      }

      // Click invite/confirm button if separate
      const confirmInvite = alice.page.locator(
        'button:has-text("Invite"), button:has-text("Send"), button:has-text("Add")'
      ).first();
      const hasConfirm = await confirmInvite.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasConfirm) {
        await confirmInvite.click();
      }
    }

    // Verify Bob gets an invitation or can now see the room
    await bob.page.waitForTimeout(3_000);
    const bobBody = await bob.page.textContent('body');
    const bobSees =
      new RegExp(SECRET_ROOM, 'i').test(bobBody || '') ||
      /invit/i.test(bobBody || '');

    expect(bobSees).toBeTruthy();
  });

  test('invited user can accept and access private room', async () => {
    // Check if Bob needs to accept an invitation
    const acceptBtn = bob.page.locator(
      'button:has-text("Accept"), button:has-text("Join"), ' +
      '[aria-label*="accept" i]'
    ).first();

    const hasAccept = await acceptBtn.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasAccept) {
      await acceptBtn.click();
    }

    // Try joining the room
    await joinRoom(bob.page, SECRET_ROOM).catch(() => {});

    // Verify Bob can see the room content
    await bob.page.waitForTimeout(2_000);
    const bobBody = await bob.page.textContent('body');
    expect(bobBody).toContain(SECRET_ROOM);

    // Send a message to verify full access
    const secretMsg = `Secret hello from ${ALICE} ${RUN_ID}`;
    await sendMessage(alice.page, secretMsg);
    await expect(bob.page.getByText(secretMsg).first()).toBeVisible({ timeout: 10_000 });
  });

  test('non-invited users still cannot see private room', async () => {
    // Charlie should still not see SecretRoom
    await charlie.page.waitForTimeout(2_000);
    const charlieBody = await charlie.page.textContent('body');
    expect(charlieBody).not.toContain(SECRET_ROOM);
  });

  test('direct message between users works', async () => {
    // Look for DM button near a user's name
    // Navigate to user list or member list first
    const bobEntry = alice.page.locator(`text=${BOB}`).first();
    await bobEntry.hover();

    const dmBtn = alice.page.locator(
      'button:has-text("DM"), button:has-text("Direct Message"), button:has-text("Message"), ' +
      '[aria-label*="direct" i], [aria-label*="dm" i], [aria-label*="message user" i], ' +
      '[title*="direct" i], [title*="message" i]'
    ).first();

    const hasDm = await dmBtn.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasDm) {
      await dmBtn.click();
    } else {
      // Try clicking on Bob's name — some apps open DM on user click
      await bobEntry.click();

      // Check if a DM option appeared
      const dmOption = alice.page.locator(
        'text=/direct message|send dm|message/i'
      ).first();
      const hasOption = await dmOption.isVisible({ timeout: 3_000 }).catch(() => false);
      if (hasOption) {
        await dmOption.click();
      }
    }

    // Wait for DM room to open
    await alice.page.waitForTimeout(2_000);

    // Send a DM
    const dmMsg = `Private hello ${RUN_ID}!`;
    await sendMessage(alice.page, dmMsg);

    // Verify Alice sees the message
    await expect(alice.page.getByText(dmMsg).first()).toBeVisible({ timeout: 5_000 });

    // Bob should see the DM conversation
    await bob.page.waitForTimeout(3_000);

    // Bob might need to click on the DM notification or conversation
    const dmNotification = bob.page.locator(
      `text=/${ALICE}|DM|Direct|Private hello/i`
    ).first();
    const hasNotif = await dmNotification.isVisible({ timeout: 5_000 }).catch(() => false);
    if (hasNotif) {
      await dmNotification.click();
    }

    // Verify Bob sees the DM message
    await expect(bob.page.getByText(dmMsg).first()).toBeVisible({ timeout: 10_000 });
  });
});
