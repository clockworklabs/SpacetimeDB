import { mkdirSync, realpathSync, symlinkSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';

const [sdkArg, projectArg] = process.argv.slice(2);
const sdk = resolve(sdkArg);
const project = resolve(projectArg);
mkdirSync(join(project, 'node_modules'), { recursive: true });
symlinkSync(
  sdk,
  join(project, 'node_modules/spacetimedb'),
  process.platform === 'win32' ? 'junction' : 'dir'
);
writeFileSync(
  join(project, 'tsconfig.json'),
  JSON.stringify({
    compilerOptions: {
      target: 'ES2022',
      module: 'ESNext',
      moduleResolution: 'Bundler',
      strict: true,
      skipLibCheck: true,
      types: [],
      outDir: 'dist',
      rootDir: '.',
      noEmitOnError: true,
    },
    include: ['**/*.ts'],
    exclude: ['node_modules', 'dist'],
  })
);
const result = spawnSync(
  process.execPath,
  [join(sdk, 'node_modules/typescript/bin/tsc'), '-p', project],
  { stdio: 'inherit' }
);
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
if (result.status === 0) {
  const { buildSync } = createRequire(
    realpathSync(join(sdk, 'node_modules/tsup/package.json'))
  )('esbuild');
  buildSync({
    entryPoints: [join(project, 'client.ts')],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    outfile: join(project, 'dist/client.cjs'),
  });
}
