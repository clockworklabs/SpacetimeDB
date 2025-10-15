// tsup.config.ts
import { defineConfig, type Options } from 'tsup';

function commonEsbuildTweaks() {
  return (options: any) => {
    // Prefer "exports"."development" when deps provide it; harmless otherwise.
    options.conditions = ['development', 'import', 'default'];
    options.mainFields = ['browser', 'module', 'main'];
  };
}

const outExtension = (ctx: { format: string }) => ({
  js: ctx.format === 'cjs' ? '.cjs' : ctx.format === 'esm' ? '.mjs' : '.js',
});

export default defineConfig([
  // Root wrapper (SSR-friendly): dist/index.{mjs,cjs}
  {
    entry: { index: 'src/index.ts' },
    format: ['esm', 'cjs'],
    target: 'es2022',
    outDir: 'dist',
    dts: false,
    sourcemap: true,
    clean: true,
    platform: 'neutral',
    treeshake: 'smallest',
    external: ['undici'],
    outExtension,
    esbuildOptions: commonEsbuildTweaks(),
  },

  // Browser-flavored root wrapper: dist/index.browser.mjs
  {
    entry: { 'index.browser': 'src/index.ts' },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist',
    dts: false,
    sourcemap: true,
    clean: true,
    platform: 'browser',
    treeshake: 'smallest',
    external: ['undici'],
    outExtension,
    esbuildOptions: commonEsbuildTweaks(),
  },

  // React subpath (SSR-friendly): dist/react/index.{mjs,cjs}
  {
    entry: { index: 'src/react/index.ts' },
    format: ['esm', 'cjs'],
    target: 'es2022',
    outDir: 'dist/react',
    dts: false,
    sourcemap: true,
    clean: true,
    platform: 'neutral',
    treeshake: 'smallest',
    outExtension,
    esbuildOptions: commonEsbuildTweaks(),
  },

  // React subpath (browser ESM): dist/browser/react/index.mjs
  {
    entry: { index: 'src/react/index.ts' },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist/browser/react',
    dts: false,
    sourcemap: true,
    clean: true,
    platform: 'browser',
    treeshake: 'smallest',
    outExtension,
    esbuildOptions: commonEsbuildTweaks(),
  },

  // SDK subpath (SSR-friendly): dist/sdk/index.{mjs,cjs}
  {
    entry: { index: 'src/sdk/index.ts' },
    format: ['esm', 'cjs'],
    target: 'es2022',
    outDir: 'dist/sdk',
    dts: false,
    sourcemap: true,
    clean: true,
    platform: 'neutral',
    treeshake: 'smallest',
    external: ['undici'],
    outExtension,
    esbuildOptions: commonEsbuildTweaks(),
  },

  // SDK browser ESM: dist/sdk/index.browser.mjs
  {
    entry: { 'index.browser': 'src/sdk/index.ts' },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist/sdk',
    dts: false,
    sourcemap: true,
    clean: true,
    platform: 'browser',
    treeshake: 'smallest',
    external: ['undici'],
    outExtension,
    esbuildOptions: commonEsbuildTweaks(),
  },

  // Server subpath (SSR / node-friendly): dist/server/index.{mjs,cjs}
  {
    entry: { index: 'src/server/index.ts' },
    format: ['esm', 'cjs'],
    target: 'es2022',
    outDir: 'dist/server',
    dts: false,
    sourcemap: true,
    clean: true,
    platform: 'neutral', // flip to 'node' if you actually rely on Node builtins
    banner: {
      js:
        'typeof globalThis!=="undefined"&&(' +
        '(globalThis.global=globalThis.global||globalThis),' +
        '(globalThis.window=globalThis.window||globalThis));',
    },
    treeshake: {
      moduleSideEffects: [
        'src/server/polyfills.ts',
        'src/server/register_hooks.ts',
      ],
    },
    external: ['undici', /^spacetime:sys.*$/],
    noExternal: ['base64-js', 'fast-text-encoding'],
    outExtension,
    esbuildOptions: commonEsbuildTweaks(),
  },

  // The below minified builds are not referenced in package.json and are
  // just included in the build for measuring the size impact of minification.
  // It is expected that consumers of the library will run their own
  // minification as part of their app bundling process.

  // Minified browser build: dist/min/index.browser.mjs
  {
    entry: { 'index.browser': 'src/index.ts' },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist/min',
    dts: false,
    sourcemap: true,
    minify: 'terser',
    platform: 'browser',
    treeshake: 'smallest',
    external: ['undici'],
    outExtension,
    esbuildOptions: commonEsbuildTweaks(),
  },

  // Minified browser React build: dist/min/react/index.mjs
  {
    entry: { index: 'src/react/index.ts' },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist/min/react',
    dts: false,
    sourcemap: true,
    minify: 'terser',
    platform: 'browser',
    external: ['undici'],
    treeshake: 'smallest',
    outExtension: ({ format }) => ({ js: format === 'cjs' ? '.cjs' : '.mjs' }),
    esbuildOptions: commonEsbuildTweaks(),
  },

  // Minified browser SDK build: dist/min/sdk/index.browser.mjs
  {
    entry: { 'index.browser': 'src/sdk/index.ts' },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist/min/sdk',
    dts: false,
    sourcemap: true,
    minify: 'terser',
    platform: 'browser',
    treeshake: 'smallest',
    external: ['undici'],
    outExtension: ({ format }) => ({ js: format === 'cjs' ? '.cjs' : '.mjs' }),
    esbuildOptions: commonEsbuildTweaks(),
  },
]) satisfies
  | Options
  | Options[]
  | ((
      overrideOptions: Options
    ) => Options | Options[] | Promise<Options | Options[]>);
