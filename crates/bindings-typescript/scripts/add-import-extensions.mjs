// Adds the file extension to every extensionless relative import under the
// given directory (default `src`), so the published .d.ts files resolve under
// NodeNext. Run with `pnpm fix:import-extensions`, for example after a merge
// brings in imports without one; lint rejects them.
import {
  existsSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';

const root = process.argv[2] ?? 'src';
const specifier =
  /((?:\bfrom\s*|\bimport\s*\(\s*|\bimport\s+)['"])(\.{1,2}\/[^'"]*?|\.{1,2})(['"])/g;
let rewrites = 0;
const unresolved = [];

for (const entry of readdirSync(root, {
  recursive: true,
  withFileTypes: true,
})) {
  if (!/\.tsx?$/.test(entry.name) || entry.name.endsWith('.d.ts')) continue;
  const file = join(entry.parentPath, entry.name);
  const source = readFileSync(file, 'utf8');
  const next = source.replace(specifier, (match, before, spec, after) => {
    if (/\.(ts|tsx|js|mjs|cjs|json|css)$/.test(spec)) return match;
    const base = resolve(dirname(file), spec);
    for (const suffix of ['.ts', '.tsx', '/index.ts', '/index.tsx']) {
      if (existsSync(base + suffix) && statSync(base + suffix).isFile()) {
        rewrites += 1;
        return before + spec.replace(/\/$/, '') + suffix + after;
      }
    }
    unresolved.push(`${file}: ${spec}`);
    return match;
  });
  if (next !== source) writeFileSync(file, next);
}

console.log(`added extensions to ${rewrites} imports`);
if (unresolved.length) {
  console.error(`could not resolve:\n${unresolved.join('\n')}`);
  process.exitCode = 1;
}
