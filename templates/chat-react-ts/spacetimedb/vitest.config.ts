import { defineConfig } from 'vitest/config';
import { spacetimedbModuleTestPlugin } from 'spacetimedb/server/test-utils/vitest';

export default defineConfig({
  cacheDir: 'node_modules/.vite-module-tests',
  plugins: [spacetimedbModuleTestPlugin()],
  test: {
    environment: 'node',
    setupFiles: ['src/test/setup.ts'],
  },
});
