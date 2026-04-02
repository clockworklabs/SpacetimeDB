import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { RUN_ID, createUserContext, sendMessage, joinRoom, APP_URL, APP_URL_B } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const ROOM = `General-${RUN_ID}`;
const ORIGINAL_MSG = `Original message ${RUN_ID}`;
const EDITED_MSG = `Edited message ${RUN_ID}`;
const SECOND_EDIT_MSG = `Second edit ${RUN_ID}`;

test.describe('Feature 8: Edit History', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, `Alice-${RUN_ID}`, APP_URL);
    bob = await createUserContext(browser, `Bob-${RUN_ID}`, APP_URL_B);

    await joinRoom(alice.page, ROOM);
    await joinRoom(bob.page, ROOM);
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('can edit own message', async () => {
    // Alice sends a message to edit
    await sendMessage(alice.page, ORIGINAL_MSG);
    await expect(alice.page.getByText(ORIGINAL_MSG).first()).toBeVisible();
    await expect(bob.page.getByText(ORIGINAL_MSG).first()).toBeVisible({ timeout: 10_000 });

    // Find the edit button — hover over message to reveal actions
    const messageEl = alice.page.getByText(ORIGINAL_MSG).first();
    await messageEl.hover();

    // Look for edit button
    const editBtn = alice.page.locator(
      'button:has-text("Edit"), [aria-label*="edit" i], [title*="edit" i], ' +
      'button svg[class*="edit" i], button .edit'
    ).first();

    if (await editBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await editBtn.click();
    } else {
      await messageEl.click({ button: 'right' });
      const editOption = alice.page.locator('text=/edit/i').first();
      if (await editOption.isVisible({ timeout: 2_000 }).catch(() => false)) {
        await editOption.click();
      }
    }

    // The message should become editable
    const editInput = alice.page.locator(
      `input[value="${ORIGINAL_MSG}"], textarea, ` +
      'input[placeholder*="edit" i], [contenteditable="true"]'
    ).first();

    if (await editInput.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await editInput.clear();
      await editInput.fill(EDITED_MSG);
    } else {
      await alice.page.keyboard.press('Control+A');
      await alice.page.keyboard.type(EDITED_MSG);
    }

    // Save the edit
    const saveBtn = alice.page.locator(
      'button:has-text("Save"), button:has-text("Done"), button:has-text("Update"), ' +
      'button:has-text("Confirm"), button[type="submit"]'
    ).first();

    if (await saveBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await saveBtn.click();
    } else {
      await alice.page.keyboard.press('Enter');
    }

    // Verify the edited text is now visible
    await expect(alice.page.getByText(EDITED_MSG).first()).toBeVisible({ timeout: 5_000 });
  });

  test('edited indicator appears on edited messages', async () => {
    await expect(
      alice.page.locator('text=/edited/i').first()
    ).toBeVisible({ timeout: 5_000 });
  });

  test('other user sees edit in real-time', async () => {
    await expect(
      bob.page.getByText(EDITED_MSG).first()
    ).toBeVisible({ timeout: 10_000 });

    // Original text should no longer be visible
    await expect(
      bob.page.getByText(ORIGINAL_MSG).first()
    ).not.toBeVisible({ timeout: 3_000 });

    await expect(
      bob.page.locator('text=/edited/i').first()
    ).toBeVisible({ timeout: 5_000 });
  });

  test('edit history is viewable by clicking the edited indicator', async () => {
    const editedIndicator = alice.page.locator('text=/edited/i').first();
    await editedIndicator.click();

    // An edit history panel/modal/popover should appear showing the original version
    await expect(
      alice.page.getByText(ORIGINAL_MSG).first()
    ).toBeVisible({ timeout: 5_000 });

    const historyBody = await alice.page.textContent('body');
    expect(historyBody).toContain(EDITED_MSG);
    expect(historyBody).toContain(ORIGINAL_MSG);
  });

  test('multiple edits are tracked in history', async () => {
    // Close any open history panel first
    await alice.page.keyboard.press('Escape');
    await alice.page.waitForTimeout(500);

    // Edit the message again
    const messageEl = alice.page.getByText(EDITED_MSG).first();
    await messageEl.hover();

    const editBtn = alice.page.locator(
      'button:has-text("Edit"), [aria-label*="edit" i], [title*="edit" i]'
    ).first();

    if (await editBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await editBtn.click();
    } else {
      await messageEl.click({ button: 'right' });
      const editOption = alice.page.locator('text=/edit/i').first();
      if (await editOption.isVisible({ timeout: 2_000 }).catch(() => false)) {
        await editOption.click();
      }
    }

    const editInput = alice.page.locator(
      'input, textarea, [contenteditable="true"]'
    ).first();

    if (await editInput.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await editInput.clear();
      await editInput.fill(SECOND_EDIT_MSG);
    } else {
      await alice.page.keyboard.press('Control+A');
      await alice.page.keyboard.type(SECOND_EDIT_MSG);
    }

    const saveBtn = alice.page.locator(
      'button:has-text("Save"), button:has-text("Done"), button:has-text("Update"), ' +
      'button[type="submit"]'
    ).first();
    if (await saveBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await saveBtn.click();
    } else {
      await alice.page.keyboard.press('Enter');
    }

    await expect(alice.page.getByText(SECOND_EDIT_MSG).first()).toBeVisible({ timeout: 5_000 });

    // View history — should show all three versions
    const editedIndicator = alice.page.locator('text=/edited/i').first();
    await editedIndicator.click();

    await alice.page.waitForTimeout(1_000);
    const historyText = await alice.page.textContent('body');
    expect(historyText).toContain(ORIGINAL_MSG);
    expect(historyText).toContain(EDITED_MSG);
    expect(historyText).toContain(SECOND_EDIT_MSG);
  });
});
