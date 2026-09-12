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
    return [pack.id, pack];
  }));
const definition = compileProgressionDefinitionFile(
  join(trackRoot, 'progression', 'ecommerce.json'), { trackRoot });

test('shipping accounting is an unprompted production check owned only by fulfilment', () => {
  const pack = packs.get('ecommerce.progression.inventory-conservation-specifications')!;
  const check = pack.checks.find(check => check.id === 'shipping-accounting')!;
  assert.equal(check.role, 'guarantee');
  const stableKey = `${pack.stableId}.${check.stableId}.202e`;
  assert.deepEqual(definition.nodes.filter(node => node.gradingChecks.some(check => check.id === stableKey))
    .map(node => node.id), ['fulfilment-queue']);
  const criterion = scenarioFeature(check, check.id).feature.criteria.find(criterion => criterion.id === '202e')!;
  assert.equal(criterion.category, 'production');
  assert(pack.task.requirements.every(fragment => !fragment.requiresFeatures?.includes('ecommerce.progression.fulfilment-queue')));
});

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
    const at = pack.id;
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
      `${pack.id} ${requirement.path}`);
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
  const [quantity, checkout] = requiredPack('ecommerce.feature.cart').checks.length
    ? selectedCriteria(requiredPack('ecommerce.feature.cart'))
      .concat(selectedCriteria(requiredPack('ecommerce.feature.checkout')))
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
  const access = requiredPack('ecommerce.progression.operations-access-specifications');
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

test('refund accounting proves one persisted effect after a same-staff replay', () => {
  const refund = requiredPack('ecommerce.progression.support-refunds');
  const accounting = selectedCriteria(refund).find(criterion => criterion.id === '615b');
  assert(accounting);
  const replay = accounting.steps.findIndex(step => step.do === 'replayAs');
  assert(replay >= 0);
  assert.equal(accounting.steps[replay]?.actor, 'staff');
  assert.equal(accounting.steps[replay]?.from, 'staff');
  assert.equal(accounting.steps[replay + 1]?.do, 'expectReplayCompleted');
  assert.equal(accounting.steps.some(step => step.do === 'expectReplayRejected'), false);
  const fresh = accounting.steps.findIndex(step => step.do === 'freshClient');
  assert(fresh > replay);
  const observations = accounting.steps.slice(fresh + 1);
  assert(observations.some(step => step.do === 'signIn' && step.actor === 'owner-fresh'));
  for (const testid of ['support-refund-total', 'order-refund-total']) {
    assert(observations.some(step => step.do === 'expectNumber' && step.actor === 'owner-fresh'
      && step.testid === testid && step.relativeTo === 'paid-total' && step.plus === 0));
  }
  assert(observations.some(step => step.do === 'expectElementCount'
    && step.testid === 'refund-entry' && step.equals === 1));
  assert(observations.some(step => step.do === 'expectNumber'
    && step.testid === 'order-refund-total' && step.equals === 0
    && step.in?.contains === 'Mouse'));
  assert(observations.some(step => step.do === 'expectElementCount'
    && step.testid === 'refund-entry' && step.contains === 'Mouse' && step.equals === 0));
  const access = selectedCriteria(refund).find(criterion => criterion.id === '615c');
  assert(access?.steps.some(step => step.do === 'expectActionOutcome'
    && step.outcome === 'refused'));
});

test('review access is verified at the server boundary', () => {
  const review = requiredPack('ecommerce.progression.review-access-specifications');
  const [reviewCriterion] = selectedCriteria(review);
  assert(reviewCriterion);
  assert(reviewCriterion.steps.some(step => step.do === 'callAction' && step.actor === 'owner'));
  assert(reviewCriterion.steps.some(step => step.do === 'expectActionOutcome' && step.outcome === 'accepted'));
  assert(reviewCriterion.steps.some(step => step.do === 'callAction' && step.actor === 'stranger'));
  assert(reviewCriterion.steps.some(step => step.do === 'expectActionOutcome'
    && step.outcome === 'application-refused' && step.routeProvenBy === 'owner'));
});

test('promotion rules use values a date input accepts', () => {
  const [criterion] = selectedCriteria(requiredPack('ecommerce.progression.promotion-rules'));
  assert(criterion);
  const values = criterion.steps
    .filter(step => step.do === 'fill' && typeof step.testid === 'string'
      && ['promotion-start', 'promotion-end'].includes(step.testid))
    .map(step => step.text);
  assert.equal(values.length, 2);
  for (const value of values) assert.match(String(value), /^\d{4}-\d{2}-\d{2}$/);
});

test('recommendations use their declared catalog entry before reading results', () => {
  const pack = requiredPack('ecommerce.l2.recommendations');
  const text = pack.task.contracts.map(fragmentText).join('\n');
  assert.match(text, /`catalog-link`.*catalog.*list visible/);
  const [criterion] = selectedCriteria(pack);
  assert(criterion);
  const firstResult = criterion.steps.findIndex(step => step.do === 'expect'
    && step.testid === 'recommended-item');
  assert(firstResult > 0);
  assert.equal(criterion.steps[firstResult - 1]?.testid, 'catalog-link');
});

test('the sequential L2 recipe runs every source its operations feature packs own', () => {
  const recipe = compileRecipeFile(
    join(trackRoot, 'composition', 'recipes', 'sequential-l2.json'), { trackRoot });
  const sources = new Set(recipe.execution.map(entry => entry.source));
  for (const name of ['operations-access-features.json',
    'inventory-operations-features.json', 'returns-pricing-features.json']) {
    const pack = compilePackDefinition(readJson(join(packRoot, name)), { source: name });
    assert.equal(pack.moduleType, 'feature');
    for (const check of pack.checks) {
      assert(sources.has(check.source), `${name} must run ${check.source}`);
    }
  }
});

// Each check has authored reporting metadata; browser transport is not a UI category.
test('every current progression check has a category independent of its operational role', () => {
  const checks = definition.nodes.flatMap(node => node.gradingChecks);
  assert.ok(checks.length);
  assert.ok(checks.every(check => ['feature', 'production', 'interface'].includes(check.category ?? '')));
  assert.ok(checks.some(check => check.role === 'feature' && check.category === 'production'));
});
