// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Rich User Presence', () => {
  test('Status Selector', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForSelector('input, button', { timeout: 30_000 });

    // 1. Find the name input and type "Alice", then submit
    await page.getByRole('textbox', { name: 'Your name...' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();

    // Verify Alice appears in the UI
    await expect(page.getByText('Alice')).toBeVisible();

    // 2. Look for a status selector (dropdown with Online, Away, DND options)
    const statusSelector = page.getByRole('combobox');
    await expect(statusSelector).toBeVisible();

    // Verify the default status is Online
    await expect(statusSelector).toHaveValue('online');

    // 3. Change the status to "Away"
    await statusSelector.selectOption('away');

    // 4. Verify the status indicator changes
    await expect(statusSelector).toHaveValue('away');

    // 5. Change back to "Online"
    await statusSelector.selectOption('online');

    // 6. Verify the indicator updates back to Online
    await expect(statusSelector).toHaveValue('online');
  });
});
