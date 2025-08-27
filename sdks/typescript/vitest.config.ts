import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    deps: {
      // Force Vite to process the workspace dependency from source
      inline: ['spacetimedb'],
    },
  },
  resolve: {
    // Make Vite/Vitest consider "source" entries from workspace packages
    conditions: ['source', 'import', 'default'],
    mainFields: ['module', 'main', 'browser'],
    preserveSymlinks: true, // useful with pnpm workspaces/links
    extensions: ['.ts', '.tsx', '.mjs', '.js', '.json'],
  },
});
