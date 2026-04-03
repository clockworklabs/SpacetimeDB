import { test, expect } from '@playwright/test';

test.describe('Seed', () => {
  test('seed', async ({ page }) => {
    await page.goto('http://localhost:5274');
    await page.waitForSelector('input, button', { timeout: 30_000 });
  });
});
