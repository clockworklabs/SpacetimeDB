import { defineConfig, type Plugin } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react() as unknown as Plugin],
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/setupTests.ts',
  },
});
