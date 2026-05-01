import { join } from 'node:path';
import { sh } from './utils.ts';
import type { SpacetimeInitConfig } from '../config.ts';

export async function initSpacetime(config: SpacetimeInitConfig) {
  const useMaincloud = process.env.STDB_MAINCLOUD === '1';
  const server = useMaincloud
    ? 'maincloud'
    : process.env.STDB_SERVER || 'local';

  const { accounts, initialBalance, stdbModule: dbName, stdbModulePath: modulePath } =
    config;

  if (!dbName || !modulePath) {
    console.log('[spacetimedb] missing STDB_MODULE/STDB_MODULE_PATH; skipping');
    return;
  }

  // 1) Publish
  console.log(
    `[spacetimedb] publish "${dbName}" from ${modulePath} (server=${server})`,
  );
  try {
    await sh('spacetime', [
      'publish',
      '-c',
      '-y',
      '--server',
      server,
      '--module-path',
      modulePath,
      dbName,
    ]);
  } catch (e: any) {
    console.warn('[spacetimedb] publish likely already done:', e.message);
  }

  // 2) Generate TS bindings
  const outDir = join(process.cwd(), 'module_bindings');
  console.log(`[spacetimedb] generate bindings → ${outDir}`);
  await sh('spacetime', [
    'generate',
    '--lang',
    'typescript',
    '--out-dir',
    outDir,
    '--module-path',
    modulePath,
    '-y',
  ]);

  // 3) Seed
  console.log(
    `[spacetimedb] seed: n=${accounts} bal=${initialBalance}`,
  );
  try {
    await sh('spacetime', [
      'call',
      '--server',
      server,
      dbName,
      'seed',
      String(accounts),
      String(initialBalance),
    ]);
  } catch (e: any) {
    console.warn('[spacetimedb] seed reducer failed/missing:', e.message);
  }
}
