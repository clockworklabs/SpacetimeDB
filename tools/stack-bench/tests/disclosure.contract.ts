import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { loadTrack } from '../src/composition/tracks.js';
import { resolveRecipeRelease } from '../src/composition/recipe-release.js';
import type { CompiledStep } from '../src/composition/definition-compiler.js';

// A scored check may require a name to exist in the application only when
// the request carries that name: a control id or attribute from a contract,
// a route or reducer from a contract or the track's actions, a table from an
// interface the contract names. What a dependency-mode request carries is
// the feature prompts, the contracts, the fixture, and the track actions;
// a specification's text arrives only when a catalog node lists it in
// `promptModules`.
const reference = join(STACK_BENCH_ROOT, 'appliance', 'campaign.ecommerce-progression-reference.json');
const STOCK_TABLES = ['item', 'warehouse', 'stock'];

const placeholder = (route: string): string => route.replace(/\{[a-zA-Z]+\}|:[a-zA-Z]+/g, '{}');

function* eachStep(steps: readonly CompiledStep[]): Generator<CompiledStep> {
  for (const step of steps) {
    yield step;
    for (const branch of step.branches ?? []) yield* eachStep(branch);
  }
}

test('every name a scored check requires reaches the coding agent', () => {
  const track = loadTrack('ecommerce');
  const campaign = compileCampaignFile(reference);
  const catalog = campaign.featureCatalog;
  assert.ok(catalog);
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.progression-catalog@2.0.3');
  assert.ok(binding);
  const { plan } = binding;

  const requestedModules = new Set(catalog.definition.nodes.flatMap(node => node.promptModules ?? []));
  const read = (relative: string): string => {
    for (const base of [track.dir, join(track.dir, 'composition')]) {
      const file = join(base, relative);
      if (existsSync(file)) return readFileSync(file, 'utf8');
    }
    throw new Error(`cannot read ${relative}`);
  };
  const isSpecification = (owner: string): boolean => /\.spec\.|specifications/.test(owner);
  let delivered = '';
  for (const fragment of plan.recipe.task.requirements) {
    const withheld = fragment.owners.length > 0 && fragment.owners.every(owner =>
      isSpecification(owner) && !requestedModules.has(owner));
    if (!withheld) delivered += `\n${read(fragment.path)}`;
  }
  for (const fragment of plan.recipe.task.contracts) delivered += `\n${read(fragment.path)}`;
  delivered += `\n${JSON.stringify(plan.fixture)}`;
  const routes = new Set(track.actions.map(action => placeholder(action.path)));
  const reducers = new Set(track.actions.map(action => action.reducer));
  for (const match of delivered.matchAll(/`(?:GET|POST|PUT|PATCH|DELETE) ([^`]+)`/g)) routes.add(placeholder(match[1]!));
  const has = (name: string): boolean => delivered.includes(name);
  const hasRoute = (route: string): boolean => routes.has(placeholder(route)) || has(route);
  const hasReducer = (reducer: string): boolean => reducers.has(reducer) || has(reducer);
  const stockInterface = STOCK_TABLES.every(table => new RegExp(`\`${table}\\(`).test(delivered));

  const steps = new Map<string, CompiledStep[]>();
  for (const execution of plan.execution) for (const group of execution.checkGroups) {
    for (const criterion of group.feature.criteria) {
      steps.set(`${group.packId}|${group.checkGroupId}|${criterion.id}`,
        [...eachStep([...group.feature.setup, ...criterion.steps])]);
    }
  }
  const byKey = new Map(plan.checks.map(check =>
    [check.stableKey, steps.get(`${check.packId}|${check.checkGroupId}|${check.criterionId}`)]));

  const missing: string[] = [];
  let scored = 0;
  for (const node of catalog.definition.nodes) for (const check of node.gradingChecks) {
    const list = byKey.get(check.id);
    assert.ok(list, `${check.id} has scenario steps`);
    scored += 1;
    const needs = new Set<string>();
    for (const step of list) {
      for (const control of [step.testid, step.in?.testid,
        (step.namedTarget as { testid?: string } | undefined)?.testid,
        (step.input as { testid?: string } | undefined)?.testid]) {
        if (control && !has(control)) needs.add(`control ${control}`);
      }
      for (const attribute of [(step.input as { attribute?: string } | undefined)?.attribute,
        (step.namedTarget as { attribute?: string } | undefined)?.attribute]) {
        if (attribute && !has(attribute)) needs.add(`attribute ${attribute}`);
      }
      const named = step.namedAction as { reducer?: string; path?: string } | undefined;
      if (named?.reducer && !hasReducer(named.reducer)) needs.add(`reducer ${named.reducer}`);
      if (named?.path && !hasRoute(named.path)) needs.add(`route ${named.path}`);
      if ((step.do === 'callAction' || step.do === 'callConcurrently') && !named
        && !track.actions.some(action => action.id === step.action)) needs.add(`action ${step.action}`);
      if (step.do === 'dbSetStock' && !stockInterface) needs.add('stock data interface');
    }
    if (needs.size) missing.push(`${node.id} ${check.id}: ${[...needs].join(', ')}`);
  }
  assert.ok(scored > 100, `${scored} scored checks`);
  assert.deepEqual(missing, []);
});
