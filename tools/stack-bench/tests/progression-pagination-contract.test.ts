import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const referenceRoot = join(STACK_BENCH_ROOT, 'reference-apps', 'ecommerce');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));

type Step = { actor?: string; actors?: string[]; do?: string; testid?: string; text?: unknown;
  contains?: string; in?: { testid?: string; contains?: string }; branches?: Step[][] };
type Criterion = { id: string; steps?: Step[] };
type Feature = { id: string | number; criteria: Criterion[]; setup?: Step[] };
type Check = { id: string; feature: string | number; source: string; criteria?: string[] };

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function strings(value: unknown, at: string): string[] {
  if (!Array.isArray(value) || value.some(item => typeof item !== 'string')) throw new Error(`${at} is invalid`);
  return [...value];
}

function steps(value: unknown, at: string): Step[] {
  if (value === undefined) return [];
  if (!Array.isArray(value)) throw new Error(`${at} is invalid`);
  return value.map((item, index) => {
    if (!object(item)) throw new Error(`${at}[${index}] is invalid`);
    const branchValue = item.branches;
    const inValue = item.in;
    if (inValue !== undefined && !object(inValue)) throw new Error(`${at}[${index}].in is invalid`);
    return { ...(typeof item.actor === 'string' ? { actor: item.actor } : {}),
      ...(item.actors === undefined ? {} : { actors: strings(item.actors, `${at}[${index}].actors`) }),
      ...(typeof item.do === 'string' ? { do: item.do } : {}),
      ...(typeof item.testid === 'string' ? { testid: item.testid } : {}), text: item.text,
      ...(typeof item.contains === 'string' ? { contains: item.contains } : {}),
      ...(inValue === undefined ? {} : { in: { ...(typeof inValue.testid === 'string'
        ? { testid: inValue.testid } : {}), ...(typeof inValue.contains === 'string'
        ? { contains: inValue.contains } : {}) } }),
      ...(branchValue === undefined ? {} : { branches: branchSets(branchValue,
        `${at}[${index}].branches`) }) };
  });
}

function branchSets(value: unknown, at: string): Step[][] {
  if (!Array.isArray(value)) throw new Error(`${at} is invalid`);
  return value.map((branch, index) => steps(branch, `${at}[${index}]`));
}

function fixtureItemNames(value: unknown): string[] {
  if (!object(value) || !Array.isArray(value.items)) throw new Error('fixture items are invalid');
  return value.items.map((item, index) => {
    if (!object(item) || typeof item.name !== 'string') throw new Error(`fixture item ${index} is invalid`);
    return item.name;
  });
}

function scenarioFeatures(value: unknown, at: string): Feature[] {
  if (!object(value) || !Array.isArray(value.features)) throw new Error(`${at} features are invalid`);
  return value.features.map((feature, featureIndex) => {
    if (!object(feature) || (typeof feature.id !== 'string' && typeof feature.id !== 'number')
      || !Array.isArray(feature.criteria)) {
      throw new Error(`${at} feature ${featureIndex} is invalid`);
    }
    return { id: feature.id, setup: steps(feature.setup, `${at} feature ${feature.id} setup`),
      criteria: feature.criteria.map((criterion, criterionIndex) => {
        if (!object(criterion) || typeof criterion.id !== 'string') {
          throw new Error(`${at} feature ${feature.id} criterion ${criterionIndex} is invalid`);
        }
        return { id: criterion.id, steps: steps(criterion.steps,
          `${at} feature ${feature.id} criterion ${criterion.id} steps`) };
      }) };
  });
}

function selectedChecks() {
  const definition = readJson(join(trackRoot, 'progression', 'ecommerce-2.0.1.json'));
  if (!object(definition) || !Array.isArray(definition.nodes)) throw new Error('progression nodes are invalid');
  const packRoot = join(trackRoot, 'composition', 'packs');
  const packs = new Map<string, Check[]>();
  for (const name of readdirSync(packRoot).filter(name => name.endsWith('.json'))) {
      const pack = readJson(join(packRoot, name));
      if (!object(pack) || typeof pack.id !== 'string' || typeof pack.version !== 'string'
        || !Array.isArray(pack.checks)) throw new Error(`pack ${name} is invalid`);
      const checks = pack.checks.map((check, index) => {
        if (!object(check) || typeof check.id !== 'string'
          || (typeof check.feature !== 'string' && typeof check.feature !== 'number')
          || typeof check.source !== 'string') throw new Error(`pack ${name} check ${index} is invalid`);
        return { id: check.id, feature: check.feature, source: check.source,
          ...(check.criteria === undefined ? {} : { criteria: strings(check.criteria, `pack ${name} criteria`) }) };
      });
      packs.set(`${pack.id}@${pack.version}`, checks);
  }

  return definition.nodes.flatMap((node, nodeIndex) => {
    if (!object(node) || typeof node.id !== 'string') throw new Error(`node ${nodeIndex} is invalid`);
    const groups = strings(node.gradingGroups, `node ${node.id} gradingGroups`);
    return groups.flatMap(reference => {
    const [packRef, checkId] = reference.split('#');
    if (!packRef || !checkId) throw new Error(`node ${node.id} has invalid grading reference ${reference}`);
    const pack = packs.get(packRef);
    assert(pack, `${node.id} references missing pack ${packRef}`);
    return pack.filter(check => check.id === checkId);
  }); });
}

function actorsFor(step: Step): string[] {
  return step.actor === undefined ? (step.actors ?? []) : [step.actor];
}

function inspectSteps(stepsToInspect: Step[], navigation: Map<string, { search: string; page: number }>,
  secondPageItems: Set<string>, failures: string[], context: string): void {
  for (const step of stepsToInspect) {
    for (const branch of step.branches ?? []) {
      inspectSteps(branch, new Map(navigation), secondPageItems, failures, context);
    }

    const actors = actorsFor(step);
    for (const actor of actors) {
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
    if (item === undefined || !secondPageItems.has(item)) continue;
    for (const actor of actors) {
      const state = navigation.get(actor);
      const searchFindsItem = state?.search !== undefined
        && item.toLowerCase().includes(state.search.toLowerCase());
      if (!searchFindsItem && state?.page !== 2) {
        failures.push(`${context}: ${actor} targets ${item} before navigating to it`);
      }
    }
  }
}

test('progression references use the storefront contract page size', () => {
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const source = readFileSync(join(referenceRoot, backend, 'client', 'src', 'App.tsx'), 'utf8');
    assert.match(source, /const CATALOG_PAGE_SIZE = 10;/, backend);
    assert.match(source, /slice\(searchPage \* CATALOG_PAGE_SIZE,/, backend);
    assert.match(source, /\(searchPage \+ 1\) \* CATALOG_PAGE_SIZE/, backend);
    assert.equal(source.match(/data-testid="search-results"/g)?.length, 1, backend);
    assert.doesNotMatch(source, /filteredItems\.slice\(12\)/, backend);
  }
});

test('graph-selected scenarios navigate to products outside the first ten catalog items', () => {
  const fixture = readJson(join(trackRoot, 'composition', 'fixtures', 'operations-1.0.0.json'));
  const secondPageItems = new Set(fixtureItemNames(fixture).sort().slice(10));
  const failures: string[] = [];

  for (const check of selectedChecks()) {
    const scenario = scenarioFeatures(readJson(join(trackRoot, check.source)), check.source);
    const feature = scenario.find(candidate => candidate.id === check.feature);
    assert(feature, `${check.source} is missing feature ${check.feature}`);
    const criteria = check.criteria === undefined
      ? feature.criteria
      : feature.criteria.filter(criterion => check.criteria?.includes(criterion.id) === true);

    for (const criterion of criteria) {
      const navigation = new Map();
      const context = `${check.source}#${feature.id}.${criterion.id}`;
      inspectSteps(feature.setup ?? [], navigation, secondPageItems, failures, context);
      inspectSteps(criterion.steps ?? [], navigation, secondPageItems, failures, context);
    }
  }

  assert.deepEqual(failures, []);
});
