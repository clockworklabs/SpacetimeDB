import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import test from 'node:test';

import { checkCompositions } from '../check-composition.mjs';
import {
  compileFixtureDefinition,
  compilePackDefinition,
  compilePromotionDefinition,
  compilePromotionFile,
  compileRecipeDefinition,
  compileRecipeFile,
  resolveTaskFragment,
} from '../composition-compiler.mjs';
import { compileScenarioDefinition } from '../definition-compiler.mjs';
import { loadTrack, suitesFor } from '../tracks.mjs';

const ECOMMERCE = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const recipePath = name => join(ECOMMERCE, 'composition', 'recipes', name);

test('the ecommerce composition tree validates as one source set', () => {
  assert.deepEqual(checkCompositions({ trackName: 'ecommerce' }), [{
      track: 'ecommerce', packs: 33, fixtures: 2, recipes: 10, checks: 477, aliases: 5,
  }]);
});

function legacyProjection(level) {
  const track = loadTrack('ecommerce');
  return suitesFor(track, level).map(suite => {
    const source = relative(track.dir, suite.spec).replaceAll('\\', '/');
    const spec = compileScenarioDefinition(JSON.parse(readFileSync(suite.spec, 'utf8')), { source });
    return {
      id: suite.id,
      source,
      features: spec.features.map(feature => ({
        id: feature.id,
        criteria: feature.criteria.map(criterion => ({
          id: criterion.id,
          points: criterion.points ?? 1,
        })),
      })),
    };
  });
}

function recipeProjection(plan) {
  return plan.execution.map(suite => ({
    id: suite.id,
    source: suite.source,
    features: suite.checkGroups.reduce((features, group) => {
      let feature = features.at(-1);
      if (!feature || feature.id !== group.feature.id) {
        feature = { id: group.feature.id, criteria: [] };
        features.push(feature);
      }
      feature.criteria.push(...group.feature.criteria.map(criterion => ({
        id: criterion.id, points: criterion.points ?? 1,
      })));
      return features;
    }, []),
  }));
}

test('ecommerce L1 and L2 recipes preserve current suite, feature, check, order, and score semantics', () => {
  const l1 = compileRecipeFile(recipePath('l1-standard-1.0.0.json'), { trackRoot: ECOMMERCE });
  const l2 = compileRecipeFile(recipePath('l2-standard-1.1.0.json'), { trackRoot: ECOMMERCE });
  assert.deepEqual(recipeProjection(l1), legacyProjection(1));
  assert.deepEqual(recipeProjection(l2), legacyProjection(2));
  assert.deepEqual({ checks: l1.checks.length, points: l1.scoring.points }, { checks: 48, points: 51 });
  assert.deepEqual({ checks: l2.checks.length, points: l2.scoring.points }, { checks: 53, points: 75 });
  const promotions = compilePromotionFile(join(ECOMMERCE, 'composition', 'promotions.json'), {
    trackRoot: ECOMMERCE,
  });
  assert.deepEqual(promotions.entries.map(entry => [entry.alias, entry.status, entry.recipe.id]), [
    ['L1', 'retired', 'ecommerce.l1-standard'],
    ['L1', 'retired', 'ecommerce.l1-standard'],
    ['L1', 'promoted', 'ecommerce.l1-modular'],
    ['L2', 'retired', 'ecommerce.l2-standard'],
    ['L2', 'promoted', 'ecommerce.l2-standard'],
  ]);
});

test('framework-neutral releases change task meaning without changing execution or scoring', () => {
  for (const [oldName, candidateName, level] of [
    ['l1-standard-1.0.0.json', 'l1-standard-1.1.0.json', 1],
    ['l2-standard-1.1.0.json', 'l2-standard-1.2.0.json', 2],
  ]) {
    const oldPlan = compileRecipeFile(recipePath(oldName), { trackRoot: ECOMMERCE });
    const candidate = compileRecipeFile(recipePath(candidateName), { trackRoot: ECOMMERCE });
    assert.deepEqual(recipeProjection(candidate), legacyProjection(level));
    assert.deepEqual(recipeProjection(candidate), recipeProjection(oldPlan));
    assert.deepEqual(candidate.scoring, oldPlan.scoring);
  }
  const l1 = compileRecipeFile(recipePath('l1-standard-1.1.0.json'), { trackRoot: ECOMMERCE });
  assert.doesNotMatch(l1.recipe.task.requirementText, /Express|similar/i);
  assert.match(l1.recipe.task.requirementText, /POST \/api\/auth\/signin/);
  assert.match(l1.recipe.task.requirementText, /POST \/api\/admin\/restock/);
  assert.match(l1.recipe.task.requirementText, /POST \/api\/checkout/);
});

test('the L2 hardening candidate re-proves exact modular L1 checks and adds every L2 check', () => {
  const l1 = compileRecipeFile(recipePath('l1-modular-2.2.0.json'), { trackRoot: ECOMMERCE });
  const oldPlan = compileRecipeFile(recipePath('l2-standard-1.2.0.json'), { trackRoot: ECOMMERCE });
  const candidate = compileRecipeFile(recipePath('l2-standard-1.3.0.json'), { trackRoot: ECOMMERCE });
  const l2Packs = new Set([
    'ecommerce.operations-access',
    'ecommerce.inventory-operations',
    'ecommerce.returns-pricing',
  ]);
  const promoted = new Set([
    'ecommerce.operations-access.operator-authorization.201c',
    'ecommerce.inventory-operations.stock-conservation.202d',
    'ecommerce.operations-access.order-owner.204a',
  ]);
  assert.equal(candidate.recipe.state, 'draft');
  assert.deepEqual(candidate.recipe.task.baseRecipe, {
    id: 'ecommerce.l1-modular', version: '2.2.0', path: 'recipes/l1-modular-2.2.0.json',
  });
  const carried = candidate.checks.filter(check =>
    l1.checks.some(base => base.stableKey === check.stableKey));
  assert.deepEqual(carried, l1.checks,
    'L2 must carry every L1 check with the same stable identity and semantics');
  const l2Checks = candidate.checks.filter(check => l2Packs.has(check.packId));
  const previousL2Checks = oldPlan.checks.filter(check => l2Packs.has(check.packId));
  assert.equal(carried.length, 48);
  assert.equal(l2Checks.length, 28);
  assert.deepEqual(l2Checks.map(check => check.stableKey).sort(),
    previousL2Checks.map(check => check.stableKey).sort());
  for (const check of l2Checks) {
    const previous = oldPlan.checks.find(item => item.stableKey === check.stableKey);
    if (promoted.has(check.stableKey)) {
      assert.equal(previous.points, 0);
      assert.equal(check.points, 2);
      assert.equal(check.source, 'scenarios/02-server-actions-1.0.0.json');
    } else {
      assert.deepEqual({ points: check.points, source: check.source },
        { points: previous.points, source: previous.source });
    }
  }
  assert.deepEqual({ checks: candidate.checks.length, points: candidate.scoring.points,
    packs: candidate.packs.length }, { checks: 76, points: 111, packs: 15 });
  assert.equal(candidate.scoring.points, l1.scoring.points
    + l2Checks.reduce((sum, check) => sum + check.points, 0));
  assert.match(candidate.recipe.task.contractText, /data-ship-input/);
  assert.match(candidate.recipe.task.contractText, /data-cancel-input/);
  assert.match(candidate.recipe.task.contractText, /data-transfer-input/);
  const focused = candidate.execution.find(execution => execution.id === 'server-actions');
  assert.deepEqual(focused.checkGroups.map(group => group.feature.id), [201, 202, 204]);
  assert(focused.checkGroups.flatMap(group => group.feature.criteria)
    .every(criterion => criterion.steps.some(step => step.do === 'callAction'
      || step.do === 'race')));
});

test('full ecommerce recipes compose the exact legacy builder task from pack-owned fragments', () => {
  for (const [recipe, prompt, contract] of [
    ['l1-standard-1.0.0.json', 'prompts/01-storefront.md', 'contracts/appendix-01.md'],
    ['l2-standard-1.1.0.json', 'prompts/02-operations.md', 'contracts/appendix-02.md'],
  ]) {
    const plan = compileRecipeFile(recipePath(recipe), { trackRoot: ECOMMERCE });
    assert.equal(plan.recipe.task.requirementText, readFileSync(join(ECOMMERCE, prompt), 'utf8'));
    assert.equal(plan.recipe.task.contractText, readFileSync(join(ECOMMERCE, contract), 'utf8'));
  }
});

test('removing session durability removes its requirement and checks without changing account access', () => {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-pack-removal-'));
  const root = join(temp, 'ecommerce');
  try {
    cpSync(ECOMMERCE, root, { recursive: true });
    const path = join(root, 'composition', 'recipes', 'l1-standard-1.0.0.json');
    const recipe = JSON.parse(readFileSync(path, 'utf8'));
    recipe.packs = recipe.packs.filter(pack => pack.id !== 'ecommerce.session-durability');
    writeFileSync(path, `${JSON.stringify(recipe, null, 2)}\n`);
    const plan = compileRecipeFile(path, { trackRoot: root });
    assert.equal(plan.checks.length, 45);
    assert(plan.checks.some(check => check.stableKey === 'ecommerce.identity-access.accounts.1a'));
    assert(!plan.checks.some(check => check.packId === 'ecommerce.session-durability'));
    assert.doesNotMatch(plan.recipe.task.requirementText, /A signed-in session/);
    assert.match(plan.recipe.task.requirementText, /A visitor can \*\*create an account\*\*/);
  } finally { rmSync(temp, { recursive: true, force: true }); }
});

test('the smoke recipe reuses two behavior packs without duplicating their definitions', () => {
  const plan = compileRecipeFile(recipePath('smoke-1.0.0.json'), { trackRoot: ECOMMERCE });
  assert.deepEqual(plan.packs.map(pack => pack.id), [
    'ecommerce.identity-access', 'ecommerce.reviews',
  ]);
  assert.deepEqual(plan.execution[0].checkGroups.map(group => group.feature.id), [1, 6]);
  assert.equal(plan.checks.length, 7);
  assert.equal(plan.scoring.points, 8);
  assert.equal(plan.packs[0].budget.status, 'bounded');
  assert(plan.packs[0].actions.includes('signUp'));
  assert.match(plan.recipe.task.requirementText, /### Accounts/);
  assert.match(plan.recipe.task.requirementText, /### Reviews/);
  assert.doesNotMatch(plan.recipe.task.requirementText, /### Cart|### Admin|### Buying/);
});

test('fixture versions make the L2 staff addition explicit', () => {
  const l1 = compileRecipeFile(recipePath('l1-standard-1.0.0.json'), { trackRoot: ECOMMERCE });
  const l2 = compileRecipeFile(recipePath('l2-standard-1.1.0.json'), { trackRoot: ECOMMERCE });
  assert.equal(l1.fixture.items.length, 12);
  assert.deepEqual(l1.fixture.accounts.map(account => account.username), ['admin']);
  assert.deepEqual(l2.fixture.accounts.map(account => account.username), ['admin', 'staff']);
  assert.equal(l1.fixture.items.find(item => item.name === 'Mirrorless Camera').stock.East, 2);
  const prompt = readFileSync(join(ECOMMERCE, 'prompts', '01-storefront.md'), 'utf8');
  const startingData = prompt.slice(prompt.indexOf('### Starting data'));
  const promptItems = startingData.split(/\r?\n/).map(line =>
    line.match(/^\| ([^|]+) \| (\d+\.\d{2}) \| (\d+) \| (\d+) \|$/))
    .filter(Boolean)
    .map(match => ({ name: match[1].trim(), price: match[2],
      stock: { East: Number(match[3]), West: Number(match[4]) } }));
  assert.deepEqual(l1.fixture.items.map(item => ({ name: item.name, price: item.price, stock: item.stock })),
    promptItems);
  assert.match(prompt, /username `admin`, password `stackbench-admin-2026`/);
  assert.match(readFileSync(join(ECOMMERCE, 'prompts', '02-operations.md'), 'utf8'),
    /username `staff`, password `stackbench-staff-2026`/);
});

test('the recommendation criterion owns its purchase prerequisite', () => {
  const l2 = compileRecipeFile(recipePath('l2-standard-1.1.0.json'), { trackRoot: ECOMMERCE });
  const operationalViews = l2.execution
    .flatMap(execution => execution.checkGroups)
    .find(group => group.packId === 'ecommerce.inventory-operations'
      && group.checkGroupId === 'operational-views');
  const recommendation = operationalViews.feature.criteria.find(criterion => criterion.id === '5c');
  const purchase = recommendation.steps.findIndex(step => step.do === 'click'
    && step.testid === 'buy-now'
    && step.in?.testid === 'item-card'
    && step.in?.contains === 'Bluetooth Speaker');
  const assertion = recommendation.steps.findIndex(step => step.do === 'expect'
    && step.testid === 'recommended-item'
    && step.contains === 'Headphones'
    && !step.absent);
  assert(purchase >= 0, '5c must establish the category purchase it observes');
  assert(assertion > purchase, '5c must purchase before observing recommendations');
  assert.equal(l2.recipe.state, 'qualified');
  assert.equal(operationalViews.packVersion, '1.1.0');
});

test('source contracts reject unknown fields, malformed versions, duplicate fixture data, and invalid aliases', () => {
  const pack = {
    schemaVersion: 1, kind: 'test-pack', id: 'example.pack', version: '1.0.0', state: 'draft',
    title: 'Pack', requiresPacks: [], conflictsWith: [], capabilities: ['browser'],
    evidence: ['browser-observation'], budget: { status: 'unmeasured' },
    task: { requirements: [{ id: 'example.requirement', path: 'prompt.md', order: 1 }], contracts: [] },
    checks: [{ id: 'group', source: 'scenarios/01.json', feature: 1, role: 'feature' }],
  };
  assert.throws(() => compilePackDefinition({ ...pack, surprise: true }), /surprise: unknown field/);
  assert.throws(() => compilePackDefinition({ ...pack, version: 'latest' }), /exact semantic version/);
  assert.throws(() => compilePackDefinition({ ...pack, requiresPacks: ['example.other'] }), /id@version/);
  assert.throws(() => compilePackDefinition({ ...pack, state: 'qualified' }),
    /qualified packs require a bounded runtime budget/);
  assert.throws(() => compilePackDefinition({ ...pack, moduleType: 'mode' }),
    /moduleType.*feature or specification/);
  assert.throws(() => compilePackDefinition({ ...pack, moduleType: 'feature', checks: [
    { ...pack.checks[0], role: 'guarantee' },
  ] }), /feature modules cannot own guarantee/);
  assert.throws(() => compilePackDefinition({ ...pack, moduleType: 'specification', task: {
    ...pack.task, requirements: [
      { ...pack.task.requirements[0], requiresFeatures: ['example.feature'] },
    ],
  } }),
    /specification modules cannot own feature/);

  const specification = compilePackDefinition({ ...pack, id: 'example.durability',
    moduleType: 'specification', task: { ...pack.task, requirements: [
      { ...pack.task.requirements[0], requiresFeatures: ['example.feature'] },
    ] }, checks: [{ ...pack.checks[0], role: 'guarantee',
      observations: ['requested', 'unmentioned'], requiresFeatures: ['example.feature'] }] });
  assert.equal(specification.moduleType, 'specification');
  assert.deepEqual(specification.checks[0].observations, ['requested', 'unmentioned']);
  assert.deepEqual(specification.checks[0].requiresFeatures, ['example.feature']);
  assert.deepEqual(specification.task.requirements[0].requiresFeatures, ['example.feature']);

  const fixture = {
    schemaVersion: 1, kind: 'fixture-set', id: 'example.fixture', version: '1.0.0', state: 'draft',
    title: 'Fixture', warehouses: ['East'], items: [
      { name: 'Item', price: '1.00', category: 'Test', stock: { East: 1 } },
    ], accounts: [], empty: [],
  };
  assert.throws(() => compileFixtureDefinition({ ...fixture, warehouses: ['East', 'East'] }), /duplicates/);

  const recipe = {
    schemaVersion: 1, kind: 'benchmark-recipe', id: 'example.recipe', version: '1.0.0', state: 'draft',
    title: 'Recipe', track: 'example',
    fixture: { path: '../fixtures/f.json', id: 'example.fixture', version: '1.0.0' },
    task: { mode: 'fresh', framing: { requirements: [
      { id: 'example.framing', path: 'prompt.md', order: 0 },
    ], contracts: [] } },
    packs: [{ path: '../packs/a.json', id: 'example.a', version: '1.0.0', includeRoles: ['feature'] }],
    execution: [{ id: 'features', source: 'scenarios/01.json' }],
    scoring: { mode: 'legacy-source-points' },
  };
  assert.throws(() => compileRecipeDefinition(recipe), /allowed only for a declared compatibility recipe/);
  assert.throws(() => compileRecipeDefinition({ ...recipe, compatibility: {
    legacyLevel: 1, mode: 'cumulative',
  } }), /cumulative compatibility requires an upgrade recipe/);
  const catalog = {
    schemaVersion: 1, kind: 'promotion-catalog', id: 'example.recipes', version: '1.0.0',
    state: 'draft', title: 'Promotions', entries: [{ alias: 'latest', status: 'candidate',
      recipe: { path: 'recipes/r.json', id: 'example.recipe', version: '1.0.0' } }],
  };
  assert.throws(() => compilePromotionDefinition(catalog), /must look like L1/);
});

test('task fragment markers are contained, unique, ordered, and non-empty', () => {
  const box = sandbox();
  try {
    assert.throws(() => resolveTaskFragment({ id: 'example.missing', path: 'prompts/task.md',
      order: 1, from: 'not present' }, { trackRoot: box.root }), /marker not found/);
    assert.throws(() => resolveTaskFragment({ id: 'example.escape', path: '../outside.md',
      order: 1 }, { trackRoot: box.root }), /escapes/);
    writeFileSync(join(box.root, 'prompts', 'task.md'), 'same\nsame\n');
    assert.throws(() => resolveTaskFragment({ id: 'example.ambiguous', path: 'prompts/task.md',
      order: 1, from: 'same' }, { trackRoot: box.root }), /marker is not unique/);
    assert.throws(() => compilePackDefinition({ ...box.makePack('bad'), task: {
      requirements: [{ id: 'example.bad', path: 'prompts/task.md', order: -1 }], contracts: [],
    } }), /non-negative integer/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

function sandbox() {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-composition-'));
  const root = join(temp, 'example');
  for (const directory of [
    'composition/recipes', 'composition/packs', 'composition/fixtures', 'scenarios', 'prompts', 'contracts',
  ]) mkdirSync(join(root, directory), { recursive: true });
  writeFileSync(join(root, 'prompts', 'task.md'), 'Build it.');
  writeFileSync(join(root, 'contracts', 'contract.json'), '{}');
  writeFileSync(join(root, 'scenarios', '01.json'), JSON.stringify({
    level: 1,
    features: [{ id: 1, name: 'Feature', actors: ['a'], setup: [], criteria: [
      { id: '1a', desc: 'Works', points: 2, steps: [{ do: 'wait', actor: 'a', ms: 1 }] },
    ] }],
  }));
  const fixture = {
    schemaVersion: 1, kind: 'fixture-set', id: 'example.fixture', version: '1.0.0', state: 'draft',
    title: 'Fixture', warehouses: ['East'], items: [
      { name: 'Item', price: '1.00', category: 'Test', stock: { East: 1 } },
    ], accounts: [], empty: [],
  };
  writeFileSync(join(root, 'composition', 'fixtures', 'fixture.json'), JSON.stringify(fixture));
  const makePack = (name, extra = {}) => ({
    schemaVersion: 1, kind: 'test-pack', id: `example.${name}`, version: '1.0.0', state: 'draft',
    title: name, requiresPacks: [], conflictsWith: [], capabilities: ['browser'],
    evidence: ['browser-observation'], budget: { status: 'unmeasured' },
    task: { requirements: [{ id: `example.${name}.requirement`, path: 'prompts/task.md', order: 10 }], contracts: [] },
    checks: [{ id: 'group', source: 'scenarios/01.json', feature: 1, role: 'feature' }],
    ...extra,
  });
  const writePack = (name, extra) => writeFileSync(join(root, 'composition', 'packs', `${name}.json`),
    JSON.stringify(makePack(name, extra)));
  const makeRecipe = (packs, scoring = { mode: 'legacy-source-points' }) => ({
    schemaVersion: 1, kind: 'benchmark-recipe', id: 'example.recipe', version: '1.0.0', state: 'draft',
    title: 'Recipe', track: 'example',
    compatibility: { legacyLevel: 1 },
    fixture: { path: '../fixtures/fixture.json', id: 'example.fixture', version: '1.0.0' },
    task: { mode: 'fresh', framing: { requirements: [
      { id: 'example.framing', path: 'prompts/task.md', order: 0 },
    ], contracts: [{ id: 'example.contract', path: 'contracts/contract.json', order: 0 }] } },
    packs: packs.map(name => ({ path: `../packs/${name}.json`, id: `example.${name}`,
      version: '1.0.0', includeRoles: ['feature'] })),
    execution: [{ id: 'features', source: 'scenarios/01.json' }],
    scoring,
  });
  const writeRecipe = value => {
    const path = join(root, 'composition', 'recipes', 'recipe.json');
    writeFileSync(path, JSON.stringify(value));
    return path;
  };
  return { temp, root, makePack, writePack, makeRecipe, writeRecipe };
}

test('composition rejects missing dependencies, dependency cycles, conflicts, duplicate ownership, and unsupported capabilities', () => {
  const box = sandbox();
  try {
    box.writePack('a', { requiresPacks: ['example.b@1.0.0'] });
    let path = box.writeRecipe(box.makeRecipe(['a']));
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }), /missing example.b@1.0.0/);

    box.writePack('b', { requiresPacks: ['example.a@1.0.0'] });
    path = box.writeRecipe(box.makeRecipe(['a', 'b']));
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }), /dependency cycle/);

    box.writePack('a', { conflictsWith: ['example.b@1.0.0'] });
    box.writePack('b');
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }), /conflicts with selected/);

    box.writePack('a');
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }), /criterion already selected/);

    box.writePack('b', { task: { requirements: [
      { id: 'example.a.requirement', path: 'prompts/task.md', order: 11 },
    ], contracts: [] } });
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }), /shared fragment.*does not match/);

    path = box.writeRecipe(box.makeRecipe(['a']));
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root, availableCapabilities: [] }),
      /unsupported capabilities: browser/);

    box.writePack('a', { moduleType: 'feature' });
    box.writePack('b', { moduleType: 'specification', task: { requirements: [{
      id: 'example.b.requirement', path: 'prompts/task.md', order: 12,
      requiresFeatures: ['example.missing'],
    }], contracts: [] }, checks: [{
      id: 'group', source: 'scenarios/01.json', feature: 1, role: 'guarantee',
      observations: ['requested', 'unmentioned'], requiresFeatures: ['example.missing'],
    }] });
    const modular = box.makeRecipe(['a', 'b']);
    modular.packs[1].includeRoles = ['guarantee'];
    path = box.writeRecipe(modular);
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }),
      /references missing feature module example.missing/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('explicit scoring must name every selected stable check exactly once', () => {
  const box = sandbox();
  try {
    box.writePack('a');
    let path = box.writeRecipe(box.makeRecipe(['a'], { mode: 'explicit', weights: {} }));
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }), /missing example.a.group.1a/);
    path = box.writeRecipe(box.makeRecipe(['a'], {
      mode: 'explicit', weights: { 'example.a.group.1a': 7 },
    }));
    const plan = compileRecipeFile(path, { trackRoot: box.root });
    assert.equal(plan.scoring.points, 7);
    assert.equal(plan.checks[0].sourcePoints, 2);
    assert.equal(plan.checks[0].points, 7);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('a versioned criterion can move to a focused scenario without changing its stable key', () => {
  const box = sandbox();
  try {
    writeFileSync(join(box.root, 'scenarios', '02.json'), JSON.stringify({
      level: 1,
      features: [{ id: 2, name: 'Focused check', actors: ['a'], setup: [], criteria: [
        { id: '1b', desc: 'Focused behavior', points: 3,
          steps: [{ do: 'wait', actor: 'a', ms: 1 }] },
      ] }],
    }));
    box.writePack('a', { checks: [
      { id: 'group-baseline', stableId: 'group', source: 'scenarios/01.json',
        feature: 1, role: 'feature' },
      { id: 'group-focused', stableId: 'group', source: 'scenarios/02.json',
        feature: 2, role: 'feature' },
    ] });
    const recipe = box.makeRecipe(['a']);
    recipe.execution.push({ id: 'focused', source: 'scenarios/02.json' });
    const plan = compileRecipeFile(box.writeRecipe(recipe), { trackRoot: box.root });
    assert.deepEqual(plan.checks.map(check => check.stableKey), [
      'example.a.group.1a',
      'example.a.group.1b',
    ]);
    assert(plan.execution.every(execution =>
      execution.checkGroups.every(group => group.checkGroupId === 'group')));
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('composition references cannot escape the track or composition roots', () => {
  const box = sandbox();
  try {
    box.writePack('a');
    const recipe = box.makeRecipe(['a']);
    recipe.packs[0].path = '../../../outside.json';
    const path = box.writeRecipe(recipe);
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }), /escapes/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('a promotion catalog cannot point a live alias at a draft recipe', () => {
  const box = sandbox();
  try {
    box.writePack('a');
    box.writeRecipe(box.makeRecipe(['a']));
    const catalog = {
      schemaVersion: 1, kind: 'promotion-catalog', id: 'example.recipes', version: '1.0.0',
      state: 'draft', title: 'Promotions', entries: [{ alias: 'L1', status: 'promoted',
        recipe: { path: 'recipes/recipe.json', id: 'example.recipe', version: '1.0.0' } }],
    };
    const path = join(box.root, 'composition', 'promotions.json');
    writeFileSync(path, JSON.stringify(catalog));
    assert.throws(() => compilePromotionFile(path, { trackRoot: box.root }), /while it is draft/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('qualified recipes cannot select draft packs, fixtures, or base recipes', () => {
  const box = sandbox();
  try {
    box.writePack('a');
    const recipe = box.makeRecipe(['a']);
    recipe.state = 'qualified';
    const path = box.writeRecipe(recipe);
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }),
      /qualified recipe selects draft fixture/);
    const fixturePath = join(box.root, 'composition', 'fixtures', 'fixture.json');
    const fixture = JSON.parse(readFileSync(fixturePath, 'utf8'));
    fixture.state = 'qualified';
    writeFileSync(fixturePath, JSON.stringify(fixture));
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }),
      /qualified recipe selects draft pack/);

    box.writePack('a', { state: 'qualified', budget: { status: 'bounded', maxRuntimeMs: 1000 } });
    const base = box.makeRecipe(['a']);
    base.id = 'example.base';
    writeFileSync(join(box.root, 'composition', 'recipes', 'base.json'), JSON.stringify(base));
    recipe.task = { ...recipe.task, mode: 'upgrade', baseRecipe: {
      path: 'base.json', id: 'example.base', version: '1.0.0',
    } };
    box.writeRecipe(recipe);
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }),
      /qualified upgrade recipe selects draft base/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('upgrade recipe references are exact, resolvable, and acyclic', () => {
  const box = sandbox();
  try {
    box.writePack('a');
    const a = box.makeRecipe(['a']);
    a.id = 'example.recipe-a';
    a.task = { ...a.task, mode: 'upgrade', baseRecipe: {
      path: 'recipe-b.json', id: 'example.recipe-b', version: '1.0.0',
    } };
    const b = box.makeRecipe(['a']);
    b.id = 'example.recipe-b';
    b.task = { ...b.task, mode: 'upgrade', baseRecipe: {
      path: 'recipe-a.json', id: 'example.recipe-a', version: '1.0.0',
    } };
    const aPath = join(box.root, 'composition', 'recipes', 'recipe-a.json');
    const bPath = join(box.root, 'composition', 'recipes', 'recipe-b.json');
    writeFileSync(aPath, JSON.stringify(a));
    writeFileSync(bPath, JSON.stringify(b));
    assert.throws(() => compileRecipeFile(aPath, { trackRoot: box.root }), /recipe dependency cycle/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});
