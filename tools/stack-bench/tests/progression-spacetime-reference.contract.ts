import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { compileProgressionDefinitionFile }
  from '../src/progression/progression-definition.js';

const root = STACK_BENCH_ROOT;
const appRoot = join(root, 'reference-apps', 'ecommerce', 'spacetime');
const trackRoot = join(root, 'tracks', 'ecommerce');
const read = (path: string): string => readFileSync(path, 'utf8');
const readJson = (path: string): unknown => JSON.parse(read(path));

function filesBelow(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? filesBelow(path) : [path];
  });
}

interface ScenarioInterface {
  roles: Set<string>;
  attributes: Set<string>;
}

function collectScenarioInterface(value: unknown, interfaceNames: ScenarioInterface): void {
  if (Array.isArray(value)) {
    for (const entry of value) collectScenarioInterface(entry, interfaceNames);
    return;
  }
  if (!isRecord(value)) return;
  if (typeof value.testid === 'string' && !(value.do === 'click' && value.ifAvailable === true)) {
    interfaceNames.roles.add(value.testid);
  }
  if (typeof value.attribute === 'string' && value.attribute.startsWith('data-')) {
    interfaceNames.attributes.add(value.attribute);
  }
  for (const entry of Object.values(value)) collectScenarioInterface(entry, interfaceNames);
}

test('the client implements every graph feature interface', () => {
  const graph = progressionGraph();
  const catalogPath = join(trackRoot, 'composition', 'recipes', 'progression-catalog.json');
  const packs = recipePackPaths(readJson(catalogPath), catalogPath);
  const interfaceNames: ScenarioInterface = { roles: new Set<string>(), attributes: new Set<string>() };

  for (const node of graph.nodes) {
    for (const featureRef of node.featureRefs) {
      const packPath = packs.get(featureRef);
      assert(packPath, `catalog must resolve ${featureRef}`);
      const pack = compilePackDefinition(readJson(packPath), { source: packPath });
      for (const check of pack.checks) {
        const scenarioPath = resolve(dirname(packPath), '..', '..', check.source);
        collectScenarioInterface(readJson(scenarioPath), interfaceNames);
      }
    }
  }

  const clientSource = filesBelow(join(appRoot, 'client', 'src'))
    .filter(path => /\.(tsx|ts)$/.test(path) && !path.includes('module_bindings'))
    .map(read)
    .join('\n');
  for (const role of interfaceNames.roles) {
    if (role === 'staff-role-account-staff') {
      assert(clientSource.includes('id={`staff-role-account-${encodeURIComponent(row.username)}`}'));
      continue;
    }
    assert(clientSource.includes(`data-role="${role}"`) || clientSource.includes(`id="${role}"`)
      || new RegExp(String.raw`data-role=\{[^}\n]*\? ['"]${role}['"] : undefined\}`).test(clientSource),
      `client must expose ${role}`);
  }
  for (const attribute of interfaceNames.attributes) {
    assert(clientSource.includes(`${attribute}=`), `client must expose ${attribute}`);
  }
});

function progressionGraph() {
  return compileProgressionDefinitionFile(join(trackRoot, 'progression', 'ecommerce.json'), {
    trackRoot,
  });
}

function recipePackPaths(value: unknown, catalogPath: string): Map<string, string> {
  if (!isRecord(value) || !Array.isArray(value.packs)) {
    throw new Error('progression catalog must have packs');
  }
  const packs = new Map<string, string>();
  for (const entry of value.packs) {
    if (!isRecord(entry) || typeof entry.id !== 'string' || typeof entry.path !== 'string') {
      throw new Error('progression catalog pack is not valid');
    }
    packs.set(entry.id, resolve(dirname(catalogPath), entry.path));
  }
  return packs;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
