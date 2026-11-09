import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const sourceDir = resolve(
  packageRoot,
  'src/server/test-utils/portable-datastore-wasm'
);
const outputDir = resolve(
  packageRoot,
  'dist/server/test-utils/portable-datastore-wasm'
);

if (!existsSync(sourceDir)) {
  throw new Error(`Missing portable datastore wasm output: ${sourceDir}`);
}

rmSync(outputDir, { force: true, recursive: true });
mkdirSync(dirname(outputDir), { recursive: true });
cpSync(sourceDir, outputDir, { recursive: true });
