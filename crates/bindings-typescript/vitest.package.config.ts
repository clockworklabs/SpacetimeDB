import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.package.test.ts'],
    testTimeout: 30_000,
  },
});
