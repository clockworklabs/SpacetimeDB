import { defineConfig } from 'vite';
import { resolve } from 'path';

export default defineConfig({
  build: {
    lib: {
      entry: resolve(__dirname, 'src/bindings.ts'),
      name: 'Bindings',
      fileName: 'bindings',
      formats: ['iife'],
    },
    outDir: 'dist',
  },
});
