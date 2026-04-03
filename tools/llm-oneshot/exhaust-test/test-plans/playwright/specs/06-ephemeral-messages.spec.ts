// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

test.describe('Ephemeral Messages', () => {
  test('Disappearing Message UI', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForSelector('input, button', { timeout: 30_000 });

    // 1. Find the name input and type "Alice", then submit
    await page.getByRole('textbox', { name: 'Your name' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();
    await page.getByText('Alice').first().waitFor({ state: 'visible' });

    // 2. Create a room called "EphemeralTest" and enter it
    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name' }).fill('EphemeralTest');
    await page.getByRole('button', { name: 'Create' }).click();
    await page.getByText('# EphemeralTest').click();

    // 3. Look for an ephemeral/disappearing option (dropdown labeled "Disappear after:")
    const ephemeralSelect = page.getByLabel(/disappear after/i);
    await expect(ephemeralSelect).toBeVisible();

    // 4. Interact with it to set a duration (select a non-"Never" option)
    await ephemeralSelect.selectOption({ index: 1 });

    // 5. Send a message with the ephemeral option enabled
    const messageInput = page.getByRole('textbox', { name: /message/i });
    await messageInput.fill('Ephemeral message test');
    await page.keyboard.press('Enter');

    // 6. Verify the message appeared with a countdown or expiry indicator
    await expect(page.getByText('Ephemeral message test')).toBeVisible();
  });
});
