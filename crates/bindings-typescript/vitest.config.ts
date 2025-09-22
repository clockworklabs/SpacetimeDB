import * as path from 'path';
import type { UserConfig } from 'vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.test.ts'],
    globals: true,
    environment: 'node',
  },
  resolve: {
    alias: {
      '#ws': path.resolve(__dirname, 'src/sdk/ws_browser.ts'),
    },
  },
}) satisfies UserConfig as UserConfig;
