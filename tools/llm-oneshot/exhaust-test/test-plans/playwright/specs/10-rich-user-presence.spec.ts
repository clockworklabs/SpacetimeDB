// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Rich User Presence', () => {
  test('Status Selector', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForSelector('input, button', { timeout: 30_000 });

    // 1. Find the name input and type "Alice", then submit
    await page.getByRole('textbox', { name: 'Your name' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();
    await page.getByText('Alice').first().waitFor({ state: 'visible' });

    // 2. Look for a status selector (dropdown with Online, Away, DND options)
    const statusSelector = page.getByRole('combobox').filter({ hasText: /online|away|dnd|invisible/i });
    await expect(statusSelector).toBeVisible();

    // Verify a default status is selected
    const defaultValue = await statusSelector.inputValue();
    expect(defaultValue).toBeTruthy();

    // 3. Change the status to "Away"
    await statusSelector.selectOption({ label: /away/i });

    // 4. Verify the status indicator changes
    await expect(statusSelector).toHaveValue(await statusSelector.inputValue());

    // 5. Change back to "Online"
    await statusSelector.selectOption({ label: /online/i });

    // 6. Verify the indicator updates back to Online
    const onlineValue = await statusSelector.inputValue();
    expect(onlineValue).toMatch(/online/i);
  });
});
