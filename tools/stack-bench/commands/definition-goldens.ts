import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { canonicalDefinitionJson, compileTrackPlan } from '../src/composition/definition-plan.js';
import { listTracks } from '../src/composition/tracks.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const GOLDEN_DIR = join(STACK_BENCH_ROOT, 'tests', 'goldens', 'definitions');
const ALL_ACTIONS = join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'definitions', 'all-actions.json');

function atomicWrite(path: string, contents: string): void {
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.tmp-${process.pid}`;
  writeFileSync(temporary, contents);
  renameSync(temporary, path);
}

interface DefinitionGolden {
  name: string;
  value: unknown;
}

export function currentDefinitionGoldens(): DefinitionGolden[] {
  const entries: DefinitionGolden[] = listTracks({ includeInternal: true }).map(name => ({
    name: `${name}.golden.json`,
    value: compileTrackPlan(name),
  }));
  entries.push({
    name: 'all-actions.golden.json',
    value: compileScenarioDefinition(JSON.parse(readFileSync(ALL_ACTIONS, 'utf8')), {
      source: ALL_ACTIONS,
    }),
  });
  return entries.sort((a, b) => a.name.localeCompare(b.name));
}

export interface DefinitionGoldenResult {
  checked: number;
  changed: string[];
}

export function checkDefinitionGoldens(
  { update = false }: { update?: boolean } = {},
): DefinitionGoldenResult {
  const entries = currentDefinitionGoldens();
  const changed: string[] = [];
  for (const entry of entries) {
    const path = join(GOLDEN_DIR, entry.name);
    const actual = canonicalDefinitionJson(entry.value);
    let expected: string | null = null;
    try {
      expected = readFileSync(path, 'utf8').replaceAll('\r\n', '\n');
    } catch (error: unknown) {
      if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error;
    }
    if (expected === actual) continue;
    changed.push(entry.name);
    if (update) atomicWrite(path, actual);
  }
  if (changed.length > 0 && !update) {
    throw new Error(
      `definition golden drift: ${changed.join(', ')}; inspect the semantic change, then run npm run check:definition-goldens -- --update`,
    );
  }
  return { checked: entries.length, changed };
}

function main(): void {
  const args = new Set(process.argv.slice(2));
  for (const arg of args) {
    if (arg !== '--update') throw new Error(`unknown argument ${arg}`);
  }
  const result = checkDefinitionGoldens({ update: args.has('--update') });
  console.log(`${result.checked} definition goldens checked${
    result.changed.length > 0 ? `; ${result.changed.length} updated` : '; no drift'}`);
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) main();
