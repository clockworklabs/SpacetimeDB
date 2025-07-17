import { defineConfig } from 'tsup';

export default defineConfig([
  {
    entryPoints: {
      index: 'src/index.ts',
    },
    format: ['esm'],
    target: 'es2022',
    legacyOutput: false,
    dts: {
      resolve: true,
    },
    clean: true,
    platform: 'browser',
    noExternal: ['brotli', 'buffer'],
    treeshake: 'smallest',
    external: ['undici'],
    env: {
      BROWSER: 'false',
    },
  },
  {
    entryPoints: {
      index: 'src/index.ts',
    },
    format: ['esm'],
    target: 'es2022',
    legacyOutput: false,
    dts: false,
    outDir: 'dist/browser',
    clean: true,
    platform: 'browser',
    noExternal: ['brotli', 'buffer'],
    treeshake: 'smallest',
    external: ['undici'],
    env: {
      BROWSER: 'true',
    },
  },
  {
    entryPoints: {
      index: 'src/index.ts',
    },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist/min',
    dts: false,
    sourcemap: true,
    noExternal: ['brotli', 'buffer', 'events'],
    treeshake: 'smallest',
    minify: 'terser',
    platform: 'browser',
    external: ['undici'],
  },
]);
