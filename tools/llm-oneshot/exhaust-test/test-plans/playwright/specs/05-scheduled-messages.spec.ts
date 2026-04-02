// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Scheduled Messages', () => {
  test('Schedule Message UI', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForSelector('input, button', { timeout: 30_000 });

    // 1. Find the name input and type "Alice", then submit
    await page.getByRole('textbox', { name: 'Your name...' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();

    // 2. Create a room called "ScheduleTest" and enter it
    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name...' }).fill('ScheduleTest');
    await page.getByRole('textbox', { name: 'Room name...' }).press('Enter');
    await page.getByText('#ScheduleTest').click();

    // 3. Look for a schedule button near the message input (Schedule checkbox)
    const scheduleCheckbox = page.getByRole('checkbox', { name: 'Schedule' });
    await expect(scheduleCheckbox).toBeVisible();

    // 4. Click the Schedule checkbox to activate scheduling
    await scheduleCheckbox.click();

    // 5. Look for a time/date picker or duration input
    // The schedule UI reveals a time input and a scheduled message textbox
    await expect(scheduleCheckbox).toBeChecked();

    // 6. Verify scheduling UI elements are present
    // The schedule input (datetime) and the scheduled message textbox appear
    await expect(page.getByRole('textbox', { name: 'Type a message to schedule...' })).toBeVisible();
    await expect(page.getByRole('button', { name: '⏰' })).toBeVisible();
  });
});
