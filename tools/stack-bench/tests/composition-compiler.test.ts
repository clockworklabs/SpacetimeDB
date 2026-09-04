import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import {
  compileFixtureDefinition,
  compilePackDefinition,
  compilePromotionDefinition,
  compilePromotionFile,
  compileRecipeDefinition,
  compileRecipeFile,
  resolveTaskFragment,
} from '../src/composition/composition-compiler.js';
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
  assert.throws(() => compilePackDefinition({ ...pack, requiresPacks: ['example.other@1.0.0'] }), /pack id/);
  assert.throws(() => compilePackDefinition({ ...pack, state: 'qualified' }),
    /qualified packs require a bounded runtime budget/);
  assert.throws(() => compilePackDefinition({ ...pack, moduleType: 'mode' }),
    /moduleType.*feature or specification/);
  assert.throws(() => compilePackDefinition({ ...pack, stableId: 'Published Score' }),
    /stableId.*lowercase letters/);
  assert.throws(() => compilePackDefinition({ ...pack, moduleType: 'feature', checks: [
    { ...pack.checks[0], role: 'guarantee' },
  ] }), /feature modules cannot own guarantee/);
  assert.throws(() => compilePackDefinition({ ...pack, moduleType: 'specification', task: {
    ...pack.task, requirements: [
      { ...pack.task.requirements[0], requiresFeatures: ['example.feature'] },
    ],
  } }),
    /specification modules cannot own feature/);

  const featureWithGradingSetup = compilePackDefinition({ ...pack, moduleType: 'feature',
    checks: [{ ...pack.checks[0], requiresFeatures: ['example.setup'] }] });
  assert.deepEqual(featureWithGradingSetup.checks[0]?.requiresFeatures, ['example.setup']);

  const specification = compilePackDefinition({ ...pack, id: 'example.durability',
    moduleType: 'specification', task: { ...pack.task, requirements: [
      { ...pack.task.requirements[0], requiresFeatures: ['example.feature'] },
    ] }, checks: [{ ...pack.checks[0], role: 'guarantee',
      observations: ['requested', 'unmentioned'], requiresFeatures: ['example.feature'] }] });
  const specificationCheck = specification.checks[0];
  const specificationRequirement = specification.task.requirements[0];
  assert(specificationCheck);
  assert(specificationRequirement);
  assert.equal(specification.moduleType, 'specification');
  assert.deepEqual(specificationCheck.observations, ['requested', 'unmentioned']);
  assert.deepEqual(specificationCheck.requiresFeatures, ['example.feature']);
  assert.deepEqual(specificationRequirement.requiresFeatures, ['example.feature']);

  const renamed = compilePackDefinition({ ...pack, id: 'example.feature-v2',
    stableId: 'example.feature' });
  assert.equal(renamed.id, 'example.feature-v2');
  assert.equal(renamed.stableId, 'example.feature');

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
    sequence: { level: 1 },
    scoring: { mode: 'source-points' },
  };
  assert.doesNotThrow(() => compileRecipeDefinition(recipe));
  assert.throws(() => compileRecipeDefinition({ ...recipe,
    scoring: { mode: 'legacy-source-points' } }), /must be source-points or explicit/);
  assert.throws(() => compileRecipeDefinition({ ...recipe, compatibility: {
    legacyLevel: 1,
  } }), /compatibility: unknown field/);
  assert.throws(() => compileRecipeDefinition({ ...recipe,
    sequence: { level: 2 } }), /sequence levels after 1 must use upgrade mode/);
  const catalog = {
    schemaVersion: 1, kind: 'promotion-catalog', id: 'example.recipes', version: '1.0.0',
    state: 'draft', title: 'Promotions', entries: [{ alias: 'latest', status: 'candidate',
      recipe: { path: 'recipes/r.json', id: 'example.recipe', version: '1.0.0' } }],
  };
  assert.throws(() => compilePromotionDefinition(catalog), /must look like L1/);
  assert.deepEqual(compilePromotionDefinition({ ...catalog, entries: [] }).entries, []);
  assert.throws(() => compilePromotionDefinition({ ...catalog, state: 'qualified', entries: [] }),
    /must be non-empty once the catalog is qualified/);
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

interface TestFragment {
  id: string;
  path: string;
  order: number;
  requiresFeatures?: string[];
}

interface TestPack {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  stableId?: string;
  moduleType?: string;
  requiresPacks: string[];
  conflictsWith: string[];
  capabilities: string[];
  evidence: string[];
  budget: { status: string; maxRuntimeMs?: number };
  task: { requirements: TestFragment[]; contracts: TestFragment[] };
  checks: Array<{
    id: string;
    stableId?: string;
    source: string;
    feature: number;
    criteria?: string[];
    role: string;
    observations?: string[];
    requiresFeatures?: string[];
  }>;
}

interface TestScoring {
  mode: string;
  weights?: Record<string, number>;
}

interface TestRecipe {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  track: string;
  sequence: { level: number };
  fixture: { path: string; id: string; version: string };
  task: {
    mode: string;
    framing: { requirements: TestFragment[]; contracts: TestFragment[] };
    baseRecipe?: { path: string; id: string; version: string };
  };
  packs: Array<{ path: string; id: string; version: string; includeRoles: string[];
    includeCheckGroups?: string[] }>;
  execution: Array<{ id: string; source: string }>;
  scoring: TestScoring;
}

interface CompositionSandbox {
  temp: string;
  root: string;
  makePack(name: string, extra?: Partial<TestPack>): TestPack;
  writePack(name: string, extra?: Partial<TestPack>): void;
  makeRecipe(packs: string[], scoring?: TestScoring): TestRecipe;
  writeRecipe(value: TestRecipe): string;
}

function sandbox(): CompositionSandbox {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-composition-'));
  const root = join(temp, 'example');
  for (const directory of [
    'composition/recipes', 'composition/packs', 'composition/fixtures', 'scenarios', 'prompts', 'contracts',
  ]) mkdirSync(join(root, directory), { recursive: true });
  writeFileSync(join(root, 'prompts', 'task.md'), 'Build it.');
  writeFileSync(join(root, 'contracts', 'contract.json'), '{}');
  writeFileSync(join(root, 'scenarios', '01.json'), JSON.stringify({
    schemaVersion: 1,
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
  const makePack = (name: string, extra: Partial<TestPack> = {}): TestPack => ({
    schemaVersion: 1, kind: 'test-pack', id: `example.${name}`, version: '1.0.0', state: 'draft',
    title: name, requiresPacks: [], conflictsWith: [], capabilities: ['browser'],
    evidence: ['browser-observation'], budget: { status: 'unmeasured' },
    task: { requirements: [{ id: `example.${name}.requirement`, path: 'prompts/task.md', order: 10 }], contracts: [] },
    checks: [{ id: 'group', source: 'scenarios/01.json', feature: 1, role: 'feature' }],
    ...extra,
  });
  const writePack = (name: string, extra: Partial<TestPack> = {}): void => {
    writeFileSync(join(root, 'composition', 'packs', `${name}.json`),
      JSON.stringify(makePack(name, extra)));
  };
  const makeRecipe = (packs: string[],
    scoring: TestScoring = { mode: 'source-points' }): TestRecipe => ({
    schemaVersion: 1, kind: 'benchmark-recipe', id: 'example.recipe', version: '1.0.0', state: 'draft',
    title: 'Recipe', track: 'example',
    sequence: { level: 1 },
    fixture: { path: '../fixtures/fixture.json', id: 'example.fixture', version: '1.0.0' },
    task: { mode: 'fresh', framing: { requirements: [
      { id: 'example.framing', path: 'prompts/task.md', order: 0 },
    ], contracts: [{ id: 'example.contract', path: 'contracts/contract.json', order: 0 }] } },
    packs: packs.map(name => ({ path: `../packs/${name}.json`, id: `example.${name}`,
      version: '1.0.0', includeRoles: ['feature'] })),
    execution: [{ id: 'features', source: 'scenarios/01.json' }],
    scoring,
  });
  const writeRecipe = (value: TestRecipe): string => {
    const path = join(root, 'composition', 'recipes', 'recipe.json');
    writeFileSync(path, JSON.stringify(value));
    return path;
  };
  return { temp, root, makePack, writePack, makeRecipe, writeRecipe };
}

test('composition rejects missing dependencies, dependency cycles, conflicts, duplicate ownership, and unsupported capabilities', () => {
  const box = sandbox();
  try {
    box.writePack('a', { requiresPacks: ['example.b'] });
    let path = box.writeRecipe(box.makeRecipe(['a']));
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }), /missing example.b/);

    box.writePack('b', { requiresPacks: ['example.a'] });
    path = box.writeRecipe(box.makeRecipe(['a', 'b']));
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }), /dependency cycle/);

    box.writePack('a', { conflictsWith: ['example.b'] });
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
    const specificationSelection = modular.packs[1];
    assert(specificationSelection);
    specificationSelection.includeRoles = ['guarantee'];
    path = box.writeRecipe(modular);
    assert.throws(() => compileRecipeFile(path, { trackRoot: box.root }),
      /references missing feature module example.missing/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('a recipe scopes grading by check group and prompts by feature module', () => {
  const box = sandbox();
  try {
    writeFileSync(join(box.root, 'scenarios', '01.json'), JSON.stringify({
      schemaVersion: 1,
      level: 1,
      features: [{ id: 1, name: 'Feature', actors: ['a'], setup: [], criteria: [
        { id: '1a', desc: 'Current behavior', points: 2,
          steps: [{ do: 'wait', actor: 'a', ms: 1 }] },
        { id: '1b', desc: 'Future behavior', points: 3,
          steps: [{ do: 'wait', actor: 'a', ms: 1 }] },
      ] }],
    }));
    box.writePack('feature', { moduleType: 'feature', checks: [{
      id: 'group', source: 'scenarios/01.json', feature: 1, criteria: ['1b'], role: 'feature',
    }] });
    box.writePack('specification', {
      moduleType: 'specification',
      task: { requirements: [
        { id: 'example.specification.current', path: 'prompts/task.md', order: 11,
          requiresFeatures: ['example.feature'] },
        { id: 'example.specification.shared-feature', path: 'prompts/task.md', order: 12,
          requiresFeatures: ['example.feature'] },
        { id: 'example.specification.future', path: 'prompts/task.md', order: 13,
          requiresFeatures: ['example.future'] },
      ], contracts: [] },
      checks: [
        { id: 'current', source: 'scenarios/01.json', feature: 1, criteria: ['1a'],
          role: 'guarantee', observations: ['requested', 'unmentioned'],
          requiresFeatures: ['example.feature'] },
        { id: 'future', source: 'scenarios/01.json', feature: 1, criteria: ['1b'],
          role: 'guarantee', observations: ['requested', 'unmentioned'],
          requiresFeatures: ['example.future'] },
      ],
    });
    const recipe = box.makeRecipe(['feature', 'specification']);
    const selection = recipe.packs[1];
    assert(selection);
    selection.includeRoles = ['guarantee'];
    selection.includeCheckGroups = ['current'];

    const plan = compileRecipeFile(box.writeRecipe(recipe), { trackRoot: box.root });
    assert.deepEqual(plan.checks.map(check => check.stableKey), [
      'example.specification.current.1a',
      'example.feature.group.1b',
    ]);
    assert.deepEqual(plan.packs[1]?.task.requirementIds,
      ['example.specification.current', 'example.specification.shared-feature']);
    assert.equal(plan.recipe.task.requirements.some(fragment =>
      fragment.id === 'example.specification.future'), false);

    selection.includeCheckGroups = ['missing'];
    assert.throws(() => compileRecipeFile(box.writeRecipe(recipe), { trackRoot: box.root }),
      /unknown check group missing/);
    selection.includeCheckGroups = ['current'];
    selection.includeRoles = ['feature'];
    assert.throws(() => compileRecipeFile(box.writeRecipe(recipe), { trackRoot: box.root }),
      /check group current has excluded role guarantee/);
    selection.includeRoles = ['guarantee'];
    selection.includeCheckGroups = ['future'];
    assert.throws(() => compileRecipeFile(box.writeRecipe(recipe), { trackRoot: box.root }),
      /references missing feature module example.future/);
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
    const selectedCheck = plan.checks[0];
    assert(selectedCheck);
    assert.equal(plan.scoring.points, 7);
    assert.equal(selectedCheck.sourcePoints, 2);
    assert.equal(selectedCheck.points, 7);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('a versioned criterion can move to a focused scenario without changing its stable key', () => {
  const box = sandbox();
  try {
    writeFileSync(join(box.root, 'scenarios', '02.json'), JSON.stringify({
      schemaVersion: 1,
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

test('separate modules can share a published scoring namespace without hiding their ownership', () => {
  const box = sandbox();
  try {
    writeFileSync(join(box.root, 'scenarios', '01.json'), JSON.stringify({
      schemaVersion: 1,
      level: 1,
      features: [{ id: 1, name: 'Feature', actors: ['a'], setup: [], criteria: [
        { id: '1a', desc: 'Requested behavior', points: 2,
          steps: [{ do: 'wait', actor: 'a', ms: 1 }] },
        { id: '1b', desc: 'Quality property', points: 3,
          steps: [{ do: 'wait', actor: 'a', ms: 1 }] },
      ] }],
    }));
    box.writePack('feature', {
      moduleType: 'feature', stableId: 'example.published',
      checks: [{ id: 'group', source: 'scenarios/01.json', feature: 1,
        criteria: ['1a'], role: 'feature' }],
    });
    box.writePack('specification', {
      moduleType: 'specification', stableId: 'example.published',
      task: { requirements: [{ id: 'example.specification.requirement', path: 'prompts/task.md',
        order: 11, requiresFeatures: ['example.feature'] }], contracts: [] },
      checks: [{ id: 'group', source: 'scenarios/01.json', feature: 1,
        criteria: ['1b'], role: 'guarantee', observations: ['requested', 'unmentioned'],
        requiresFeatures: ['example.feature'] }],
    });
    const recipe = box.makeRecipe(['feature', 'specification'], { mode: 'explicit', weights: {
      'example.published.group.1a': 2,
      'example.published.group.1b': 3,
    } });
    const specificationSelection = recipe.packs[1];
    assert(specificationSelection);
    specificationSelection.includeRoles = ['guarantee'];
    const plan = compileRecipeFile(box.writeRecipe(recipe), { trackRoot: box.root });
    assert.deepEqual(plan.checks.map(check => ({
      stableKey: check.stableKey,
      packId: check.packId,
      stablePackId: check.stablePackId,
    })), [
      { stableKey: 'example.published.group.1a', packId: 'example.feature',
        stablePackId: 'example.published' },
      { stableKey: 'example.published.group.1b', packId: 'example.specification',
        stablePackId: 'example.published' },
    ]);
    assert.deepEqual(plan.packs.map(selected => [selected.id, selected.stableId]), [
      ['example.feature', 'example.published'],
      ['example.specification', 'example.published'],
    ]);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('published scoring namespaces still reject duplicate stable check keys', () => {
  const box = sandbox();
  try {
    writeFileSync(join(box.root, 'scenarios', '02.json'), JSON.stringify({
      schemaVersion: 1,
      level: 1,
      features: [{ id: 2, name: 'Other feature', actors: ['a'], setup: [], criteria: [
        { id: '1a', desc: 'Conflicting identity', points: 1,
          steps: [{ do: 'wait', actor: 'a', ms: 1 }] },
      ] }],
    }));
    box.writePack('a', { stableId: 'example.published' });
    box.writePack('b', { stableId: 'example.published', checks: [
      { id: 'group', source: 'scenarios/02.json', feature: 2, role: 'feature' },
    ] });
    const recipe = box.makeRecipe(['a', 'b']);
    recipe.execution.push({ id: 'other', source: 'scenarios/02.json' });
    assert.throws(() => compileRecipeFile(box.writeRecipe(recipe), { trackRoot: box.root }),
      /duplicate stable check key example\.published\.group\.1a/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('composition references cannot escape the track or composition roots', () => {
  const box = sandbox();
  try {
    box.writePack('a');
    const recipe = box.makeRecipe(['a']);
    const pack = recipe.packs[0];
    assert(pack);
    pack.path = '../../../outside.json';
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
    recipe.sequence = { level: 2 };
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
    a.sequence = { level: 2 };
    a.task = { ...a.task, mode: 'upgrade', baseRecipe: {
      path: 'recipe-b.json', id: 'example.recipe-b', version: '1.0.0',
    } };
    const b = box.makeRecipe(['a']);
    b.id = 'example.recipe-b';
    b.sequence = { level: 2 };
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
