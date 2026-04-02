// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Basic Chat', () => {
  test('User Registration and Room Creation', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForSelector('input, button', { timeout: 30_000 });

    // 1. Find the name/display name input field and type "Alice"
    await page.getByRole('textbox', { name: 'Your name...' }).fill('Alice');

    // 2. Click the join/register/submit button
    await page.getByRole('button', { name: 'Join' }).click();

    // 3. Verify "Alice" appears somewhere on the page
    await expect(page.getByText('Alice')).toBeVisible();

    // 4. Find the room name input and type "TestRoom"
    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name...' }).fill('TestRoom');

    // 5. Click the create/add room button (press Enter)
    await page.getByRole('textbox', { name: 'Room name...' }).press('Enter');

    // 6. Verify "TestRoom" appears in the room list
    await expect(page.getByText('#TestRoom')).toBeVisible();
  });

  test('Messaging Between Two Users', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForSelector('input, button', { timeout: 30_000 });

    // Register as Alice
    await page.getByRole('textbox', { name: 'Your name...' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();

    // Create a room
    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name...' }).fill('TestRoom');
    await page.getByRole('textbox', { name: 'Room name...' }).press('Enter');

    // 1. Click on "TestRoom" to enter it
    await page.getByText('#TestRoom').click();

    // 2. Find the message input field and type "Hello from Alice!"
    await page.getByRole('textbox', { name: 'Type a message...' }).fill('Hello from Alice!');

    // 3. Press Enter to send the message
    await page.getByRole('textbox', { name: 'Type a message...' }).press('Enter');

    // 4. Verify "Hello from Alice!" appears in the chat area
    await expect(page.getByText('Hello from Alice!')).toBeVisible();

    // 5. Verify the online/user list shows "Alice"
    await expect(page.getByText('Alice (you)')).toBeVisible();
  });
});
