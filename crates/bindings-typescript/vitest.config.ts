import type { UserConfig } from 'vite';
import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  resolve: {
    alias: {
      'spacetime:sys@2.0': fileURLToPath(
        new URL('./tests/mocks/spacetime_sys_v2_0.ts', import.meta.url)
      ),
      'spacetime:sys@2.1': fileURLToPath(
        new URL('./tests/mocks/spacetime_sys_v2_1.ts', import.meta.url)
      ),
    },
  },
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
