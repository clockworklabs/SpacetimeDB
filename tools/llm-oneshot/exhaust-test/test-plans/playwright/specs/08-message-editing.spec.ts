// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

const APP_URL = process.env.APP_URL || 'http://localhost:5274';

test.describe('Message Editing', () => {
  test('Edit Own Message', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForSelector('input, button', { timeout: 30_000 });

    // 1. Find the name input and type "Alice", then submit
    await page.getByRole('textbox', { name: 'Enter your name' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();

    // 2. Create a room called "EditTest" and enter it
    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name...' }).fill('EditTest');
    await page.getByRole('textbox', { name: 'Room name...' }).press('Enter');
    await page.getByText('#EditTest').click();

    // 3. Send a message "Original message"
    await page.getByRole('textbox', { name: 'Type a message...' }).fill('Original message');
    await page.getByRole('textbox', { name: 'Type a message...' }).press('Enter');
    await expect(page.getByText('Original message')).toBeVisible();

    // 4. Hover over the message to reveal action buttons
    await page.getByText('Original message').hover();

    // 5. Look for an "Edit" button (✏️) and click it
    await page.getByRole('button', { name: '✏️' }).click();

    // 6. Change the text to "Edited message" and save
    const editInput = page.getByText('Editing message:').locator('..').getByRole('textbox');
    await editInput.fill('Edited message');
    await page.getByRole('button', { name: 'Save' }).click();

    // 7. Verify the message now shows "Edited message"
    await expect(page.getByText('Edited message')).toBeVisible();

    // 8. Look for an "(edited)" indicator on the message
    await expect(page.getByText(/edited/i)).toBeVisible();
  });
});
