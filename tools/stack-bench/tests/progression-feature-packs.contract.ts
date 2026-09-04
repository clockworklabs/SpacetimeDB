import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition, compileRecipeFile, resolveTaskFragment,
  type CompiledPackDefinition } from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition, type CompiledCriterion, type CompiledFeature }
  from '../src/composition/definition-compiler.js';
import { compileProgressionDefinitionFile, type CompiledProgressionNode }
  from '../src/progression/progression-definition.js';

// Rules every feature pack in the dependency catalog must hold, checked once
// over the whole catalog rather than restated per pack. Exact ids, points,
// paths and hook names are data the compiler already binds; they are not
// asserted here.

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const packs = new Map<string, CompiledPackDefinition>(readdirSync(packRoot)
  .filter(name => name.endsWith('.json')).map(name => {
    const pack = compilePackDefinition(readJson(join(packRoot, name)), { source: name });
    return [`${pack.id}@${pack.version}`, pack];
  }));
const definition = compileProgressionDefinitionFile(
  join(trackRoot, 'progression', 'ecommerce-2.0.3.json'), { trackRoot });

function requiredPack(reference: string): CompiledPackDefinition {
  const pack = packs.get(reference);
  if (!pack) throw new Error(`the catalog references missing pack ${reference}`);
  return pack;
}

const featurePacks: Array<{ node: CompiledProgressionNode; pack: CompiledPackDefinition }> =
  definition.nodes.flatMap(node => node.featureRefs.map(reference =>
    ({ node, pack: requiredPack(reference) })));

type Fragment = CompiledPackDefinition['task']['requirements'][number];

function fragmentText(fragment: Fragment): string {
  return resolveTaskFragment(fragment, { trackRoot, source: fragment.id }).text;
}

function scenarioFeature(check: CompiledPackDefinition['checks'][number],
  at: string): { feature: CompiledFeature } {
  const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)),
    { source: check.source });
  const feature = scenario.features.find(candidate => candidate.id === check.feature);
  assert(feature, `${at} must select a feature that exists in ${check.source}`);
  return { feature };
}

function selectedCriteria(pack: CompiledPackDefinition): CompiledCriterion[] {
  return pack.checks.flatMap(check => {
    const at = `${pack.id}.${check.id}`;
    const { feature } = scenarioFeature(check, at);
    const ids = check.criteria ?? feature.criteria.map(criterion => criterion.id);
    return ids.map(id => {
      const criterion = feature.criteria.find(candidate => candidate.id === id);
      assert(criterion, `${at} must select ${id} from ${check.source}`);
      return criterion;
    });
  });
}

test('every feature pack states one whole product request and one interface it can add to an app', () => {
  const requirementPaths = new Set<string>();
  const contractPaths = new Set<string>();
  for (const { pack } of featurePacks) {
    const at = `${pack.id}@${pack.version}`;
    assert.equal(pack.moduleType, 'feature', at);
    assert.equal(pack.task.requirements.length, 1, `${at} must state one product request`);
    assert.equal(pack.task.contracts.length, 1, `${at} must state one application interface`);
    const [requirement] = pack.task.requirements;
    const [contract] = pack.task.contracts;
    assert(requirement && contract);
    // Dependency mode adds features to an existing app, so every feature
    // must compose as an upgrade, and its interface must travel with it.
    assert(requirement.modes?.includes('upgrade'), `${at} must compose as an upgrade`);
    assert.deepEqual(contract.modes, requirement.modes, `${at} interface modes must match its request`);
    for (const fragment of [requirement, contract]) {
      assert.equal(fragment.from, undefined, `${at} must not slice ${fragment.path}`);
      assert.equal(fragment.until, undefined, `${at} must not slice ${fragment.path}`);
    }
    assert.equal(requirementPaths.has(requirement.path), false,
      `${requirement.path} is shared by two feature packs`);
    assert.equal(contractPaths.has(contract.path), false,
      `${contract.path} is shared by two feature packs`);
    requirementPaths.add(requirement.path);
    contractPaths.add(contract.path);
  }
});

test('feature requests are implementation-neutral and never name the testing interface', () => {
  for (const { pack } of featurePacks) {
    const [requirement] = pack.task.requirements;
    assert(requirement);
    assert.doesNotMatch(fragmentText(requirement),
      /framework|ORM|database|websocket|endpoint|\broutes?\b|reducer|testid|MongoDB|PostgreSQL|SpacetimeDB/i,
      `${pack.id}@${pack.version} ${requirement.path}`);
  }
});

test('every check selects criteria that exist in its scenario', () => {
  for (const { pack } of featurePacks) {
    for (const check of pack.checks) {
      const at = `${pack.id}.${check.id}`;
      const { feature } = scenarioFeature(check, at);
      for (const id of check.criteria ?? []) {
        assert(feature.criteria.some(criterion => criterion.id === id),
          `${at} selects ${id}, which ${check.source} does not define`);
      }
    }
  }
});

test('shopping criteria in one scenario never share a product, so state cannot leak between them', () => {
  const [quantity, checkout] = requiredPack('ecommerce.feature.cart@2.0.0').checks.length
    ? selectedCriteria(requiredPack('ecommerce.feature.cart@2.0.0'))
      .concat(selectedCriteria(requiredPack('ecommerce.feature.checkout@2.0.0')))
    : [];
  assert(quantity && checkout, 'cart and checkout must each select a criterion');
  const product = (criterion: CompiledCriterion): string => {
    const add = criterion.steps.find(step => step.do === 'click' && step.testid === 'add-to-cart');
    assert(add && typeof add.in?.contains === 'string', `${criterion.id} must add a named product`);
    return add.in.contains;
  };
  assert.notEqual(product(quantity), product(checkout));
});

test('fulfilment and cancellation keep separate authorization owners', () => {
  // A cross-feature authorization check is graded by exactly one feature, so
  // a failure has one repair owner.
  const access = requiredPack('ecommerce.progression.operations-access-specifications@1.0.0');
  assert.equal(access.moduleType, 'specification');
  const owners = new Map(access.checks.map(check => [check.id, check.requiresFeatures]));
  assert.deepEqual(owners.get('operator-authorization-direct'),
    ['ecommerce.progression.fulfilment-queue']);
  assert.deepEqual(owners.get('order-owner-direct'), ['ecommerce.l2.order-cancellation-features']);
  const fulfilment = definition.nodes.find(node => node.id === 'fulfilment-queue');
  const cancellation = definition.nodes.find(node => node.id === 'order-cancellation');
  assert(fulfilment && cancellation);
  const owns = (node: CompiledProgressionNode, checkId: string): boolean => {
    const check = access.checks.find(candidate => candidate.id === checkId);
    assert(check, `${access.id} must define ${checkId}`);
    const prefix = `${access.stableId ?? access.id}.${check.stableId ?? check.id}.`;
    return node.gradingChecks.some(graded => graded.id.startsWith(prefix));
  };
  assert(owns(fulfilment, 'operator-authorization-direct'));
  assert(!owns(fulfilment, 'order-owner-direct'));
  assert(owns(cancellation, 'order-owner-direct'));
  assert(!owns(cancellation, 'operator-authorization-direct'));
});

test('every replayed request names a declared actor whose request it replays', () => {
  // A replay captured from an actor the scenario never declared records
  // nothing and degrades to inconclusive, which reads as a pass on its own.
  // Replaying one's own request is legitimate (idempotency); an undeclared
  // source is not.
  const graded = new Set(definition.nodes.flatMap(node => node.gradingChecks.map(check => check.id)));
  let replays = 0;
  for (const pack of packs.values()) {
    for (const check of pack.checks) {
      const prefix = `${pack.stableId ?? pack.id}.${check.stableId ?? check.id}.`;
      if (![...graded].some(id => id.startsWith(prefix))) continue;
      const { feature } = scenarioFeature(check, `${pack.id}.${check.id}`);
      const actors = feature.actors ?? [];
      for (const criterion of feature.criteria) {
        for (const step of criterion.steps.filter(candidate => candidate.do === 'replayAs')) {
          replays += 1;
          assert(typeof step.from === 'string' && actors.includes(step.from),
            `${check.source} ${criterion.id} replays a request from undeclared actor ${String(step.from)}`);
          assert(typeof step.actor === 'string' && actors.includes(step.actor),
            `${check.source} ${criterion.id} replays as undeclared actor ${String(step.actor)}`);
        }
      }
    }
  }
  assert(replays > 0, 'the catalog must grade at least one replayed request');
});

test('privacy checks prove a server or transport boundary, not a hidden control', () => {
  const review = requiredPack('ecommerce.progression.review-access-specifications@1.0.0');
  const [reviewCriterion] = selectedCriteria(review);
  assert(reviewCriterion);
  assert(reviewCriterion.steps.some(step => step.do === 'replayAs'));
  assert(reviewCriterion.steps.some(step => step.do === 'expectReplayRejected'));

  const support = requiredPack('ecommerce.progression.support-privacy-specifications@1.0.0');
  const [supportCriterion] = selectedCriteria(support);
  assert(supportCriterion);
  // A "not received" veto is only meaningful beside a positive control.
  assert.deepEqual(supportCriterion.steps.map(step => step.do),
    ['expectReceived', 'expectNotReceived']);
});

test('promotion rules use values a datetime-local input accepts', () => {
  const [criterion] = selectedCriteria(requiredPack('ecommerce.progression.promotion-rules@1.0.1'));
  assert(criterion);
  const values = criterion.steps
    .filter(step => step.do === 'fill' && typeof step.testid === 'string'
      && ['promotion-start', 'promotion-end'].includes(step.testid))
    .map(step => step.text);
  assert.equal(values.length, 2);
  for (const value of values) assert.match(String(value), /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/);
});

test('the sequential L2 recipe runs every source its operations feature packs own', () => {
  const recipe = compileRecipeFile(
    join(trackRoot, 'composition', 'recipes', 'sequential-l2-1.6.0.json'), { trackRoot });
  const sources = new Set(recipe.execution.map(entry => entry.source));
  for (const name of ['operations-access-features-1.0.0.json',
    'inventory-operations-features-1.2.0.json', 'returns-pricing-features-1.1.0.json']) {
    const pack = compilePackDefinition(readJson(join(packRoot, name)), { source: name });
    assert.equal(pack.moduleType, 'feature');
    for (const check of pack.checks) {
      assert(sources.has(check.source), `${name} must run ${check.source}`);
    }
  }
});
