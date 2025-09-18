import { defineConfig, type Options } from 'tsup';

function commonEsbuildTweaks() {
  return (options: any) => {
    // Prefer "exports"."source" when deps provide it; harmless otherwise.
    options.conditions = ['source', 'import', 'default'];
    options.mainFields = ['browser', 'module', 'main'];
  };
}

export default defineConfig([
  // ESM wrapper -> dist/index.js
  {
    entry: { index: 'src/index.ts' },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist',
    dts: false, // types come from ./src in package.json
    sourcemap: true,
    clean: true,
    platform: 'neutral',
    noExternal: ['spacetimedb'],
    treeshake: 'smallest',
    external: ['undici'],
    esbuildOptions: commonEsbuildTweaks(),
  },

  // Browser-flavored wrapper -> dist/browser/index.js
  {
    entry: { index: 'src/index.ts' },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist/browser',
    dts: false,
    sourcemap: true,
    clean: true,
    platform: 'browser',
    noExternal: ['spacetimedb'],
    treeshake: 'smallest',
    external: ['undici'],
    esbuildOptions: commonEsbuildTweaks(),
  },

  // The below minified builds are not referenced in package.json and are
  // just included in the build for measuring the size impact of minification.
  // It is expected that consumers of the library will run their own
  // minification as part of their app bundling process.

  // Minified browser build -> dist/min/index.js
  {
    entry: { index: 'src/index.ts' },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist/min',
    dts: false,
    sourcemap: true,
    minify: 'terser',
    platform: 'browser',
    noExternal: ['spacetimedb'],
    treeshake: 'smallest',
    external: ['undici'],
    esbuildOptions: commonEsbuildTweaks(),
  },
]) satisfies
  | Options
  | Options[]
  | ((
    overrideOptions: Options
  ) => Options | Options[] | Promise<Options | Options[]>) as
  | Options
  | Options[]
  | ((
    overrideOptions: Options
  ) => Options | Options[] | Promise<Options | Options[]>);
