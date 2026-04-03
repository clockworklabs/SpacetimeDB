// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Permissions', () => {
  test('Admin Controls Visible', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForSelector('input, button', { timeout: 30_000 });

    // 1. Find the name input and type "Alice", then submit
    await page.getByRole('textbox', { name: 'Your name' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();
    await page.getByText('Alice').first().waitFor({ state: 'visible' });

    // 2. Create a room called "AdminTest" and enter it
    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name' }).fill('AdminTest');
    await page.getByRole('button', { name: 'Create' }).click();
    await page.getByText('# AdminTest').click();

    // 3. Look for admin-related buttons in the room header or member list
    const membersBtn = page.getByRole('button', { name: /members|manage|admin/i });
    await expect(membersBtn).toBeVisible();
    await membersBtn.click();

    // 4. Check if Alice (room creator) shows as admin
    await expect(page.getByText(/alice.*admin|admin.*alice/i)).toBeVisible();
  });
});
