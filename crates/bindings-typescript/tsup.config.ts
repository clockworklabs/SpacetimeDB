// tsup.config.ts
import { defineConfig, type Options } from 'tsup';
import * as path from 'node:path';

function commonEsbuildTweaks() {
  return (options: any) => {
    options.conditions = ['development', 'import', 'default'];
    options.mainFields = ['browser', 'module', 'main'];
  };
}

const outExtension = (ctx: { format: string }) => ({
  js: ctx.format === 'cjs' ? '.cjs' : ctx.format === 'esm' ? '.mjs' : '.js',
});

const WS_BROWSER = path.resolve(__dirname, 'src/sdk/ws_browser.ts');
const WS_NODE = path.resolve(__dirname, 'src/sdk/ws_node.ts');

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
    esbuildOptions: o => {
      commonEsbuildTweaks()(o);
      o.alias = { ...(o.alias || {}), '#ws': WS_NODE };
    },
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
    esbuildOptions: o => {
      commonEsbuildTweaks()(o);
      o.alias = { ...(o.alias || {}), '#ws': WS_BROWSER };
    },
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
    esbuildOptions: o => {
      commonEsbuildTweaks()(o);
      o.alias = { ...(o.alias || {}), '#ws': WS_NODE };
    },
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
    esbuildOptions: o => {
      commonEsbuildTweaks()(o);
      o.alias = { ...(o.alias || {}), '#ws': WS_BROWSER };
    },
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
    esbuildOptions: o => {
      commonEsbuildTweaks()(o);
      o.alias = { ...(o.alias || {}), '#ws': WS_NODE };
    },
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
    esbuildOptions: o => {
      commonEsbuildTweaks()(o);
      o.alias = { ...(o.alias || {}), '#ws': WS_BROWSER };
    },
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
    platform: 'neutral',
    treeshake: 'smallest',
    external: ['undici'],
    outExtension,
    esbuildOptions: o => {
      commonEsbuildTweaks()(o);
      o.alias = { ...(o.alias || {}), '#ws': WS_NODE };
    },
  },

  // --- size-only minified builds below ---

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
    esbuildOptions: o => {
      commonEsbuildTweaks()(o);
      o.alias = { ...(o.alias || {}), '#ws': WS_BROWSER };
    },
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
    esbuildOptions: o => {
      commonEsbuildTweaks()(o);
      o.alias = { ...(o.alias || {}), '#ws': WS_BROWSER };
    },
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
    esbuildOptions: o => {
      commonEsbuildTweaks()(o);
      o.alias = { ...(o.alias || {}), '#ws': WS_BROWSER };
    },
  },
]) satisfies
  | Options
  | Options[]
  | ((
      overrideOptions: Options
    ) => Options | Options[] | Promise<Options | Options[]>);
