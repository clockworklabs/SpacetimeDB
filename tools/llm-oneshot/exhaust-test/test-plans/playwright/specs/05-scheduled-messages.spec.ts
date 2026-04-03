// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Scheduled Messages', () => {
  test('Schedule Message UI', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForSelector('input, button', { timeout: 30_000 });

    // 1. Find the name input and type "Alice", then submit
    await page.getByRole('textbox', { name: 'Your name' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();
    await page.getByText('Alice').first().waitFor({ state: 'visible' });

    // 2. Create a room called "ScheduleTest" and enter it
    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name' }).fill('ScheduleTest');
    await page.getByRole('button', { name: 'Create' }).click();
    await page.getByText('# ScheduleTest').click();

    // 3. Look for a schedule button near the message input (clock icon / aria-label)
    const scheduleBtn = page.getByRole('button', { name: /schedule/i });
    await expect(scheduleBtn).toBeVisible();

    // 4. Click the schedule button to activate scheduling UI
    await scheduleBtn.click();

    // 5 & 6. Verify scheduling UI elements are present (time/date picker or duration input)
    // After clicking, a datetime input or delay selector should appear
    const schedulingInput = page.locator('input[type="datetime-local"], input[type="time"], select').first();
    await expect(schedulingInput).toBeVisible();
  });
});
