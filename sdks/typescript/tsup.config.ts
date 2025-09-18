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
    esbuildOptions: commonEsbuildTweaks(),
  },

  // Minified browser build -> dist/min/index.js (keeps your size script happy)
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
    esbuildOptions: commonEsbuildTweaks(),
  },

  // React subpath (SSR-friendly) -> dist/react/index.js
  {
    entry: { index: 'src/react/index.ts' },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist/react',
    dts: false, // wrapper doesn't own .d.ts; package.json points to src
    sourcemap: true,
    clean: true,
    platform: 'neutral',
    noExternal: ['spacetimedb'],
    treeshake: 'smallest',
    esbuildOptions: commonEsbuildTweaks(),
  },

  // React subpath (browser) -> dist/browser/react/index.js
  {
    entry: { index: 'src/react/index.ts' },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist/browser/react',
    dts: false,
    sourcemap: true,
    clean: true,
    platform: 'browser',
    noExternal: ['spacetimedb'],
    treeshake: 'smallest',
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
