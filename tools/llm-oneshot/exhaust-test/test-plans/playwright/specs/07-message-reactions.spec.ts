// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Message Reactions', () => {
  test('Add Reaction to Message', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForSelector('input, button', { timeout: 30_000 });

    // 1. Find the name input and type "Alice", then submit
    await page.getByRole('textbox', { name: 'Your name' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();
    await page.getByText('Alice').first().waitFor({ state: 'visible' });

    // 2. Create a room called "ReactionTest" and enter it
    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name' }).fill('ReactionTest');
    await page.getByRole('button', { name: 'Create' }).click();
    await page.getByText('# ReactionTest').click();

    // 3. Send a message "React to this!"
    await page.getByRole('textbox', { name: 'Message #ReactionTest…' }).fill('React to this!');
    await page.keyboard.press('Enter');
    await expect(page.getByText('React to this!')).toBeVisible();

    // 4. Hover over the message to reveal action buttons
    await page.getByText('React to this!').hover();

    // 5. Look for a reaction button and click it
    const reactBtn = page.getByRole('button', { name: /react|emoji|😊/i }).first();
    await expect(reactBtn).toBeVisible();
    await reactBtn.click();

    // 6. Select an emoji from the picker (👍)
    await page.getByRole('button', { name: '👍' }).click();

    // 7. Verify a reaction count appears on the message
    await expect(page.getByText('👍')).toBeVisible();
  });
});
