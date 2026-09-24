// Mirrors dist/**/*.d.ts as .d.cts so `require` consumers get CommonJS type
// declarations. The package is `"type": "module"`, so its .d.ts files are ESM
// types and would otherwise describe the .cjs builds as ES modules.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const dist = fileURLToPath(new URL('../dist', import.meta.url));
const relativeTs = /((?:from\s+|import\()['"]\.{1,2}\/[^'"]+)\.ts(['"])/g;

for (const entry of readdirSync(dist, {
  recursive: true,
  withFileTypes: true,
})) {
  if (!entry.isFile() || !entry.name.endsWith('.d.ts')) continue;
  const file = join(entry.parentPath, entry.name);
  const source = readFileSync(file, 'utf8');
  writeFileSync(
    file.replace(/\.d\.ts$/, '.d.cts'),
    source.replace(relativeTs, '$1.cjs$2')
  );
}
