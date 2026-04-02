import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { createUserContext, sendMessage, joinRoom } from '../fixtures';

let alice: { context: BrowserContext; page: Page };
let bob: { context: BrowserContext; page: Page };

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Feature 8: Edit History', () => {
  test.beforeAll(async ({ browser }) => {
    alice = await createUserContext(browser, 'Alice', APP_URL);
    bob = await createUserContext(browser, 'Bob', APP_URL);

    await joinRoom(alice.page, 'General');
    await joinRoom(bob.page, 'General');
  });

  test.afterAll(async () => {
    await alice?.context.close();
    await bob?.context.close();
  });

  test('can edit own message', async () => {
    // Alice sends a message to edit
    await sendMessage(alice.page, 'Original message text');
    await expect(alice.page.getByText('Original message text')).toBeVisible();
    await expect(bob.page.getByText('Original message text')).toBeVisible({ timeout: 10_000 });

    // Find the edit button — hover over message to reveal actions
    const messageEl = alice.page.getByText('Original message text');
    await messageEl.hover();

    // Look for edit button
    const editBtn = alice.page.locator(
      'button:has-text("Edit"), [aria-label*="edit" i], [title*="edit" i], ' +
      'button svg[class*="edit" i], button .edit'
    ).first();

    if (await editBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await editBtn.click();
    } else {
      // Try right-click context menu
      await messageEl.click({ button: 'right' });
      const editOption = alice.page.locator('text=/edit/i').first();
      if (await editOption.isVisible({ timeout: 2_000 }).catch(() => false)) {
        await editOption.click();
      }
    }

    // The message should become editable — find the edit input
    const editInput = alice.page.locator(
      'input[value="Original message text"], textarea, ' +
      'input[placeholder*="edit" i], [contenteditable="true"]'
    ).first();

    if (await editInput.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await editInput.clear();
      await editInput.fill('Edited message text');
    } else {
      // Some implementations replace the text inline — try selecting all and typing
      await alice.page.keyboard.press('Control+A');
      await alice.page.keyboard.type('Edited message text');
    }

    // Save the edit
    const saveBtn = alice.page.locator(
      'button:has-text("Save"), button:has-text("Done"), button:has-text("Update"), ' +
      'button:has-text("Confirm"), button[type="submit"]'
    ).first();

    if (await saveBtn.isVisible({ timeout: 3_000 }).catch(() => false)) {
      await saveBtn.click();
    } else {
      // Press Enter to save
      await alice.page.keyboard.press('Enter');
    }

    // Verify the edited text is now visible
    await expect(alice.page.getByText('Edited message text')).toBeVisible({ timeout: 5_000 });
  });

  test('edited indicator appears on edited messages', async () => {
    // The edited message should show an "(edited)" indicator
    await expect(
      alice.page.locator('text=/edited/i').first()
    ).toBeVisible({ timeout: 5_000 });
  });

  test('other user sees edit in real-time', async () => {
    // Bob should see the updated message text
    await expect(
      bob.page.getByText('Edited message text')
    ).toBeVisible({ timeout: 10_000 });

    // Original text should no longer be visible
    await expect(
      bob.page.getByText('Original message text')
    ).not.toBeVisible({ timeout: 3_000 });

    // Bob should also see the "(edited)" indicator
    await expect(
      bob.page.locator('text=/edited/i').first()
    ).toBeVisible({ timeout: 5_000 });
  });

  test('edit history is viewable by clicking the edited indicator', async () => {
    // Click on the "(edited)" text/link to view history
    const editedIndicator = alice.page.locator('text=/edited/i').first();
    await editedIndicator.click();

    // An edit history panel/modal/popover should appear
    // It should show the original version of the message
    await expect(
      alice.page.getByText('Original message text')
    ).toBeVisible({ timeout: 5_000 });

    // Verify the history shows both versions
    const historyBody = await alice.page.textContent('body');
    expect(historyBody).toContain('Edited message text');
    expect(historyBody).toContain('Original message text');
  });

  test('multiple edits are tracked in history', async () => {
    // Close any open history panel first
    await alice.page.keyboard.press('Escape');
    await alice.page.waitForTimeout(500);

    // Edit the message again
    const messageEl = alice.page.getByText('Edited message text');
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
      await editInput.fill('Second edit of message');
    } else {
      await alice.page.keyboard.press('Control+A');
      await alice.page.keyboard.type('Second edit of message');
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

    await expect(alice.page.getByText('Second edit of message')).toBeVisible({ timeout: 5_000 });

    // View history — should show all three versions
    const editedIndicator = alice.page.locator('text=/edited/i').first();
    await editedIndicator.click();

    await alice.page.waitForTimeout(1_000);
    const historyText = await alice.page.textContent('body');
    expect(historyText).toContain('Original message text');
    expect(historyText).toContain('Edited message text');
    expect(historyText).toContain('Second edit of message');
  });
});
