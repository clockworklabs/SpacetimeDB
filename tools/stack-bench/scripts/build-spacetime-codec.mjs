// Use the SDK's own installed build tool and source. The controller image pins
// both through its existing release source identity; no runtime SDK import.
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const sdk = new URL('../../../crates/bindings-typescript/package.json', import.meta.url);
const requireSdk = createRequire(sdk);
const { build } = await import(pathToFileURL(requireSdk.resolve('tsup')).href);
await build({
  entry: { 'spacetime-wire-codec': fileURLToPath(new URL('../src/stacks/spacetime-wire-codec.entry.mjs', import.meta.url)) },
  outDir: fileURLToPath(new URL('../dist/src/stacks', import.meta.url)),
  config: false, format: ['esm'], platform: 'node', target: 'node22',
  bundle: true, noExternal: [/.*/], dts: false, clean: false,
  outExtension: () => ({ js: '.js' }),
});
