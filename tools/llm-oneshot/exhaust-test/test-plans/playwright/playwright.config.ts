import { defineConfig } from '@playwright/test';

const APP_URL = process.env.APP_URL || 'http://localhost:5173';

export default defineConfig({
  testDir: './specs',
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false, // features depend on prior state (room exists, users registered)
  retries: 0, // benchmark — no retries, failures are data
  reporter: [
    ['list'],
    ['json', { outputFile: 'test-results/results.json' }],
  ],
  use: {
    baseURL: APP_URL,
    headless: true,
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
});
