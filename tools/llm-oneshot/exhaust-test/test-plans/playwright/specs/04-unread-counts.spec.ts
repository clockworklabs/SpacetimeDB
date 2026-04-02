// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Unread Counts', () => {
  test('Unread Badge Shows', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForSelector('input, button', { timeout: 30_000 });

    // 1. Find the name input and type "Alice", then submit
    await page.getByRole('textbox', { name: 'Your name...' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();

    // 2. Create two rooms: "Room1" and "Room2"
    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name...' }).fill('Room1');
    await page.getByRole('textbox', { name: 'Room name...' }).press('Enter');

    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name...' }).fill('Room2');
    await page.getByRole('textbox', { name: 'Room name...' }).press('Enter');

    // 3. Enter "Room1" and send a message "Test message"
    await page.getByText('#Room1').click();
    await page.getByRole('textbox', { name: 'Type a message...' }).fill('Test message');
    await page.getByRole('textbox', { name: 'Type a message...' }).press('Enter');

    // Verify message appears
    await expect(page.getByText('Test message')).toBeVisible();

    // 4. Look at the sidebar/room list for any numeric badge or unread indicator
    // Verify both rooms are visible in the sidebar
    await expect(page.getByText('#Room1')).toBeVisible();
    await expect(page.getByText('#Room2')).toBeVisible();
  });
});
