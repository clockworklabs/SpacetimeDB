import type { UserConfig } from 'vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.test.ts'],
    globals: true,
    environment: 'node',
    typecheck: {
      include: ['tests/**/*.test.ts'],
      tsconfig: './tsconfig.typecheck.json',
    },
  },
}) satisfies UserConfig as UserConfig;
