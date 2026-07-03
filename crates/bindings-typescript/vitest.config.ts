import { fileURLToPath } from 'node:url';
import type { UserConfig } from 'vite';
import { defineConfig } from 'vitest/config';

const sysMock = fileURLToPath(
  new URL('./tests/__mocks__/spacetime-sys.ts', import.meta.url)
);

export default defineConfig({
  resolve: {
    // The `spacetime:sys@*` virtual modules are injected by the SpacetimeDB V8
    // host at runtime. Tests that import `src/server/runtime.ts` need a stub so
    // those host syscalls resolve under Node/vitest.
    alias: [
      { find: 'spacetime:sys@2.0', replacement: sysMock },
      { find: 'spacetime:sys@2.1', replacement: sysMock },
    ],
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
