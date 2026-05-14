import type { UserConfig } from 'vite';
import { defineConfig } from 'vitest/config';
import { spacetimedbModuleTestPlugin } from './src/server/test-utils/vitest';

export default defineConfig({
  plugins: [spacetimedbModuleTestPlugin()],
  test: {
    include: ['tests/**/*.test.ts'],
    setupFiles: ['tests/setup.ts'],
    globals: true,
    environment: 'node',
    typecheck: {
      include: ['tests/**/*.test.ts'],
      tsconfig: './tsconfig.typecheck.json',
    },
  },
}) satisfies UserConfig as UserConfig;
