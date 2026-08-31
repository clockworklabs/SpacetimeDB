import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition, type CompiledPackDefinition }
  from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { compileProgressionDefinitionFile }
  from '../src/progression/progression-definition.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const readPack = (name: string): CompiledPackDefinition =>
  compilePackDefinition(readJson(join(packRoot, name)), { source: name });

const featurePackNames = [
  'l2-stock-transfers-features-1.0.1.json',
  'l2-order-cancellation-features-1.0.1.json',
  'l3-order-returns-features-1.1.1.json',
  'l2-price-history-features-2.0.1.json',
];

test('the current graph keeps reservation product ownership separate from stable score identity', () => {
  const definition = compileProgressionDefinitionFile(join(trackRoot, 'progression',
    'ecommerce-2.0.1.json'), { trackRoot });
  const reservations = definition.nodes.find(node => node.id === 'reservations');
  assert(reservations);
  assert.deepEqual(reservations.featureRefs, ['ecommerce.l3.reservations-features@2.0.0']);
  const restart = reservations.gradingChecks.find(check =>
    check.id === 'ecommerce.l3.deferred-durability.restart-survival.314a');
  assert(restart);
  assert.deepEqual(restart.requiresFeatures, ['ecommerce.l3.reservations-features']);
});

function fragmentText(
  fragment: CompiledPackDefinition['task']['requirements'][number],
): string {
  const source = readFileSync(join(trackRoot, fragment.path), 'utf8');
  const start = fragment.from ? source.indexOf(fragment.from) : 0;
  const end = fragment.until ? source.indexOf(fragment.until, start + 1) : source.length;
  assert(start >= 0 && end > start, `${fragment.id} must resolve to text`);
  return source.slice(start, end);
}

test('each feature pack owns only its prompt and exact dependencies', () => {
  const packs = new Map(featurePackNames.map(name => {
    const pack = readPack(name);
    assert.equal(pack.moduleType, 'feature');
    return [pack.id, pack];
  }));

  const cancellation = requiredPack(packs, 'ecommerce.l2.order-cancellation-features');
  const returns = requiredPack(packs, 'ecommerce.l3.order-returns-features');
  const pricing = requiredPack(packs, 'ecommerce.l2.price-history-features');
  const transfers = requiredPack(packs, 'ecommerce.l2.stock-transfers-features');

  assert.deepEqual(cancellation.requiresPacks,
    ['ecommerce.feature.purchasing@1.2.1', 'ecommerce.feature.warehouse-admin@1.2.1']);
  assert.deepEqual(returns.requiresPacks,
    ['ecommerce.l3.order-delivery-features@1.1.1', 'ecommerce.feature.warehouse-admin@1.2.1']);
  assert.deepEqual(pricing.requiresPacks,
    ['ecommerce.progression.catalog-management@1.0.2']);
  assert.deepEqual(transfers.requiresPacks, ['ecommerce.feature.warehouse-admin@1.2.1']);

  assert.doesNotMatch(fragmentText(requiredRequirement(cancellation)), /return|price|Live operational views/i);
  assert.doesNotMatch(fragmentText(requiredRequirement(returns)), /cancel|Live operational views/i);
  assert.doesNotMatch(fragmentText(requiredRequirement(pricing)), /Cancelling and returning|Live operational views/);
  assert.doesNotMatch(fragmentText(requiredRequirement(transfers)), /Cancelling and returning|Live operational views/);
});

test('dependency-owned checks use only interfaces supplied by their parents', () => {
  const warehouse = readPack('feature-warehouse-admin-1.2.1.json');
  assert.deepEqual(warehouse.requiresPacks,
    ['ecommerce.feature.catalog-items@1.0.0', 'ecommerce.progression.staff-access@1.0.0']);
  assert.deepEqual(warehouse.checks.map(check => [check.id, check.criteria]), [
    ['access-boundary', ['7a']], ['warehouse-view', ['7b']], ['warehouse-stock', ['7c']],
    ['admin-write', ['103a']],
  ]);
  for (const check of warehouse.checks) {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
    });
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    assert(feature, `${warehouse.id}.${check.id} must select a feature`);
    const setup = feature.setup;
    assert(setup.some(step => step.testid === 'staff-signin-submit'));
    assert.equal(setup.some(step => step.do === 'signIn' || step.do === 'signUp'), false);
  }

  const cancellation = readPack('l2-order-cancellation-features-1.0.1.json');
  const cancellationCheck = cancellation.checks.find(check => check.id === 'cancellation-core');
  assert(cancellationCheck, 'the cancellation pack must own cancellation-core');
  assert(cancellationCheck.source
    .endsWith('02-order-cancellation-core-1.0.0.json'));
  const cancellationScenario = compileScenarioDefinition(
    readJson(join(trackRoot, cancellationCheck.source)), { source: cancellationCheck.source });
  assert.equal(JSON.stringify(cancellationScenario).includes('queue-item'), false);

  const queue = readPack('progression-cancellation-queue-specifications-1.0.0.json');
  assert.deepEqual(queue.requiresPacks, []);
  assert.deepEqual(requiredRequirement(queue).requiresFeatures,
    ['ecommerce.l2.order-cancellation-features', 'ecommerce.progression.fulfilment-queue']);
  assert.deepEqual(queue.checks.map(check => [check.id, check.criteria]), [
    ['queue-removal', ['3d']],
  ]);

  const activity = readPack('progression-staff-activity-1.0.2.json');
  const activityCheck = activity.checks[0];
  assert(activityCheck, 'the staff activity pack must have a check');
  const staffScenario = compileScenarioDefinition(readJson(join(trackRoot, activityCheck.source)), {
    source: activityCheck.source,
  });
  const activityFeature = staffScenario.features.find(feature => feature.id === 624);
  assert(activityFeature, 'the staff activity scenario must include feature 624');
  const activitySetup = activityFeature.setup;
  assert(activitySetup.some(step => step.testid === 'catalog-save'));
  assert.equal(activitySetup.some(step => step.testid === 'price-submit'), false);
});

test('catalog and faceted search expose only their own product work', () => {
  const catalog = readPack('feature-catalog-items-1.0.0.json');
  const search = readPack('progression-faceted-search-1.0.1.json');
  const management = readPack('progression-catalog-management-1.0.2.json');
  assert.deepEqual(catalog.requiresPacks, []);
  assert.deepEqual(search.requiresPacks, ['ecommerce.feature.catalog-discovery@1.0.0']);
  assert.deepEqual(management.requiresPacks,
    ['ecommerce.feature.catalog-discovery@1.0.0', 'ecommerce.progression.staff-roles@1.0.0']);
  assert.equal(management.checks[0]?.source,
    'scenarios/progression-catalog-management-1.0.1.json');
  const managementScenario = compileScenarioDefinition(
    readJson(join(trackRoot, management.checks[0]!.source)), { source: management.checks[0]!.source });
  assert.deepEqual(managementScenario.features.map(feature => feature.id), [622]);
  for (const fragment of [...catalog.task.requirements, ...catalog.task.contracts]) {
    assert.deepEqual(fragment.modes, ['fresh', 'upgrade']);
  }
  assert.deepEqual(catalog.checks.map(check => [check.id, check.criteria]), [
    ['values', ['2a']],
  ]);
  const catalogPrompt = fragmentText(requiredRequirement(catalog));
  assert.match(catalogPrompt, /price.*total stock/i);
  assert.doesNotMatch(catalogPrompt, /review|rating|warehouse|framework|database/i);
  const searchPrompt = fragmentText(requiredRequirement(search));
  assert.match(searchPrompt, /category.*minimum price.*maximum price.*availability/is);
  assert.doesNotMatch(searchPrompt, /warehouse|admin|framework|database/i);
});

test('support nodes have isolated prompts, hooks, scenarios, and exact dependencies', () => {
  const intake = readPack('progression-support-intake-1.0.0.json');
  const triage = readPack('progression-support-triage-1.0.0.json');
  const history = readPack('progression-support-history-1.0.0.json');
  assert.deepEqual(intake.requiresPacks, []);
  for (const fragment of [...intake.task.requirements, ...intake.task.contracts]) {
    assert.deepEqual(fragment.modes, ['fresh', 'upgrade']);
  }
  assert.deepEqual(triage.requiresPacks,
    ['ecommerce.progression.staff-access@1.0.0',
      'ecommerce.progression.support-intake@1.0.0']);
  assert.deepEqual(history.requiresPacks,
    ['ecommerce.feature.accounts@1.2.0', 'ecommerce.progression.support-intake@1.0.0']);

  const intakeText = `${fragmentText(requiredRequirement(intake))}\n${fragmentText(requiredContract(intake))}`;
  const triageText = `${fragmentText(requiredRequirement(triage))}\n${fragmentText(requiredContract(triage))}`;
  const historyText = `${fragmentText(requiredRequirement(history))}\n${fragmentText(requiredContract(history))}`;
  assert.doesNotMatch(intakeText, /assign|priority|status|reply|order|refund/i);
  assert.doesNotMatch(triageText, /reply|order|refund|customer history/i);
  assert.doesNotMatch(historyText, /assign|priority|reply|order|refund/i);
  assert.doesNotMatch(`${intakeText}\n${triageText}\n${historyText}`,
    /framework|ORM|database|websocket/i);

  assert.deepEqual(intake.checks.map(check => check.id), ['ticket-create']);
  assert.deepEqual(triage.checks.map(check => [check.id, check.criteria]), [
    ['assignment', ['611a']], ['priority', ['611b']], ['status', ['611c']],
  ]);
  assert.deepEqual(history.checks.map(check => [check.id, check.criteria]), [
    ['persistence', ['612a']], ['privacy', ['612b']],
  ]);
  for (const [pack, expectedLevel] of [[intake, 1], [triage, 2], [history, 2]] as const) {
    for (const check of pack.checks) {
      const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
        source: check.source,
        expectedLevel,
      });
      assert.deepEqual(scenario.features.map(feature => feature.id), [check.feature]);
      const feature = scenario.features[0];
      assert(feature, `${check.source} must contain its selected feature`);
      for (const criterion of check.criteria ?? feature.criteria.map(item => item.id)) {
        assert(feature.criteria.some(item => item.id === criterion),
          `${pack.id} must own ${criterion}`);
      }
    }
  }
});

function requiredPack(
  packs: ReadonlyMap<string, CompiledPackDefinition>,
  packId: string,
): CompiledPackDefinition {
  const pack = packs.get(packId);
  if (!pack) throw new Error(`pack ${packId} is required`);
  return pack;
}

function requiredRequirement(
  pack: CompiledPackDefinition,
): CompiledPackDefinition['task']['requirements'][number] {
  const requirement = pack.task.requirements[0];
  if (!requirement) throw new Error(`${pack.id} must have a product requirement`);
  return requirement;
}

function requiredContract(
  pack: CompiledPackDefinition,
): CompiledPackDefinition['task']['contracts'][number] {
  const contract = pack.task.contracts[0];
  if (!contract) throw new Error(`${pack.id} must have a testing contract`);
  return contract;
}
