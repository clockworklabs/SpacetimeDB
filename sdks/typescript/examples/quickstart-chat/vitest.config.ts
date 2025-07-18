import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: 'jsdom', // or "node" if you're not testing DOM
    setupFiles: './src/setupTests.ts',
    testTimeout: 30_000, // give extra time for real connections
    hookTimeout: 30_000,
  },
});
