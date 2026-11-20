// `spacetimedb/server` and `spacetimedb/server/test-utils` must share one copy
// of the server runtime. A module under unit test imports the first and is run
// by the harness in the second, so if each bundled its own copy, the harness's
// `instanceof` checks would not recognise a `Range` the module had built.

import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const entries = ['dist/server/index.mjs', 'dist/server/test-utils/index.mjs'];

function chunkImports(entry) {
  const path = resolve(packageRoot, entry);
  const code = readFileSync(path, 'utf8');
  if (/\bRange = class\b|\bclass Range\b/.test(code)) {
    throw new Error(`${entry} bundles its own copy of the Range class.`);
  }
  return new Set(
    [...code.matchAll(/from\s*["'](\.{1,2}\/[^"']*chunk-[^"']+)["']/g)].map(
      match => resolve(dirname(path), match[1])
    )
  );
}

const [server, testUtils] = entries.map(chunkImports);
const shared = [...server].filter(chunk => testUtils.has(chunk));
const definesRange = shared.filter(chunk =>
  /\bRange = class\b|\bclass Range\b/.test(readFileSync(chunk, 'utf8'))
);

if (definesRange.length !== 1) {
  throw new Error(
    `Expected ${entries.join(' and ')} to import the Range class from one shared chunk, found ${definesRange.length}.`
  );
}

console.log('The server and test-utils entry points share one server runtime.');
