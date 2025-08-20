import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'node',
    deps: { inline: ['spacetimedb'] },
  },
  resolve: {
    // include conditions that point to source in dev
    conditions: ['development', 'source', 'node', 'import', 'default'],
    mainFields: ['module', 'main', 'browser'],
    preserveSymlinks: true,
    extensions: ['.ts', '.tsx', '.mjs', '.js', '.json'],
  },
})