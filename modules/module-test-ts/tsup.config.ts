// tsup.config.ts
import { defineConfig, type Options } from 'tsup';

const outExtension = (ctx: { format: string }) => ({
  js: ctx.format === 'cjs' ? '.cjs' : ctx.format === 'esm' ? '.mjs' : '.js',
});

export default defineConfig([
  {
    entry: { index: 'src/index.ts' },
    format: ['esm'],
    target: 'es2022',
    outDir: 'dist',
    dts: false,
    sourcemap: true,
    clean: true,
    platform: 'neutral', // flip to 'node' if you actually rely on Node builtins
    treeshake: 'smallest',
    external: ['undici'],
    noExternal: ['base64-js', 'fast-text-encoding'],
    outExtension,
  },
]) satisfies
  | Options
  | Options[]
  | ((
      overrideOptions: Options
    ) => Options | Options[] | Promise<Options | Options[]>);
