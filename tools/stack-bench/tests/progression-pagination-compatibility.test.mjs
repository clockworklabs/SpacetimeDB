import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const referenceRoot = join(import.meta.dirname, '..', 'reference-apps', 'ecommerce', 'progression');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));

function selectedChecks() {
  const definition = readJson(join(trackRoot, 'progression', 'ecommerce-1.0.0.json'));
  const packRoot = join(trackRoot, 'composition', 'packs');
  const packs = new Map(readdirSync(packRoot)
    .filter(name => name.endsWith('.json'))
    .map(name => {
      const pack = readJson(join(packRoot, name));
      return [`${pack.id}@${pack.version}`, pack];
    }));

  return definition.nodes.flatMap(node => node.gradingGroups.flatMap(reference => {
    const [packRef, checkId] = reference.split('#');
    const pack = packs.get(packRef);
    assert(pack, `${node.id} references missing pack ${packRef}`);
    return pack.checks.filter(check => check.id === checkId);
  }));
}

function actorsFor(step) {
  return step.actor === undefined ? (step.actors ?? []) : [step.actor];
}

function inspectSteps(steps, navigation, secondPageItems, failures, context) {
  for (const step of steps ?? []) {
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
    if (!secondPageItems.has(item)) continue;
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
  const secondPageItems = new Set(fixture.items.map(item => item.name).sort().slice(10));
  const failures = [];

  for (const check of selectedChecks()) {
    const scenario = readJson(join(trackRoot, check.source));
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    assert(feature, `${check.source} is missing feature ${check.feature}`);
    const criteria = check.criteria === undefined
      ? feature.criteria
      : feature.criteria.filter(criterion => check.criteria.includes(criterion.id));

    for (const criterion of criteria) {
      const navigation = new Map();
      const context = `${check.source}#${feature.id}.${criterion.id}`;
      inspectSteps(feature.setup, navigation, secondPageItems, failures, context);
      inspectSteps(criterion.steps, navigation, secondPageItems, failures, context);
    }
  }

  assert.deepEqual(failures, []);
});
