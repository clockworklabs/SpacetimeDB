#!/usr/bin/env node
// Check scenario action names, actors, UI hooks, and score totals without an app.

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { compileRecipeFile, type CompiledOwnedTaskFragment, type CompiledRecipeRelease }
  from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition, type CompiledStep }
  from '../src/composition/definition-compiler.js';
import { DEFAULT_TRACK, listTracks, loadTrack, type Track }
  from '../src/composition/tracks.js';

interface ScenarioScope {
  features: Map<number, Set<string>>;
  contractOwners: Set<string>;
  requirementOwners: Set<string>;
  contractText: string;
  requirementText: string;
}

interface RecipeSource {
  baseRecipe: string | null;
  isolatesSelectedSources: boolean;
}

type HooksByLevel = Map<string, Set<string>>;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readJson(path: string): unknown {
  return JSON.parse(readFileSync(path, 'utf8')) as unknown;
}

function optionValue(args: readonly string[], option: string): string | null {
  const index = args.indexOf(option);
  if (index === -1) return null;
  const value = args[index + 1];
  if (value === undefined || value.startsWith('--')) {
    throw new Error(`${option} requires a value`);
  }
  return value;
}

function readRecipeSource(path: string): RecipeSource {
  const source = readJson(path);
  if (!isRecord(source)) throw new Error(`${path}: recipe must be an object`);
  const task = source.task;
  if (!isRecord(task)) throw new Error(`${path}: recipe task must be an object`);
  const baseRecipe = task.baseRecipe;
  let baseRecipePath: string | null = null;
  if (baseRecipe !== undefined) {
    if (!isRecord(baseRecipe) || typeof baseRecipe.path !== 'string') {
      throw new Error(`${path}: task.baseRecipe.path must be a string`);
    }
    baseRecipePath = baseRecipe.path;
  }
  return {
    baseRecipe: baseRecipePath,
    isolatesSelectedSources: source.execution === 'all-selected-sources',
  };
}

function contractHookIds(path: string): string[] {
  const contract = readJson(path);
  if (!isRecord(contract) || !Array.isArray(contract.hooks)) {
    throw new Error(`${path}: hooks must be an array`);
  }
  return contract.hooks.map((hook, index) => {
    if (!isRecord(hook) || typeof hook.id !== 'string') {
      throw new Error(`${path}: hooks[${index}].id must be a string`);
    }
    return hook.id;
  });
}

// Contract levels are cumulative. A level can use hooks introduced earlier.
function hooksByLevel(track: Track): HooksByLevel {
  const perFile = new Map<string, string[]>();
  for (const file of readdirSync(track.contracts).filter(name => /^\d\d-.*\.json$/.test(name))) {
    perFile.set(file.slice(0, 2), contractHookIds(join(track.contracts, file)));
  }
  const byLevel: HooksByLevel = new Map();
  for (const level of perFile.keys()) {
    const ids = [...perFile.entries()]
      .filter(([candidate]) => candidate <= level)
      .flatMap(([, hookIds]) => hookIds);
    byLevel.set(level, new Set(ids));
  }
  return byLevel;
}

function packId(reference: string): string {
  return reference.slice(0, reference.lastIndexOf('@'));
}

function ownedFragment(
  fragment: CompiledOwnedTaskFragment,
  owners: ReadonlySet<string>,
): boolean {
  return fragment.owners.some(owner => owners.has(owner));
}

function recipeScenarioScopes(track: Track, recipeFile: string): Map<string, ScenarioScope> {
  const recipeDir = join(track.dir, 'composition', 'recipes');
  const chain: CompiledRecipeRelease[] = [];
  const seen = new Set<string>();
  let currentFile: string | null = recipeFile;
  let isolatesSelectedSources = false;
  while (currentFile !== null) {
    if (seen.has(currentFile)) throw new Error(`recipe base cycle at ${currentFile}`);
    seen.add(currentFile);
    const path = join(recipeDir, currentFile);
    chain.push(compileRecipeFile(path, { trackRoot: track.dir }));
    const source = readRecipeSource(path);
    if (chain.length === 1) isolatesSelectedSources = source.isolatesSelectedSources;
    currentFile = source.baseRecipe;
  }

  const recipe = chain[0];
  if (recipe === undefined) throw new Error(`recipe chain is empty for ${recipeFile}`);
  const packs = new Map(recipe.packs.map(pack => [pack.id, pack]));
  const contracts = isolatesSelectedSources
    ? recipe.recipe.task.contracts
    : chain.flatMap(release => release.recipe.task.contracts);
  const requirements = isolatesSelectedSources
    ? recipe.recipe.task.requirements
    : chain.flatMap(release => release.recipe.task.requirements);
  const scopes = new Map<string, ScenarioScope>();

  const ownersFor = (check: CompiledRecipeRelease['checks'][number]): Set<string> => {
    const found = new Set([check.packId, ...(check.requiresFeatures ?? [])]);
    const visit = (id: string): void => {
      const pack = packs.get(id);
      if (pack === undefined) return;
      for (const reference of pack.requiresPacks) {
        const dependency = packId(reference);
        if (found.has(dependency)) continue;
        found.add(dependency);
        visit(dependency);
      }
    };
    [...found].forEach(visit);
    return found;
  };

  for (const check of recipe.checks) {
    const source = check.source.replace(/^scenarios\//, '');
    const scope = scopes.get(source) ?? {
      features: new Map<number, Set<string>>(),
      contractOwners: new Set<string>(),
      requirementOwners: new Set<string>(),
      contractText: '',
      requirementText: '',
    };
    const criteria = scope.features.get(check.featureId) ?? new Set<string>();
    criteria.add(check.criterionId);
    scope.features.set(check.featureId, criteria);
    for (const owner of ownersFor(check)) {
      scope.contractOwners.add(owner);
      scope.requirementOwners.add(owner);
    }
    scopes.set(source, scope);
  }

  for (const scope of scopes.values()) {
    const selectedContracts = isolatesSelectedSources
      ? contracts.filter(fragment => ownedFragment(fragment, scope.contractOwners))
      : contracts;
    const selectedRequirements = isolatesSelectedSources
      ? requirements.filter(fragment => ownedFragment(fragment, scope.requirementOwners))
      : requirements;
    scope.contractText = selectedContracts.map(fragment => fragment.text).join('\n');
    scope.requirementText = selectedRequirements.map(fragment => fragment.text).join('\n');
  }
  return scopes;
}

function normalizeText(text: string): string {
  return text.replace(/\*\*/g, '').replace(/—/g, '-').toLowerCase().replace(/\s+/g, ' ').trim();
}

function promptFor(track: Track, level: string): string | null {
  const dir = join(track.dir, 'prompts');
  if (!existsSync(dir)) return null;
  const file = readdirSync(dir).find(name => name.startsWith(`${level}-`) && name.endsWith('.md'));
  return file === undefined ? null : normalizeText(readFileSync(join(dir, file), 'utf8'));
}

function referencedActors(step: CompiledStep): string[] {
  return [step.from, step.fromActor].filter((actor): actor is string => actor !== undefined);
}

function referencedTestIds(step: CompiledStep): string[] {
  return [step.testid, step.in?.testid].filter((id): id is string => id !== undefined);
}

function main(args: readonly string[]): number {
  const trackArg = optionValue(args, '--track');
  const recipeArg = optionValue(args, '--recipe');
  const availableTracks = listTracks();
  const trackNames = trackArg === null
    ? (availableTracks.length > 0 ? availableTracks : [DEFAULT_TRACK])
    : [trackArg];
  if (recipeArg !== null && trackNames.length !== 1) {
    throw new Error('--recipe requires one --track');
  }

  const knownActions = new Set(ACTION_REGISTRY.ids);
  let problems = 0;
  let unstatedWarnings = 0;
  let staleStatementWarnings = 0;
  const fail = (where: string, message: string): void => {
    console.log(`  ${where}: ${message}`);
    problems += 1;
  };

  for (const name of trackNames) {
    const track = loadTrack(name);
    console.log(`# track: ${name}`);
    const contracts = hooksByLevel(track);
    const recipeScopes = recipeArg === null ? null : recipeScenarioScopes(track, recipeArg);
    for (const file of readdirSync(track.scenarios).filter(candidate => candidate.endsWith('.json'))) {
      const recipeScope = recipeScopes?.get(file);
      if (recipeScopes !== null && recipeScope === undefined) continue;
      const scenarioPath = join(track.scenarios, file);
      let spec;
      try {
        spec = compileScenarioDefinition(readJson(scenarioPath), { source: scenarioPath });
      } catch (error: unknown) {
        fail(file, error instanceof Error ? error.message : String(error));
        continue;
      }
      const level = String(spec.level).padStart(2, '0');
      const hooks = recipeScope === undefined ? (contracts.get(level) ?? null) : null;

      console.log(file);
      const prompt = recipeScope === undefined
        ? promptFor(track, level)
        : normalizeText(recipeScope.requirementText);
      for (const feature of spec.features) {
        const selectedCriteria = recipeScope?.features.get(feature.id);
        if (recipeScope !== undefined && selectedCriteria === undefined) continue;
        const criteria = selectedCriteria === undefined
          ? feature.criteria
          : feature.criteria.filter(criterion => selectedCriteria.has(criterion.id));
        for (const criterion of criteria) {
          if (criterion.statedBy !== undefined) {
            if (recipeScope === undefined && prompt !== null
              && !prompt.includes(normalizeText(criterion.statedBy))) {
              staleStatementWarnings += 1;
              console.log(`  warn F${feature.id} ${criterion.id}: statedBy text is not in the legacy level ${level} prompt`);
            }
          } else if (recipeScope === undefined && criterion.points > 0) {
            unstatedWarnings += 1;
            console.log(`  warn F${feature.id} ${criterion.id}: carries ${criterion.points} point(s) with no statedBy - the requirement may be unstated`);
          }
        }

        const actors = new Set(feature.actors ?? []);
        const steps = [...feature.setup, ...criteria.flatMap(criterion => criterion.steps)];
        const declared = (actor: string): boolean => actors.has(actor)
          || [...actors].some(candidate => actor.startsWith(`${candidate}-`));
        for (const step of steps) {
          const at = `F${feature.id} ${step.do}`;
          if (!knownActions.has(step.do)) fail(at, `unknown step type "${step.do}"`);
          if (step.actor !== undefined && actors.size > 0 && !declared(step.actor)) {
            fail(at, `actor "${step.actor}" is not in the feature's actor list`);
          }
          for (const actor of referencedActors(step)) {
            if (actors.size > 0 && !declared(actor)) {
              fail(at, `actor "${actor}" is not in the feature's actor list`);
            }
          }
          if (hooks !== null) {
            for (const id of referencedTestIds(step)) {
              if (!hooks.has(id)) fail(at, `testid "${id}" is not in the contract`);
            }
          }
          if (recipeScope !== undefined) {
            for (const id of referencedTestIds(step)) {
              if (!recipeScope.contractText.includes(`\`${id}\``)) {
                fail(at, `testid "${id}" is not in the selected recipe contracts`);
              }
            }
          }
        }

        const points = criteria.reduce((total, criterion) => total + criterion.points, 0);
        if (recipeScope === undefined && feature.max !== undefined && points !== feature.max) {
          fail(`F${feature.id}`, `criteria total ${points} but max says ${feature.max}`);
        }
      }
    }
  }

  const warnings = unstatedWarnings + staleStatementWarnings;
  console.log(problems > 0
    ? `\n${problems} error(s); ${warnings} warning(s)`
    : warnings > 0
      ? `\n0 errors; ${warnings} warning(s) (${unstatedWarnings} point-carrying criteria lack statedBy; ${staleStatementWarnings} statedBy references are outside legacy prompts)`
      : '\n0 errors; 0 warnings');
  return problems > 0 ? 1 : 0;
}

process.exitCode = main(process.argv.slice(2));
