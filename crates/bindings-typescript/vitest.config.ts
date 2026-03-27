import type { UserConfig } from 'vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.test.ts'],
    globals: true,
    environment: 'node',
    typecheck: {
      include: ['src/**/*.test-d.ts'],
      tsconfig: './tsconfig.typecheck.json',
    },
  },
}) satisfies UserConfig as UserConfig;
