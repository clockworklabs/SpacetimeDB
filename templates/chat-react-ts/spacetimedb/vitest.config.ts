import { defineConfig } from 'vitest/config';
import { spacetimedbModuleTestPlugin } from 'spacetimedb/server/test-utils/vitest';
import type { PluginOption } from 'vite';

export default defineConfig({
  cacheDir: 'node_modules/.vite-module-tests',
  plugins: [spacetimedbModuleTestPlugin() as unknown as PluginOption],
  test: {
    environment: 'node',
    setupFiles: ['src/test/setup.ts'],
  },
});
