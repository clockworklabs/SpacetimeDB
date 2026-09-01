#!/usr/bin/env node

import { existsSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

import { compilePackDefinition, compileRecipeFile, resolveTaskFragment } from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.js';
import { buildRecipeRelease } from '../src/composition/recipe-release.js';
import { composeSelectedRecipeTask, selectRecipeRelease } from '../src/composition/recipe-selection.js';
import { TRACKS_DIR } from '../src/composition/tracks.js';
import type { CompiledPackDefinition, CompiledRecipePlan } from '../src/composition/composition-compiler.js';
import type { RecipeRelease } from '../src/composition/recipe-release.js';
import type { RecipeSelectionOptions, SelectedRecipeRelease } from '../src/composition/recipe-selection.js';

export { selectRecipeRelease } from '../src/composition/recipe-selection.js';

interface TrackRootOptions {
  trackRoot: string;
}

interface PackIndexEntry {
  pack: CompiledPackDefinition;
  path: string;
}

interface CalibrationValue {
  id: string;
  version: string;
  recipe?: { id?: string; version?: string; contentSha256?: string };
}

type RecipeOptions = TrackRootOptions & RecipeSelectionOptions;
type RecipeTaskKind = 'requirements' | 'contracts';

function json<T = unknown>(path: string, label: string): T {
  try { return JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`cannot read ${label} ${path}: ${message}`, { cause: error });
  }
}

function contained(root: string, path: string, label: string): string {
  const absoluteRoot = realpathSync(resolve(root));
  const candidate = resolve(path);
  const lexical = relative(absoluteRoot, candidate);
  if (lexical === '..' || lexical.startsWith(`..${sep}`)) throw new Error(`${label} escapes ${absoluteRoot}`);
  if (!existsSync(candidate)) throw new Error(`${label} does not exist: ${candidate}`);
  const absolute = realpathSync(candidate);
  const physical = relative(absoluteRoot, absolute);
  if (physical === '..' || physical.startsWith(`..${sep}`)) throw new Error(`${label} escapes ${absoluteRoot}`);
  return absolute;
}

function packIndex(trackRoot: string): Map<string, PackIndexEntry> {
  const directory = join(trackRoot, 'composition', 'packs');
  const byRef = new Map<string, PackIndexEntry>();
  for (const name of readdirSync(directory).filter(file => file.endsWith('.json')).sort()) {
    const path = join(directory, name);
    const pack = compilePackDefinition(json(path, 'pack'), {
      source: relative(trackRoot, path).replaceAll('\\', '/'),
    });
    const ref = `${pack.id}@${pack.version}`;
    if (byRef.has(ref)) throw new Error(`duplicate pack release ${ref}`);
    byRef.set(ref, { pack, path: realpathSync(path) });
  }
  for (const [ref, { pack }] of byRef) {
    for (const dependency of [...pack.requiresPacks, ...pack.conflictsWith]) {
      if (!byRef.has(dependency)) throw new Error(`${ref} references missing pack ${dependency}`);
    }
  }
  return byRef;
}

export function validatePackFile(path: string, options: Partial<TrackRootOptions> = {}) {
  const { trackRoot } = options;
  if (trackRoot === undefined) throw new Error('pack validation requires trackRoot');
  const root = realpathSync(resolve(trackRoot));
  const absolute = contained(join(root, 'composition'), path, 'pack path');
  const pack = compilePackDefinition(json(absolute, 'pack'), {
    source: relative(root, absolute).replaceAll('\\', '/'),
  });
  const packs = packIndex(root);
  const ownRef = `${pack.id}@${pack.version}`;
  const indexed = packs.get(ownRef);
  if (!indexed || indexed.path !== absolute) throw new Error(`${ownRef} is not the indexed source ${absolute}`);
  for (const ref of [...pack.requiresPacks, ...pack.conflictsWith]) {
    if (!packs.has(ref)) throw new Error(`${ownRef} references missing pack ${ref}`);
  }
  const sourceCache = new Map();
  for (const kind of ['requirements', 'contracts'] satisfies RecipeTaskKind[]) {
    for (const fragment of pack.task[kind]) {
      resolveTaskFragment(fragment, { trackRoot: root,
        source: `${relative(root, absolute).replaceAll('\\', '/')}.task.${kind}.${fragment.id}`,
        sourceCache });
    }
  }
  const state = new Map<string, 'visiting' | 'done'>();
  const visit = (ref: string, chain: string[] = []): void => {
    if (state.get(ref) === 'done') return;
    if (state.get(ref) === 'visiting') throw new Error(`pack dependency cycle: ${[...chain, ref].join(' -> ')}`);
    state.set(ref, 'visiting');
    const entry = packs.get(ref);
    if (!entry) throw new Error(`missing pack release ${ref}`);
    for (const dependency of entry.pack.requiresPacks) {
      if (!packs.has(dependency)) throw new Error(`${ref} references missing pack ${dependency}`);
      visit(dependency, [...chain, ref]);
    }
    state.set(ref, 'done');
  };
  visit(ownRef);
  let criteria = 0;
  for (const check of pack.checks) {
    const scenarioPath = contained(root, join(root, check.source), `${pack.id}.${check.id}.source`);
    const scenario = compileScenarioDefinition(json(scenarioPath, 'scenario'), {
      source: relative(root, scenarioPath).replaceAll('\\', '/'),
    });
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    if (!feature) throw new Error(`${pack.id}.${check.id} references missing feature ${check.feature}`);
    criteria += feature.criteria.length;
  }
  return { id: pack.id, version: pack.version, state: pack.state, path: absolute,
    checkGroups: pack.checks.length, criteria, requiresPacks: pack.requiresPacks };
}

export function validateRecipeFile(path: string, options: Partial<TrackRootOptions> = {}): {
  plan: CompiledRecipePlan;
  release: RecipeRelease;
} {
  const { trackRoot } = options;
  if (trackRoot === undefined) throw new Error('recipe validation requires trackRoot');
  const absolute = contained(join(trackRoot, 'composition'), path, 'recipe path');
  const plan = compileRecipeFile(absolute, { trackRoot });
  const release = buildRecipeRelease(absolute, { trackRoot });
  return { plan, release };
}

export function showRecipeFile(path: string, options: RecipeOptions): SelectedRecipeRelease & {
  builderTask: ReturnType<typeof composeSelectedRecipeTask> & { note: string };
} {
  const compiled = validateRecipeFile(path, options);
  const selected = selectRecipeRelease(compiled.release, options);
  const builderTask = composeSelectedRecipeTask(compiled.plan, selected.selection);
  return {
    ...selected,
    builderTask: {
      ...builderTask,
      note: 'Pack selection defines the requested task; a check-only filter narrows measurement inside it.',
    },
  };
}

const same = (left: unknown, right: unknown): boolean =>
  canonicalDefinitionJson(left) === canonicalDefinitionJson(right);

function meaningView(release: RecipeRelease) {
  return {
    track: release.track,
    task: release.task,
    checks: release.checkCatalog.map(({ stableKey, packId, checkGroupId, role, source,
      featureId, criterionId, description }) => ({ stableKey, packId, checkGroupId, role,
      source, featureId, criterionId, description })),
  };
}

function scoringView(release: RecipeRelease) {
  return { scoring: release.scoring,
    checks: release.checkCatalog.map(({ stableKey, points }) => ({ stableKey, points })) };
}

function metadataView(release: RecipeRelease) {
  return { id: release.id, version: release.version, state: release.state, title: release.title,
    sequence: release.sequence, sourceManifestSha256: release.sourceManifestSha256 };
}

function matchingCalibrations(trackRoot: string, release: RecipeRelease): Array<{
  path: string;
  value: CalibrationValue;
}> {
  const directory = join(trackRoot, 'composition', 'calibrations');
  if (!existsSync(directory)) return [];
  return readdirSync(directory).filter(name => name.endsWith('.json')).sort()
    .map(name => ({ path: join(directory, name),
      value: json<CalibrationValue>(join(directory, name), 'calibration') }))
    .filter(({ value }) => value.recipe?.id === release.id
      && value.recipe?.version === release.version
      && value.recipe?.contentSha256 === release.contentSha256);
}

export function diffRecipeFiles(fromPath: string, toPath: string, options: Partial<TrackRootOptions> = {}) {
  const { trackRoot } = options;
  if (trackRoot === undefined) throw new Error('recipe diff requires trackRoot');
  const from = validateRecipeFile(fromPath, { trackRoot }).release;
  const to = validateRecipeFile(toPath, { trackRoot }).release;
  const categories = {
    meaning: !same(meaningView(from), meaningView(to)),
    scoring: !same(scoringView(from), scoringView(to)),
    fixtures: !same(from.components.fixture, to.components.fixture),
    execution: from.executionSha256 !== to.executionSha256,
    metadata: !same(metadataView(from), metadataView(to)),
  };
  const recipeBindingChanged = from.id !== to.id || from.version !== to.version
    || from.meaningSha256 !== to.meaningSha256 || from.executionSha256 !== to.executionSha256
    || from.contentSha256 !== to.contentSha256;
  const calibrations = matchingCalibrations(trackRoot, from).map(({ path, value }) => {
    const invalidated = [];
    const stateChanged = from.state !== to.state;
    if (recipeBindingChanged) invalidated.push('recipe binding');
    if (stateChanged) invalidated.push('recipe qualification state');
    if (categories.fixtures) invalidated.push('fixture binding');
    if (categories.scoring) invalidated.push('zero-point control policy');
    if (categories.meaning || categories.scoring || categories.execution || categories.fixtures) {
      invalidated.push('reference repetitions', 'mutation repetitions');
    }
    if (categories.meaning || categories.scoring || categories.fixtures) invalidated.push('null repetitions');
    if (recipeBindingChanged || stateChanged) invalidated.push('promotion decision');
    return { id: value.id, version: value.version,
      path: relative(trackRoot, path).replaceAll('\\', '/'), invalidated: [...new Set(invalidated)] };
  });
  const fragmentDiff = (kind: RecipeTaskKind) => {
    const before = new Map(from.task[kind].map(fragment => [fragment.id, fragment]));
    const after = new Map(to.task[kind].map(fragment => [fragment.id, fragment]));
    return {
      added: [...after.keys()].filter(key => !before.has(key)).sort(),
      removed: [...before.keys()].filter(key => !after.has(key)).sort(),
      changed: [...after.keys()].filter(key => before.has(key)
        && !same(before.get(key), after.get(key))).sort(),
    };
  };
  return {
    from: { id: from.id, version: from.version, state: from.state, meaningSha256: from.meaningSha256,
      executionSha256: from.executionSha256, contentSha256: from.contentSha256 },
    to: { id: to.id, version: to.version, state: to.state, meaningSha256: to.meaningSha256,
      executionSha256: to.executionSha256, contentSha256: to.contentSha256 },
    categories,
    taskFragments: {
      requirements: fragmentDiff('requirements'),
      contracts: fragmentDiff('contracts'),
      composedTaskChanged: from.task.composedSha256 !== to.task.composedSha256,
    },
    calibrations,
  };
}

type CliSubject = 'pack' | 'recipe';
type CliCommand = 'validate' | 'show' | 'diff';

interface ParsedArgs extends RecipeSelectionOptions {
  json: boolean;
  positional: string[];
  packIds: string[];
  checkKeys: string[];
  track?: string;
  trackRoot?: string;
}

interface CliArgs extends ParsedArgs {
  subject: CliSubject;
  command: CliCommand;
  paths: string[];
  trackRoot: string;
}

function parse(argv: string[]): CliArgs {
  const { positionals, values } = parseArgs({ args: argv.slice(2), allowPositionals: true,
    options: { track: { type: 'string' }, 'track-root': { type: 'string' },
      pack: { type: 'string', multiple: true }, check: { type: 'string', multiple: true },
      json: { type: 'boolean' } } });
  const args: ParsedArgs = { json: values.json ?? false, positional: positionals,
    packIds: (values.pack ?? []).flatMap(value => value.split(',').filter(Boolean)),
    checkKeys: (values.check ?? []).flatMap(value => value.split(',').filter(Boolean)),
    track: values.track,
    trackRoot: values['track-root'] === undefined ? undefined : resolve(values['track-root']) };
  const [subject, command, ...paths] = args.positional;
  if (subject !== 'pack' && subject !== 'recipe') {
    throw new Error('usage: npm run pack -- validate <file> --track <name> | npm run recipe -- validate|show <file> --track <name> | npm run recipe -- diff <from> <to> --track <name>');
  }
  if (command !== 'validate' && command !== 'show' && command !== 'diff') {
    throw new Error('usage: npm run pack -- validate <file> --track <name> | npm run recipe -- validate|show <file> --track <name> | npm run recipe -- diff <from> <to> --track <name>');
  }
  if (subject === 'pack' && command !== 'validate') throw new Error(`pack ${command} is not supported`);
  if ((command === 'diff' ? paths.length !== 2 : paths.length !== 1)) throw new Error(`${subject} ${command} received the wrong number of paths`);
  if (!args.trackRoot && !args.track) throw new Error('--track or --track-root is required');
  if ((args.packIds.length || args.checkKeys.length) && !(subject === 'recipe' && command === 'show')) {
    throw new Error('--pack and --check are allowed only with recipe show');
  }
  const trackRoot = args.trackRoot ?? join(TRACKS_DIR, args.track ?? '');
  return { ...args, subject, command, paths, trackRoot };
}

function main() {
  const args = parse(process.argv);
  const firstPath = args.paths[0];
  if (firstPath === undefined) throw new Error('command requires a source path');
  let result: object;
  if (args.subject === 'pack') result = validatePackFile(firstPath, args);
  else if (args.command === 'diff') {
    const secondPath = args.paths[1];
    if (secondPath === undefined) throw new Error('recipe diff requires two source paths');
    result = diffRecipeFiles(firstPath, secondPath, args);
  } else if (args.command === 'show') result = showRecipeFile(firstPath, args);
  else {
    const compiled = validateRecipeFile(firstPath, args);
    result = {
      id: compiled.release.id, version: compiled.release.version, state: compiled.release.state,
      packs: compiled.release.components.packs.length, checks: compiled.release.checkCatalog.length,
      points: compiled.release.checkCatalog.reduce((total, check) => total + check.points, 0),
      meaningSha256: compiled.release.meaningSha256,
      executionSha256: compiled.release.executionSha256,
      contentSha256: compiled.release.contentSha256,
    };
  }
  if (args.json || args.command === 'show' || args.command === 'diff') console.log(JSON.stringify(result, null, 2));
  else if ('id' in result && 'version' in result && 'state' in result) {
    console.log(`${String(result.id)}@${String(result.version)} ${String(result.state)}: valid`);
  } else throw new Error('validation result has no release identity');
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) {
  try { main(); }
  catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 2;
  }
}
