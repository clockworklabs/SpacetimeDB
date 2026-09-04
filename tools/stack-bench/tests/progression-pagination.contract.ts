import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compileFixtureDefinition, compilePackDefinition }
  from '../src/composition/composition-compiler.js';
import type { PackCheck } from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import type { CompiledStep } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { loadValidatedProgressionSource } from './helpers/progression-source.js';

const root = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));

function selectedChecks(): PackCheck[] {
  const source = loadValidatedProgressionSource(
    join(root, 'progression', 'ecommerce.json'), root);
  const packRoot = join(root, 'composition', 'packs');
  const packs = new Map(readdirSync(packRoot).filter(name => name.endsWith('.json')).map(name => {
    const pack = compilePackDefinition(readJson(join(packRoot, name)), { source: name });
    return [pack.id, pack];
  }));

  return source.definition.nodes.flatMap(node => source.gradingGroups(node.id).flatMap(reference => {
    const [packRef, checkId] = reference.split('#');
    assert(packRef && checkId);
    const pack = packs.get(packRef);
    assert(pack, `${node.id} references missing pack ${packRef}`);
    return pack.checks.filter(check => check.id === checkId);
  }));
}

function actors(step: CompiledStep): string[] {
  return step.actor === undefined ? (step.actors ?? []) : [step.actor];
}

function inspectSteps(steps: CompiledStep[], navigation: Map<string, { search: string; page: number }>,
  secondPageItems: Set<string>, failures: string[], context: string): void {
  for (const step of steps) {
    for (const branch of step.branches ?? []) {
      inspectSteps(branch, new Map(navigation), secondPageItems, failures, context);
    }
    for (const actor of actors(step)) {
      if (step.do === 'fill' && step.testid === 'search-input') {
        navigation.set(actor, { search: String(step.text), page: 1 });
      } else if (step.do === 'click' && step.testid === 'search-next-page') {
        navigation.set(actor, { search: '', page: 2 });
      } else if (step.do === 'reload' || step.do === 'openClient') {
        navigation.delete(actor);
      } else if (step.do === 'freshClient') {
        navigation.delete(`${actor}-fresh`);
      }
    }

    const item = step.in?.testid === 'item-card'
      ? step.in.contains
      : (step.testid === 'item-card' ? step.contains : undefined);
    if (item === undefined || step.absent || !secondPageItems.has(item)) continue;
    for (const actor of actors(step)) {
      const state = navigation.get(actor);
      const foundBySearch = state?.search !== undefined
        && item.toLowerCase().includes(state.search.toLowerCase());
      if (!foundBySearch && state?.page !== 2) failures.push(`${context}: ${actor} targets ${item}`);
    }
  }
}

test('selected scenarios navigate before using products outside the first catalog page', () => {
  const fixture = compileFixtureDefinition(readJson(join(root, 'composition', 'fixtures',
    'operations.json')));
  const secondPageItems = new Set(fixture.items.map(item => item.name).sort().slice(10));
  const failures: string[] = [];

  for (const check of selectedChecks()) {
    const scenario = compileScenarioDefinition(readJson(join(root, check.source)),
      { source: check.source });
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    assert(feature, `${check.source} is missing feature ${check.feature}`);
    const criteria = check.criteria === undefined
      ? feature.criteria
      : feature.criteria.filter(criterion => check.criteria?.includes(criterion.id) === true);
    for (const criterion of criteria) {
      const navigation = new Map<string, { search: string; page: number }>();
      const context = `${check.source}#${feature.id}.${criterion.id}`;
      inspectSteps(feature.setup, navigation, secondPageItems, failures, context);
      inspectSteps(criterion.steps, navigation, secondPageItems, failures, context);
    }
  }

  assert.deepEqual(failures, []);
});
