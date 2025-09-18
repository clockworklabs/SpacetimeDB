import type { UserConfig } from 'vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    deps: {
      external: ['spacetimedb'],
    },
  },
  resolve: {
    // Prefer source in dev *if your SDK exposes "source" in exports*.
    // Otherwise omit "source".
    conditions: ['source', 'development', 'node', 'import', 'default'],
    mainFields: ['module', 'main', 'browser'],
    preserveSymlinks: false,
    extensions: ['.ts', '.tsx', '.mjs', '.js', '.json'],
  },
  optimizeDeps: {
    esbuildOptions: { conditions: ['source', 'import', 'module', 'default'] },
    exclude: ['spacetimedb'],
  },
}) satisfies UserConfig as UserConfig;
