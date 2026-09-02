import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { compileProgressionDefinitionFile } from '../src/progression/progression-definition.js';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
const stackBenchRoot = STACK_BENCH_ROOT;
const trackRoot = join(stackBenchRoot, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const appRoot = join(stackBenchRoot, 'reference-apps', 'ecommerce', 'mongodb');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function visit(value: unknown, callback: (item: Record<string, unknown>) => void): void {
  if (Array.isArray(value)) value.forEach(entry => visit(entry, callback));
  else if (record(value)) {
    callback(value);
    Object.values(value).forEach(entry => visit(entry, callback));
  }
}

test('the MongoDB progression reference exposes every graph testing hook', () => {
  const graph = compileProgressionDefinitionFile(join(trackRoot, 'progression', 'ecommerce-2.0.1.json'));
  const packs = new Map(readdirSync(packRoot).filter(name => name.endsWith('.json')).map(name => {
    const pack = compilePackDefinition(readJson(join(packRoot, name)), { source: name });
    return [`${pack.id}@${pack.version}`, pack];
  }));
  const packRefs = new Set(graph.nodes.flatMap(node => node.featureRefs));
  const testIds = new Set<string>();

  for (const reference of packRefs) {
    const pack = packs.get(reference);
    assert(pack, `missing graph pack ${reference}`);
    for (const check of pack.checks ?? []) {
      const scenario = readJson(join(trackRoot, check.source));
      visit(scenario, value => {
        if (typeof value.testid === 'string') testIds.add(value.testid);
      });
    }
  }

  const clientSource = readdirSync(join(appRoot, 'client', 'src'))
    .filter(name => name.endsWith('.tsx'))
    .map(name => readFileSync(join(appRoot, 'client', 'src', name), 'utf8'))
    .join('\n');
  const missing = [...testIds].filter(id => !clientSource.includes(id)).sort();
  assert.deepEqual(missing, []);
});

test('the MongoDB progression reference binds named actions and its runtime', () => {
  const clientSource = readFileSync(join(appRoot, 'client', 'src', 'ProgressionPanel.tsx'), 'utf8');
  const serverSource = [
    readFileSync(join(appRoot, 'server', 'src', 'index.ts'), 'utf8'),
    readFileSync(join(appRoot, 'server', 'src', 'progression.ts'), 'utf8'),
  ].join('\n');
  const reservationSource = readFileSync(join(appRoot, 'server', 'src', 'stock-reservations.ts'), 'utf8');
  const reference = readJson(join(appRoot, 'reference.json'));
  const serverPackage = readJson(join(appRoot, 'server', 'package.json'));
  const clientPackage = readJson(join(appRoot, 'client', 'package.json'));
  assert(record(reference));
  assert(record(serverPackage));
  assert(record(clientPackage));
  assert(Array.isArray(reference.installDirectories));
  assert(record(reference.server));
  assert(record(reference.client));
  assert(record(serverPackage.scripts));
  assert(record(clientPackage.scripts));
  const viteConfig = readFileSync(join(appRoot, 'client', 'vite.config.ts'), 'utf8');

  assert.match(clientSource, /data-action-input/);
  assert.match(clientSource, /\/api\/support\/cases\/\$\{ticket\.id\}\/order/);
  assert.match(clientSource, /\/api\/support\/cases\/\$\{ticket\.id\}\/refund/);
  assert.match(clientSource, /\/api\/admin\/scheduled-restocks/);
  assert.match(serverSource, /app\.use\("\/api\/support", supportRouter\)/);
  assert.match(serverSource, /app\.use\("\/api\/admin", adminRouter\)/);
  assert.match(serverSource, /installProgressionRoutes\(app, io/);
  assert.match(serverSource, /JWT_SECRET \|\| "stack-bench-reference-only-secret-2026"/);
  assert.match(serverSource, /reserveStock\(item\._id, qty\)/);
  assert.match(reservationSource, /quantity: \{ \$gte: 1 \}/);
  assert.match(reservationSource, /\$inc: \{ quantity: -1 \}/);
  assert.deepEqual(reference.installDirectories, ['server', 'client']);
  assert.deepEqual(reference.server, { directory: 'server' });
  assert.deepEqual(reference.client, { directory: 'client' });
  assert.equal(serverPackage.scripts.typecheck, 'tsc --noEmit');
  assert.equal(serverPackage.scripts.test, 'tsx --test src/*.test.ts');
  assert.equal(clientPackage.scripts.build, 'vite build');
  assert.match(viteConfig, /Number\(process\.env\.API_PORT\) \|\| 6401/);
  assert.match(viteConfig, /Number\(process\.env\.VITE_PORT\) \|\| 6673/);
  assert.match(viteConfig, /host: "0\.0\.0\.0"/);
});
