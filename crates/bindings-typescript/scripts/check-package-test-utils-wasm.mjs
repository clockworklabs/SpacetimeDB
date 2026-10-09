import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const npmCache = mkdtempSync(join(tmpdir(), 'spacetimedb-npm-cache-'));
let output;
try {
  output = execFileSync(
    'npm',
    ['pack', '--dry-run', '--json', '--ignore-scripts'],
    {
      cwd: packageRoot,
      encoding: 'utf8',
      env: { ...process.env, npm_config_cache: npmCache },
    }
  );
} finally {
  rmSync(npmCache, { force: true, recursive: true });
}
const [pack] = JSON.parse(output);
const packagedFiles = new Set(pack.files.map(file => file.path));
const basename = 'spacetimedb_portable_datastore_wasm';
const requiredFiles = [
  `dist/server/test-utils/portable-datastore-wasm/${basename}.cjs`,
  `dist/server/test-utils/portable-datastore-wasm/${basename}.d.ts`,
  `dist/server/test-utils/portable-datastore-wasm/${basename}_bg.wasm`,
];
const missingFiles = requiredFiles.filter(file => !packagedFiles.has(file));

if (missingFiles.length > 0) {
  throw new Error(
    `Portable datastore Wasm files are missing from the npm package:\n${missingFiles.map(file => `  - ${file}`).join('\n')}`
  );
}

console.log('The npm package includes the portable datastore Wasm runtime.');
