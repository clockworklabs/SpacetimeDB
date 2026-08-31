import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition, type CompiledPackDefinition }
  from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition, type CompiledFeature }
  from '../src/composition/definition-compiler.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const readPack = (name: string) => compilePackDefinition(
  readJson(join(trackRoot, 'composition', 'packs', name)), { source: name });

function fragmentText(fragment: CompiledPackDefinition['task']['requirements'][number]): string {
  assert.equal(fragment.from, undefined, `${fragment.id} must use a whole file`);
  assert.equal(fragment.until, undefined, `${fragment.id} must use a whole file`);
  return readFileSync(join(trackRoot, fragment.path), 'utf8');
}

function selectedCriteria(pack: CompiledPackDefinition): Array<{
  check: CompiledPackDefinition['checks'][number];
  feature: CompiledFeature;
  criterion: CompiledFeature['criteria'][number];
}> {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
    });
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    assert(feature, `${pack.id}.${check.id} must select a feature`);
    if (!check.criteria) throw new Error(`${pack.id}.${check.id} must select criteria`);
    return check.criteria.map(id => {
      const criterion = feature.criteria.find(candidate => candidate.id === id);
      assert(criterion, `${pack.id}.${check.id} must select ${id}`);
      return { check, feature, criterion };
    });
  });
}

test('the dependency catalog has exact whole-file instructions and focused checks', () => {
  const items = readPack('feature-catalog-items-1.0.0.json');
  const discovery = readPack('feature-catalog-discovery-1.0.0.json');
  assert.deepEqual(items.requiresPacks, []);
  assert.deepEqual(discovery.requiresPacks, ['ecommerce.feature.catalog-items@1.0.0']);
  const itemPrompt = fragmentText(requiredFragment(items.task.requirements[0], 'the item prompt'));
  const discoveryPrompt = fragmentText(requiredFragment(discovery.task.requirements[0],
    'the discovery prompt'));
  assert.match(itemPrompt, /name,[\s\S]*price, and total stock/i);
  assert.match(discoveryPrompt, /ten most-purchased items/i);
  assert.match(discoveryPrompt, /without regard to case/i);
  assert.deepEqual(items.checks.map(check => [check.id, check.criteria]), [['values', ['2a']]]);
  assert.deepEqual(discovery.checks.map(check => [check.id, check.criteria]), [
    ['ranking', ['2b']], ['search', ['2d']],
  ]);
  const selected = [...selectedCriteria(items), ...selectedCriteria(discovery)];
  assert.equal(selected.reduce((total, item) => total + item.criterion.points, 0), 3);
  assert(selected.every(item => item.feature.criteria.length === 1));
  assert(selected.every(item => item.feature.setup.length === 0));
});

test('faceted search covers every filter and stable pagination without prompt leakage', () => {
  const pack = readPack('progression-faceted-search-1.0.1.json');
  assert.deepEqual(pack.requiresPacks, ['ecommerce.feature.catalog-discovery@1.0.0']);
  assert.deepEqual(pack.capabilities, ['browser', 'direct-database-write']);
  const prompt = fragmentText(requiredFragment(pack.task.requirements[0], 'the task prompt'));
  const contract = fragmentText(requiredFragment(pack.task.contracts[0], 'the task contract'));
  for (const text of ['category', 'minimum price', 'maximum price', 'availability',
    'six results per page']) assert.match(prompt, new RegExp(text, 'i'));
  for (const hook of ['category-filter', 'minimum-price', 'maximum-price', 'in-stock-filter',
    'search-results', 'item-card', 'search-next-page', 'search-previous-page']) {
    assert.match(contract, new RegExp(`\`${hook}\``));
  }
  assert.doesNotMatch(`${prompt}\n${contract}`, /framework|ORM|database|websocket/i);
  assert.deepEqual(pack.checks.map(check => [check.id, check.criteria]), [
    ['filters', ['401a']], ['pagination', ['402a']],
  ]);
  const selected = selectedCriteria(pack);
  assert.equal(selected.reduce((total, item) => total + item.criterion.points, 0), 6);
  assert(selected.every(item => item.feature.criteria.length === 1));

  const filterSelection = selected.find(item => item.check.id === 'filters');
  assert(filterSelection, 'the filters check must be selected');
  const filters = filterSelection.feature;
  const setupTestIds = filters.setup.map(step => step.testid).filter(Boolean);
  assert.deepEqual(setupTestIds,
    ['category-filter', 'minimum-price', 'maximum-price', 'in-stock-filter']);
  const filterCriterion = filters.criteria[0];
  assert(filterCriterion, 'the filters feature must have a criterion');
  const checks = filterCriterion.steps.filter(step => step.do === 'expect');
  for (const item of ['Coffee Grinder', 'Gaming Mouse', 'Desk Lamp', 'Espresso Machine',
    'Air Purifier']) assert(checks.some(step => step.contains === item));
  const paginationSelection = selected.find(item => item.check.id === 'pagination');
  assert(paginationSelection, 'the pagination check must be selected');
  const paginationCriterion = paginationSelection.feature.criteria[0];
  assert(paginationCriterion, 'the pagination feature must have a criterion');
  assert(paginationCriterion.steps.filter(step => step.do === 'expectSequence')
    .every(step => step.testid === 'item-name'));
});

test('catalog and faceted search graph nodes bind to the expected packs', () => {
  const definition = readJson(join(trackRoot, 'progression', 'ecommerce-2.0.1.json'));
  const catalog = progressionNode(definition, 'catalog');
  const search = progressionNode(definition, 'faceted-search');
  assert.deepEqual(catalog.featureRefs, ['ecommerce.feature.catalog@1.2.0']);
  assert.deepEqual(catalog.gradingGroups, [
    'ecommerce.feature.catalog@1.2.0#values',
    'ecommerce.feature.catalog@1.2.0#ranking',
    'ecommerce.feature.catalog@1.2.0#search',
  ]);
  assert.deepEqual(search.featureRefs, ['ecommerce.progression.faceted-search@1.0.0']);
  assert.deepEqual(search.dependencies.map(dependency => dependency.id), ['catalog']);
});

interface ProgressionNode {
  featureRefs: string[];
  gradingGroups: string[];
  dependencies: Array<{ id: string }>;
}

function progressionNode(definition: unknown, id: string): ProgressionNode {
  if (!isRecord(definition) || !Array.isArray(definition.nodes)) {
    throw new Error('progression definition must have nodes');
  }
  const node = definition.nodes.find(candidate => isRecord(candidate) && candidate.id === id);
  if (!isRecord(node) || !stringArray(node.featureRefs) || !stringArray(node.gradingGroups)
    || !Array.isArray(node.dependencies)
    || !node.dependencies.every(dependency => isRecord(dependency) && typeof dependency.id === 'string')) {
    throw new Error(`progression node ${id} is not valid`);
  }
  return {
    featureRefs: node.featureRefs,
    gradingGroups: node.gradingGroups,
    dependencies: node.dependencies.map(dependency => ({ id: dependency.id })),
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function stringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every(item => typeof item === 'string');
}

function requiredFragment(
  fragment: CompiledPackDefinition['task']['requirements'][number] | undefined,
  label: string,
): CompiledPackDefinition['task']['requirements'][number] {
  if (!fragment) throw new Error(`${label} is required`);
  return fragment;
}
