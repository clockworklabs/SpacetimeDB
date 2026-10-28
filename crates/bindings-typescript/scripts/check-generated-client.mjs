import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { join, dirname } from 'node:path';
import { readFileSync, writeFileSync } from 'node:fs';
import { build } from 'tsup';

const project = process.argv[2];
const require = createRequire(import.meta.url);
const legacyPackage = process.argv[3];
let legacyEntry;
if (legacyPackage) {
  legacyEntry = require.resolve(legacyPackage);
  const configPath = join(project, 'tsconfig.json');
  const config = JSON.parse(readFileSync(configPath, 'utf8'));
  // SDK 2.12 predates the exactOptionalPropertyTypes compatibility fix.
  config.compilerOptions.exactOptionalPropertyTypes = false;
  config.compilerOptions.paths = {
    spacetimedb: [join(dirname(legacyEntry), 'index.d.ts')],
  };
  writeFileSync(configPath, JSON.stringify(config));
}

execFileSync(
  process.execPath,
  [require.resolve('typescript/bin/tsc'), '-p', join(project, 'tsconfig.json')],
  { stdio: 'inherit' }
);
await build({
  entry: [join(project, 'consumer.ts')],
  outDir: join(project, 'out'),
  format: 'cjs',
  platform: 'node',
  config: false,
  silent: true,
  noExternal: [/^spacetimedb(?:\/|$)/],
  esbuildOptions(options) {
    if (legacyEntry) options.alias = { spacetimedb: legacyEntry };
  },
  tsconfig: join(project, 'tsconfig.json'),
  outExtension: () => ({ js: '.cjs' }),
});
execFileSync(process.execPath, [join(project, 'out/consumer.cjs')], {
  stdio: 'inherit',
});
