import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  fullyParallel: false, // Run sequentially for real-time tests
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1, // Single worker for consistent timing
  reporter: [
    ['html', { open: 'never' }],
    ['json', { outputFile: 'results/test-results.json' }],
    ['list']
  ],
  timeout: 60000, // 60 second timeout per test
  expect: {
    timeout: 10000, // 10 second expect timeout
  },
  use: {
    baseURL: process.env.CLIENT_URL || 'http://localhost:5173',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },
  outputDir: 'results/artifacts',
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});

