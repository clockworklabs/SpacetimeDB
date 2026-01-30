import { join } from 'node:path';
import { ACC, BAL, sh } from './utils.ts';

export async function initSpacetime() {
  const useMaincloud = process.env.STDB_MAINCLOUD === '1';
  const server = useMaincloud
    ? 'maincloud'
    : process.env.STDB_SERVER || 'local';

  const dbName = process.env.STDB_MODULE!;
  const modulePath = process.env.STDB_MODULE_PATH!;

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
      '--project-path',
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
    '--project-path',
    modulePath,
  ]);

  // 3) Seed
  console.log(`[spacetimedb] seed: n=${ACC} bal=${BAL}`);
  try {
    await sh('spacetime', [
      'call',
      '--server',
      server,
      dbName,
      'seed',
      String(ACC),
      String(BAL),
    ]);
  } catch (e: any) {
    console.warn('[spacetimedb] seed reducer failed/missing:', e.message);
  }
}
