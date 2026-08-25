import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const readPack = name => compilePackDefinition(readJson(join(packRoot, name)), { source: name });

const packs = {
  accounts: readPack('feature-accounts-1.2.0.json'),
  profile: readPack('progression-customer-profile-1.0.0.json'),
  access: readPack('progression-staff-access-1.0.0.json'),
  roles: readPack('progression-staff-roles-1.0.0.json'),
};

function fragmentText(fragment) {
  const text = readFileSync(join(trackRoot, fragment.path), 'utf8');
  const start = fragment.from ? text.indexOf(fragment.from) : 0;
  const end = fragment.until ? text.indexOf(fragment.until, start + 1) : text.length;
  assert(start >= 0 && end > start, `${fragment.id} must select text`);
  return text.slice(start, end);
}

function selectedCriteria(pack) {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
    });
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    assert(feature, `${pack.id}.${check.id} must select a feature`);
    return check.criteria.map(id => {
      const criterion = feature.criteria.find(candidate => candidate.id === id);
      assert(criterion, `${pack.id}.${check.id} must select ${id}`);
      return { feature, criterion };
    });
  });
}

test('identity and staff feature packs have exact dependencies and isolated prompt text', () => {
  assert(Object.values(packs).every(pack => pack.state === 'draft'));
  assert.deepEqual(packs.accounts.requiresPacks, []);
  assert.deepEqual(packs.accounts.task.requirements[0].modes, ['fresh', 'upgrade']);
  assert.deepEqual(packs.accounts.task.contracts[0].modes, ['fresh', 'upgrade']);
  assert.deepEqual(packs.profile.requiresPacks, ['ecommerce.feature.accounts@1.2.0']);
  assert.deepEqual(packs.access.requiresPacks, []);
  assert.deepEqual(packs.roles.requiresPacks, ['ecommerce.progression.staff-access@1.0.0']);

  const accountRequirement = fragmentText(packs.accounts.task.requirements[0]);
  assert.doesNotMatch(accountRequirement, /endpoint|reducer|SpacetimeDB|Catalog|Staff/i);
  assert.match(fragmentText(packs.accounts.task.contracts[0]), /POST \/api\/auth\/signup/);
  assert.doesNotMatch(fragmentText(packs.profile.task.requirements[0]), /Staff roles/);
  assert.doesNotMatch(fragmentText(packs.access.task.requirements[0]), /Support intake/);
  assert.doesNotMatch(fragmentText(packs.roles.task.requirements[0]), /Catalog management/);

  assert.equal(packs.profile.task.requirements[0].path,
    'prompts/modular/customer-profile-1.0.0.md');
  assert.equal(packs.profile.task.contracts[0].path, 'contracts/customer-profile-1.0.md');
  assert.equal(packs.roles.task.requirements[0].path, 'prompts/modular/staff-roles-1.0.0.md');
  assert.equal(packs.roles.task.contracts[0].path, 'contracts/staff-roles-1.0.md');
  for (const pack of [packs.profile, packs.roles]) {
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      assert.equal(fragment.from, undefined);
      assert.equal(fragment.until, undefined);
    }
    assert.doesNotMatch(fragmentText(pack.task.requirements[0]),
      /framework|ORM|database|MongoDB|PostgreSQL|SpacetimeDB|endpoint|reducer/i);
  }
});

test('each feature pack owns exact, bounded criteria without shared setup failures', () => {
  assert.deepEqual(packs.accounts.checks.map(check => check.criteria),
    [['1a'], ['1b'], ['1c'], ['1d']]);
  assert.deepEqual(packs.profile.checks[0].criteria, ['620a', '620b']);
  assert.deepEqual(packs.access.checks[0].criteria, ['601a', '601b']);
  assert.deepEqual(packs.roles.checks[0].criteria, ['621a', '621b']);

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
  const accessContract = fragmentText(packs.access.task.contracts[0]);
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
  assert(roles.every(criterion => criterion.steps[0].testid === 'staff-signin-username'));
  assert.equal(roles.find(criterion => criterion.id === '621b').steps
    .some(step => step.do === 'replayAs'), true);
});

test('profile and staff roles use focused scenarios and dedicated interfaces', () => {
  assert.equal(packs.profile.checks[0].source,
    'scenarios/progression-customer-profile-1.0.0.json');
  assert.equal(packs.roles.checks[0].source,
    'scenarios/progression-staff-roles-1.0.0.json');
  for (const pack of [packs.profile, packs.roles]) {
    const scenario = compileScenarioDefinition(
      readJson(join(trackRoot, pack.checks[0].source)), { source: pack.checks[0].source });
    assert.deepEqual(scenario.features.map(feature => feature.id), [pack.checks[0].feature]);
  }

  const profileContract = fragmentText(packs.profile.task.contracts[0]);
  for (const hook of ['profile-link', 'profile-name', 'profile-address', 'profile-save',
    'profile-address-summary']) assert.match(profileContract, new RegExp(`\\b${hook}\\b`));

  const roleContract = fragmentText(packs.roles.task.contracts[0]);
  for (const hook of ['staff-role-row', 'staff-role-select', 'staff-role-save']) {
    assert.match(roleContract, new RegExp(`\\b${hook}\\b`));
  }

  const definition = readJson(join(trackRoot, 'progression', 'ecommerce-1.0.0.json'));
  const profileNode = definition.nodes.find(node => node.id === 'customer-profile');
  const rolesNode = definition.nodes.find(node => node.id === 'staff-roles');
  assert.deepEqual(profileNode.dependencies.map(item => item.id), ['accounts']);
  assert.deepEqual(rolesNode.dependencies.map(item => item.id), ['staff-access']);
  assert.deepEqual(profileNode.gradingGroups,
    ['ecommerce.progression.customer-profile@1.0.0#profile']);
  assert.deepEqual(rolesNode.gradingGroups,
    ['ecommerce.progression.staff-roles@1.0.0#roles']);
});
