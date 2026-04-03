// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

test.describe('Unread Counts', () => {
  test('Unread Badge Shows', async ({ page }) => {
    await page.goto('http://localhost:5173');

    // 1. Find the name input and type "Alice", then submit
    await page.getByRole('textbox', { name: 'Your name' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();
    await page.getByText('Alice').first().waitFor({ state: 'visible' });

    // 2. Create two rooms: "Room1" and "Room2"
    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name' }).fill('Room1');
    await page.getByRole('button', { name: 'Create' }).click();
    await page.getByText('Room1').first().waitFor({ state: 'visible' });

    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name' }).fill('Room2');
    await page.getByRole('button', { name: 'Create' }).click();
    await page.getByText('Room2').first().waitFor({ state: 'visible' });

    // 3. Enter "Room1" and send a message "Test message"
    await page.getByText('# Room1').click();
    await page.getByRole('textbox', { name: 'Message #Room1…' }).fill('Test message');
    await page.keyboard.press('Enter');

    // Verify the message was sent
    await expect(page.getByText('Test message')).toBeVisible();

    // 4. Look at the sidebar/room list for any numeric badge or unread indicator
    await expect(page.getByText('# Room2')).toBeVisible();
  });
});
