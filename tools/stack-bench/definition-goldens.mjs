import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { compileScenarioDefinition } from './definition-compiler.mjs';
import { canonicalDefinitionJson, compileTrackPlan } from './definition-plan.mjs';
import { listTracks } from './tracks.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const GOLDEN_DIR = join(ROOT, 'tests', 'goldens', 'definitions');
const ALL_ACTIONS = join(ROOT, 'tests', 'fixtures', 'definitions', 'all-actions.json');

function atomicWrite(path, contents) {
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.tmp-${process.pid}`;
  writeFileSync(temporary, contents);
  renameSync(temporary, path);
}

export function currentDefinitionGoldens() {
  const entries = listTracks({ includeInternal: true }).map(name => ({
    name: `${name}.golden.json`,
    value: compileTrackPlan(name),
  }));
  const allActions = compileScenarioDefinition(JSON.parse(readFileSync(ALL_ACTIONS, 'utf8')), {
    source: ALL_ACTIONS,
  });
  entries.push({ name: 'all-actions.golden.json', value: allActions });
  return entries.sort((a, b) => a.name.localeCompare(b.name));
}

export function checkDefinitionGoldens({ update = false } = {}) {
  const entries = currentDefinitionGoldens();
  const changed = [];
  for (const entry of entries) {
    const path = join(GOLDEN_DIR, entry.name);
    const actual = canonicalDefinitionJson(entry.value);
    let expected = null;
    try { expected = readFileSync(path, 'utf8').replaceAll('\r\n', '\n'); }
    catch (error) {
      if (error.code !== 'ENOENT') throw error;
    }
    if (expected === actual) continue;
    changed.push(entry.name);
    if (update) atomicWrite(path, actual);
  }
  if (changed.length && !update) {
    throw new Error(`definition golden drift: ${changed.join(', ')}; inspect the semantic change, then run node definition-goldens.mjs --update`);
  }
  return { checked: entries.length, changed };
}

function main() {
  const args = new Set(process.argv.slice(2));
  for (const arg of args) {
    if (arg !== '--update') throw new Error(`unknown argument ${arg}`);
  }
  const result = checkDefinitionGoldens({ update: args.has('--update') });
  console.log(`${result.checked} definition goldens checked${result.changed.length
    ? `; ${result.changed.length} updated` : '; no drift'}`);
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) main();
