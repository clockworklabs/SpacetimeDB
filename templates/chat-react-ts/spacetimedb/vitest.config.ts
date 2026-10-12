import { defineConfig, type Plugin } from 'vitest/config';
import { spacetimedbModuleTestPlugin } from 'spacetimedb/server/test-utils/vitest';

export default defineConfig({
  cacheDir: 'node_modules/.vite-module-tests',
  plugins: [spacetimedbModuleTestPlugin() as unknown as Plugin],
  test: {
    environment: 'node',
    setupFiles: ['src/test/setup.ts'],
  },
});
