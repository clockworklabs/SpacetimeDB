#!/usr/bin/env node
// Static check on the scenario files, so a typo costs a second rather than a
// full graded run. Verifies that every step names a `do` the grader implements,
// every testid it touches is a hook the contract actually requires, and every
// actor it names is declared. None of this needs an app.
//
// Usage: node commands/check-scenarios.mjs [--track NAME] [--recipe FILE]

import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadTrack, listTracks, DEFAULT_TRACK } from '../src/composition/tracks.mjs';
import { compileRecipeFile } from '../src/composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));

const trackArg = process.argv.includes('--track')
  ? process.argv[process.argv.indexOf('--track') + 1]
  : null;
const recipeArg = process.argv.includes('--recipe')
  ? process.argv[process.argv.indexOf('--recipe') + 1]
  : null;
const trackNames = trackArg ? [trackArg] : (listTracks().length ? listTracks() : [DEFAULT_TRACK]);
if (recipeArg && trackNames.length !== 1) {
  throw new Error('--recipe requires one --track');
}

const known = new Set(ACTION_REGISTRY.ids);

let problems = 0;
const fail = (where, msg) => { console.log(`  ${where}: ${msg}`); problems++; };

// A track's contracts are keyed by level, one file per level whatever its name.
// Levels are CUMULATIVE — a level 2 scenario legitimately drives level 1's
// hooks, because the app it grades still has them — so each level's set is the
// union of every level up to it, the same way the linter loads them.
function hooksByLevel(track) {
  const perFile = {};
  for (const f of readdirSync(track.contracts).filter(f => /^\d\d-.*\.json$/.test(f))) {
    perFile[f.slice(0, 2)] = JSON.parse(readFileSync(join(track.contracts, f), 'utf8')).hooks.map(h => h.id);
  }
  const byLevel = {};
  for (const lvl of Object.keys(perFile)) {
    byLevel[lvl] = new Set(
      Object.entries(perFile).filter(([l]) => l <= lvl).flatMap(([, ids]) => ids));
  }
  return byLevel;
}

const packId = reference => reference.slice(0, reference.lastIndexOf('@'));

function recipeScenarioScopes(track, recipeFile) {
  const recipeDir = join(track.dir, 'composition', 'recipes');
  const chain = [];
  const seen = new Set();
  let currentFile = recipeFile;
  while (currentFile) {
    if (seen.has(currentFile)) throw new Error(`recipe base cycle at ${currentFile}`);
    seen.add(currentFile);
    const path = join(recipeDir, currentFile);
    chain.push(compileRecipeFile(path, { trackRoot: track.dir }));
    const source = JSON.parse(readFileSync(path, 'utf8'));
    currentFile = source.task?.baseRecipe?.path ?? null;
  }
  const recipe = chain[0];
  const packs = new Map(recipe.packs.map(pack => [pack.id, pack]));
  const isolatedPackScope = recipe.recipe.execution === 'all-selected-sources';
  const contracts = isolatedPackScope
    ? recipe.recipe.task.contracts
    : chain.flatMap(release => release.recipe.task.contracts);
  const requirements = isolatedPackScope
    ? recipe.recipe.task.requirements
    : chain.flatMap(release => release.recipe.task.requirements);
  const scopes = new Map();

  const ownersFor = check => {
    const found = new Set([check.packId, ...(check.requiresFeatures ?? [])]);
    const visit = id => {
      const pack = packs.get(id);
      if (!pack) return;
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
      features: new Map(),
      contractOwners: new Set(),
      requirementOwners: new Set(),
    };
    const criteria = scope.features.get(check.featureId) ?? new Set();
    criteria.add(check.criterionId);
    scope.features.set(check.featureId, criteria);
    for (const owner of ownersFor(check)) {
      scope.contractOwners.add(owner);
      scope.requirementOwners.add(owner);
    }
    scopes.set(source, scope);
  }

  for (const scope of scopes.values()) {
    const owned = (fragment, owners) => fragment.owners.some(owner => owners.has(owner));
    const selectedContracts = isolatedPackScope
      ? contracts.filter(fragment => owned(fragment, scope.contractOwners))
      : contracts;
    const selectedRequirements = isolatedPackScope
      ? requirements.filter(fragment => owned(fragment, scope.requirementOwners))
      : requirements;
    scope.contractText = selectedContracts.map(fragment => fragment.text).join('\n');
    scope.requirementText = selectedRequirements.map(fragment => fragment.text).join('\n');
  }
  return scopes;
}

// `statedBy` predates modular prompt treatments. It remains useful provenance,
// but it is no longer a universal launch invariant: a specification can be
// intentionally evaluated without prompting, and requested treatments
// are assembled from pack-owned prompt fragments rather than only the legacy
// level prompt. Keep stale legacy quotes visible without confusing them with an
// executable scenario error. Pack composition and prompt snapshots validate the
// actual prompted/unprompted treatment bindings.
import { existsSync } from 'node:fs';
const norm = t => t.replace(/\*\*/g, '').replace(/—/g, '-').toLowerCase().replace(/\s+/g, ' ').trim();
function promptFor(track, level) {
  const dir = join(track.dir, 'prompts');
  if (!existsSync(dir)) return null;
  const f = readdirSync(dir).find(f => f.startsWith(level + '-') && f.endsWith('.md'));
  return f ? norm(readFileSync(join(dir, f), 'utf8')) : null;
}

let unstatedWarnings = 0;
let staleStatementWarnings = 0;

for (const name of trackNames) {
 const track = loadTrack(name);
 console.log(`# track: ${name}`);
 const contracts = hooksByLevel(track);
 const recipeScopes = recipeArg ? recipeScenarioScopes(track, recipeArg) : null;
 for (const file of readdirSync(track.scenarios).filter(f => f.endsWith('.json'))) {
  const recipeScope = recipeScopes?.get(file);
  if (recipeScopes && !recipeScope) continue;
  const scenarioPath = join(track.scenarios, file);
  let spec;
  try {
    spec = compileScenarioDefinition(JSON.parse(readFileSync(scenarioPath, 'utf8')),
      { source: scenarioPath });
  } catch (error) {
    fail(file, error.message);
    continue;
  }
  const level = String(spec.level).padStart(2, '0');
  const hooks = recipeScope ? null : contracts[level] ?? null;

  console.log(`${file}`);
  const prompt = recipeScope ? norm(recipeScope.requirementText) : promptFor(track, level);
  for (const f of spec.features ?? []) {
    const selectedCriteria = recipeScope?.features.get(f.id);
    if (recipeScope && !selectedCriteria) continue;
    const criteria = recipeScope
      ? (f.criteria ?? []).filter(criterion => selectedCriteria.has(criterion.id))
      : f.criteria ?? [];
    for (const c of criteria) {
      if (c.statedBy) {
        if (!recipeScope && prompt && !prompt.includes(norm(c.statedBy))) {
          staleStatementWarnings++;
          console.log(`  warn F${f.id} ${c.id}: statedBy text is not in the ${recipeScope ? 'selected recipe' : `legacy level ${level}`} prompt`);
        }
      } else if (!recipeScope && (c.points ?? 0) > 0) {
        unstatedWarnings++;
        console.log(`  warn F${f.id} ${c.id}: carries ${c.points} point(s) with no statedBy - the requirement may be unstated`);
      }
    }
    const actors = new Set(f.actors ?? []);
    const steps = [...(f.setup ?? []), ...criteria.flatMap(c => c.steps ?? [])];
    for (const s of steps) {
      const at = `F${f.id} ${s.do}`;
      if (!known.has(s.do)) fail(at, `unknown step type "${s.do}"`);
      // A step may address a client opened mid-scenario (freshClient), whose
      // name is derived from a declared actor.
      const declared = a => actors.has(a) || [...actors].some(x => a.startsWith(`${x}-`));
      if (s.actor && actors.size && !declared(s.actor)) fail(at, `actor "${s.actor}" is not in the feature's actor list`);
      for (const a of [s.from, s.fromActor]) {
        if (a && actors.size && !declared(a)) fail(at, `actor "${a}" is not in the feature's actor list`);
      }
      if (hooks) {
        for (const id of [s.testid, s.in?.testid]) {
          if (id && !hooks.has(id)) fail(at, `testid "${id}" is not in the contract`);
        }
      }
      if (recipeScope) {
        for (const id of [s.testid, s.in?.testid]) {
          if (id && !recipeScope.contractText.includes(`\`${id}\``)) {
            fail(at, `testid "${id}" is not in the selected recipe contracts`);
          }
        }
      }
    }
    const pts = criteria.reduce((n, c) => n + (c.points ?? 1), 0);
    if (!recipeScope && f.max != null && pts !== f.max) {
      fail(`F${f.id}`, `criteria total ${pts} but max says ${f.max}`);
    }
  }
 }
}

const warnings = unstatedWarnings + staleStatementWarnings;
console.log(problems ? `\n${problems} error(s); ${warnings} warning(s)`
  : warnings
    ? `\n0 errors; ${warnings} warning(s) (${unstatedWarnings} point-carrying criteria lack statedBy; ${staleStatementWarnings} statedBy references are outside legacy prompts)`
    : '\n0 errors; 0 warnings');
process.exit(problems ? 1 : 0);
