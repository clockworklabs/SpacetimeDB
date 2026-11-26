import { configDefaults, defineConfig, type Plugin } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react() as unknown as Plugin],
  // Use one copy of React even when the SDK is linked from a local checkout.
  resolve: {
    dedupe: ['react', 'react-dom'],
  },
  test: {
    globals: true,
    environment: 'jsdom', // or "node" if you're not testing DOM
    setupFiles: './src/setupTests.ts',
    testTimeout: 15_000, // give extra time for real connections
    hookTimeout: 15_000,
    // The module's tests run separately, with its own config.
    exclude: [...configDefaults.exclude, 'spacetimedb/**'],
  },
});
