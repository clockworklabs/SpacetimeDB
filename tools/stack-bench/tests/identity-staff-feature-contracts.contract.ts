import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition, resolveTaskFragment, type CompiledPackDefinition }
  from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition, type CompiledCriterion, type CompiledFeature }
  from '../src/composition/definition-compiler.js';
import type { CompiledProgressionNode } from '../src/progression/progression-definition.js';
import { loadValidatedProgressionSource } from './helpers/progression-source.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const readPack = (name: string): CompiledPackDefinition =>
  compilePackDefinition(readJson(join(packRoot, name)), { source: name });

const packs = {
  accounts: readPack('feature-accounts-1.2.0.json'),
  profile: readPack('progression-customer-profile-1.0.0.json'),
  access: readPack('progression-staff-access-1.0.0.json'),
  roles: readPack('progression-staff-roles-1.0.0.json'),
};

function fragmentText(
  fragment: CompiledPackDefinition['task']['requirements'][number],
): string {
  return resolveTaskFragment(fragment, { trackRoot, source: fragment.id }).text;
}

function selectedCriteria(pack: CompiledPackDefinition): Array<{
  feature: CompiledFeature;
  criterion: CompiledCriterion;
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
      return { feature, criterion };
    });
  });
}

test('identity and staff feature packs have exact dependencies and isolated prompt text', () => {
  const accountRequirement = requiredRequirement(packs.accounts);
  const accountContract = requiredContract(packs.accounts);
  const profileRequirement = requiredRequirement(packs.profile);
  const profileContract = requiredContract(packs.profile);
  const accessRequirement = requiredRequirement(packs.access);
  const rolesRequirement = requiredRequirement(packs.roles);
  const rolesContract = requiredContract(packs.roles);
  assert.deepEqual(packs.accounts.requiresPacks, []);
  assert.deepEqual(accountRequirement.modes, ['fresh', 'upgrade']);
  assert.deepEqual(accountContract.modes, ['fresh', 'upgrade']);
  assert.deepEqual(packs.profile.requiresPacks, ['ecommerce.feature.accounts@1.2.0']);
  assert.deepEqual(packs.access.requiresPacks, []);
  assert.deepEqual(packs.roles.requiresPacks, ['ecommerce.progression.staff-access@1.0.0']);

  assert.doesNotMatch(fragmentText(accountRequirement), /endpoint|reducer|SpacetimeDB|Catalog|Staff/i);
  assert.match(fragmentText(accountContract), /POST \/api\/auth\/signup/);
  assert.doesNotMatch(fragmentText(profileRequirement), /Staff roles/);
  assert.doesNotMatch(fragmentText(accessRequirement), /Support intake/);
  assert.doesNotMatch(fragmentText(rolesRequirement), /Catalog management/);

  assert.equal(profileRequirement.path,
    'prompts/modular/customer-profile-1.0.0.md');
  assert.equal(profileContract.path, 'contracts/customer-profile-1.0.md');
  assert.equal(rolesRequirement.path, 'prompts/modular/staff-roles-1.0.0.md');
  assert.equal(rolesContract.path, 'contracts/staff-roles-1.0.md');
  for (const pack of [packs.profile, packs.roles]) {
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      assert.equal(fragment.from, undefined);
      assert.equal(fragment.until, undefined);
    }
    assert.doesNotMatch(fragmentText(requiredRequirement(pack)),
      /framework|ORM|database|MongoDB|PostgreSQL|SpacetimeDB|endpoint|reducer/i);
  }
});

test('each feature pack owns exact, bounded criteria without shared setup failures', () => {
  assert.deepEqual(packs.accounts.checks.map(check => check.criteria),
    [['1a'], ['1b'], ['1c'], ['1d']]);
  assert.deepEqual(requiredCheck(packs.profile).criteria, ['620a', '620b']);
  assert.deepEqual(requiredCheck(packs.access).criteria, ['601a', '601b']);
  assert.deepEqual(requiredCheck(packs.roles).criteria, ['621a', '621b']);

  for (const pack of Object.values(packs)) {
    const selected = selectedCriteria(pack);
    assert.equal(selected.reduce((total, item) => total + item.criterion.points, 0), 4);
    if (pack !== packs.accounts) {
      assert(selected.every(item => item.feature.setup.length === 0),
        `${pack.id} must not share fallible setup across criteria`);
    }
  }
});

test('staff access is self-contained and staff-role checks inherit its interface', () => {
  const accessContract = fragmentText(requiredContract(packs.access));
  for (const hook of [
    'staff-signin-username', 'staff-signin-password', 'staff-signin-submit',
    'staff-current-user', 'staff-link',
  ]) assert.match(accessContract, new RegExp(`\\b${hook}\\b`));
  assert.match(accessContract, /stackbench-staff-2026/);
  assert.match(accessContract, /stackbench-customer-2026/);

  const access = selectedCriteria(packs.access).map(item => item.criterion);
  assert(access.every(criterion => criterion.steps.some(step =>
    step.testid === 'staff-current-user')));
  assert(access.every(criterion => !criterion.steps.some(step => step.do === 'signIn'
    || step.do === 'signUp')));

  const roles = selectedCriteria(packs.roles).map(item => item.criterion);
  const adminCriterion = roles.find(criterion => criterion.id === '621a');
  const replayCriterion = roles.find(criterion => criterion.id === '621b');
  assert(adminCriterion, 'staff roles must include criterion 621a');
  assert(replayCriterion, 'staff roles must include criterion 621b');
  const adminStep = adminCriterion.steps[0];
  const replayStep = replayCriterion.steps[0];
  assert(adminStep && replayStep, 'staff-role criteria must contain steps');
  assert.equal(adminStep.actor, 'admin');
  assert.equal(replayStep.actor, 'replayAdmin');
  const replay = replayCriterion.steps
    .find(step => step.do === 'replayAs');
  assert(replay, 'criterion 621b must replay an administrator session');
  assert.equal(replay.from, 'replayAdmin');
});

test('profile and staff roles use focused scenarios and dedicated interfaces', () => {
  const profileCheck = requiredCheck(packs.profile);
  const rolesCheck = requiredCheck(packs.roles);
  assert.equal(profileCheck.source,
    'scenarios/progression-customer-profile-1.0.0.json');
  assert.equal(rolesCheck.source,
    'scenarios/progression-staff-roles-1.0.0.json');
  for (const pack of [packs.profile, packs.roles]) {
    const check = requiredCheck(pack);
    const scenario = compileScenarioDefinition(
      readJson(join(trackRoot, check.source)), { source: check.source });
    assert.deepEqual(scenario.features.map(feature => feature.id), [check.feature]);
  }
  const profile = selectedCriteria(packs.profile).map(item => item.criterion);
  const ownerCriterion = profile[0];
  const privateOwnerCriterion = profile[1];
  assert(ownerCriterion && privateOwnerCriterion, 'customer profile must have two criteria');
  const ownerStep = ownerCriterion.steps[0];
  const privateOwnerStep = privateOwnerCriterion.steps[0];
  assert(ownerStep && privateOwnerStep, 'customer-profile criteria must contain steps');
  assert.equal(ownerStep.actor, 'owner');
  assert.equal(privateOwnerStep.actor, 'privateOwner');

  const profileContract = fragmentText(requiredContract(packs.profile));
  for (const hook of ['profile-link', 'profile-name', 'profile-address', 'profile-save',
    'profile-address-summary']) assert.match(profileContract, new RegExp(`\\b${hook}\\b`));

  const roleContract = fragmentText(requiredContract(packs.roles));
  for (const hook of ['staff-role-row', 'staff-role-select', 'staff-role-save']) {
    assert.match(roleContract, new RegExp(`\\b${hook}\\b`));
  }

  const { definition, gradingGroups } = loadValidatedProgressionSource(
    join(trackRoot, 'progression', 'ecommerce-2.0.1.json'), trackRoot);
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
  const profileNode = requiredNode(nodes, 'customer-profile');
  const rolesNode = requiredNode(nodes, 'staff-roles');
  assert.deepEqual(profileNode.dependencies, ['accounts']);
  assert.deepEqual(rolesNode.dependencies, ['staff-access']);
  assert.deepEqual(gradingGroups(profileNode.id),
    ['ecommerce.progression.customer-profile@1.0.0#profile']);
  assert.deepEqual(gradingGroups(rolesNode.id),
    ['ecommerce.progression.staff-roles@1.0.0#roles']);
});

function requiredNode(
  nodes: ReadonlyMap<string, CompiledProgressionNode>,
  nodeId: string,
): CompiledProgressionNode {
  const node = nodes.get(nodeId);
  if (!node) throw new Error(`progression node ${nodeId} is required`);
  return node;
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

function requiredCheck(
  pack: CompiledPackDefinition,
): CompiledPackDefinition['checks'][number] {
  const check = pack.checks[0];
  if (!check) throw new Error(`${pack.id} must have a check`);
  return check;
}
