import path from 'node:path';
import { defineConfig } from 'tsup';

// Hard path to the spacetimedb source file
const STDB_SRC = path.resolve(
  __dirname,
  '../../../../crates/bindings-typescript/src/index.ts'
);

// Minimal alias plugin: rewrites "spacetimedb" to STDB_SRC
function aliasSpacetimedb() {
  return {
    name: 'alias-spacetimedb',
    setup(build: any) {
      build.onResolve({ filter: /^spacetimedb$/ }, () => ({ path: STDB_SRC }));
    },
  };
}

function commonEsbuildTweaks() {
  return (options: any) => {
    // Prefer package.json "exports" condition "source"
    // Fall back to normal import/default if a dep doesn't have it
    options.conditions = ['source', 'import', 'default'];
    // Some ecosystems still look at these; harmless to set
    options.mainFields = ['browser', 'module', 'main'];
  };
}

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
    outDir: 'dist',
    clean: true,
    platform: 'browser',
    noExternal: ['spacetimedb', 'brotli', 'buffer'],
    treeshake: 'smallest',
    external: ['undici'],
    env: {
      BROWSER: 'false',
    },
    esbuildPlugins: [aliasSpacetimedb()],
    esbuildOptions: commonEsbuildTweaks(),
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
    noExternal: ['spacetimedb', 'brotli', 'buffer'],
    treeshake: 'smallest',
    external: ['undici'],
    env: {
      BROWSER: 'true',
    },
    esbuildPlugins: [aliasSpacetimedb()],
    esbuildOptions: commonEsbuildTweaks(),
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
    noExternal: ['spacetimedb', 'brotli', 'buffer', 'events'],
    treeshake: 'smallest',
    minify: 'terser',
    platform: 'browser',
    external: ['undici'],
    esbuildPlugins: [aliasSpacetimedb()],
    esbuildOptions: commonEsbuildTweaks(),
  },
]);
