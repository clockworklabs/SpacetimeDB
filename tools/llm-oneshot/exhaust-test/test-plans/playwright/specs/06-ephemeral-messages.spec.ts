// spec: specs/plans/chat-app-features.md
// seed: specs/seed.spec.ts

import { test, expect } from '@playwright/test';

const APP_URL = process.env.APP_URL || 'http://localhost:5274';

test.describe('Ephemeral Messages', () => {
  test('Disappearing Message UI', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForSelector('input, button', { timeout: 30_000 });

    // 1. Find the name input and type "Alice", then submit
    await page.getByRole('textbox', { name: 'Enter your name' }).fill('Alice');
    await page.getByRole('button', { name: 'Join' }).click();

    // 2. Create a room called "EphemeralTest" and enter it
    await page.getByRole('button', { name: '+' }).click();
    await page.getByRole('textbox', { name: 'Room name...' }).fill('EphemeralTest');
    await page.getByRole('textbox', { name: 'Room name...' }).press('Enter');
    await page.getByText('#EphemeralTest').click();

    // 3. Look for an ephemeral/disappearing toggle (Ephemeral checkbox)
    const ephemeralCheckbox = page.getByRole('checkbox', { name: 'Ephemeral' });
    await expect(ephemeralCheckbox).toBeVisible();

    // 4. Interact with it to set a duration
    await ephemeralCheckbox.click();
    await expect(ephemeralCheckbox).toBeChecked();

    // Duration dropdown appears (30s, 1m, 5m, 1h options)
    const durationSelect = page.getByRole('combobox');
    await expect(durationSelect).toBeVisible();

    // 5. Send a message with the ephemeral option enabled
    const messageInput = page.getByRole('textbox', { name: /Send ephemeral message/ });
    await expect(messageInput).toBeVisible();
    await messageInput.fill('Ephemeral message test');
    await messageInput.press('Enter');

    // 6. Verify the message appeared (ephemeral messages display temporarily)
    await expect(page.getByText('Ephemeral message test')).toBeVisible();
  });
});
