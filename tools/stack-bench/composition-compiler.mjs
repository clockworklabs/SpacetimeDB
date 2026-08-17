import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { dirname, relative, resolve, sep } from 'node:path';

import { compileScenarioDefinition } from './definition-compiler.mjs';

export const COMPOSITION_SCHEMA_VERSION = 1;

const ID = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/;
const STATES = new Set(['draft', 'qualified', 'retired']);
const ROLES = new Set(['feature', 'guarantee', 'control']);
const MODULE_TYPES = new Set(['feature', 'specification']);
const OBSERVATIONS = new Set(['requested', 'unmentioned']);

const isObject = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const fail = (at, message) => { throw new Error(`invalid benchmark composition at ${at}: ${message}`); };

function strictObject(value, at, allowed) {
  if (!isObject(value)) fail(at, 'must be an object');
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) fail(`${at}.${key}`, 'unknown field');
  }
}

function string(value, at, { nonEmpty = true } = {}) {
  if (typeof value !== 'string' || (nonEmpty && value.trim().length === 0)) {
    fail(at, `must be a${nonEmpty ? ' non-empty' : ''} string`);
  }
  return value;
}

function id(value, at) {
  string(value, at);
  if (!ID.test(value)) fail(at, 'must contain lowercase letters, numbers, dots, dashes, or underscores');
  return value;
}

function version(value, at) {
  string(value, at);
  if (!VERSION.test(value)) fail(at, 'must be an exact semantic version');
  return value;
}

function exactRef(value, at) {
  string(value, at);
  const split = value.lastIndexOf('@');
  if (split < 1) fail(at, 'must be an exact id@version reference');
  id(value.slice(0, split), `${at} id`);
  version(value.slice(split + 1), `${at} version`);
  return value;
}

function uniqueStrings(value, at) {
  if (!Array.isArray(value)) fail(at, 'must be an array');
  const seen = new Set();
  return value.map((item, index) => {
    string(item, `${at}[${index}]`);
    if (seen.has(item)) fail(`${at}[${index}]`, `duplicates ${JSON.stringify(item)}`);
    seen.add(item);
    return item;
  });
}

function identityFields(value, at, kind) {
  if (value.schemaVersion !== COMPOSITION_SCHEMA_VERSION) {
    fail(`${at}.schemaVersion`, `must be ${COMPOSITION_SCHEMA_VERSION}`);
  }
  if (value.kind !== kind) fail(`${at}.kind`, `must be ${JSON.stringify(kind)}`);
  id(value.id, `${at}.id`);
  version(value.version, `${at}.version`);
  if (!STATES.has(value.state)) fail(`${at}.state`, 'must be draft, qualified, or retired');
  string(value.title, `${at}.title`);
}

function readJson(path, label) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new Error(`cannot read ${label} ${path}: ${error.message}`, { cause: error });
  }
}

function contained(root, from, path, at) {
  string(path, at);
  const lexicalRoot = resolve(root);
  const candidate = resolve(from, path);
  const lexicalRelative = relative(lexicalRoot, candidate);
  if (lexicalRelative === '..' || lexicalRelative.startsWith(`..${sep}`)) {
    fail(at, `escapes ${lexicalRoot}`);
  }
  if (!existsSync(candidate)) fail(at, `does not exist: ${path}`);
  const rootPath = realpathSync(lexicalRoot);
  const target = realpathSync(candidate);
  const rel = relative(rootPath, target);
  if (rel === '..' || rel.startsWith(`..${sep}`)) fail(at, `escapes ${rootPath}`);
  return { absolute: target, relative: rel.replaceAll('\\', '/') };
}

const PACK_FIELDS = new Set([
  'schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'description',
  'moduleType', 'requiresPacks', 'conflictsWith', 'capabilities', 'evidence', 'budget', 'task', 'checks',
]);
const CHECK_REF_FIELDS = new Set([
  'id', 'stableId', 'source', 'feature', 'criteria', 'role', 'observations', 'requiresFeatures',
]);
const BUDGET_FIELDS = new Set(['status', 'maxRuntimeMs']);
const PACK_TASK_FIELDS = new Set(['requirements', 'contracts']);
const TASK_FRAGMENT_FIELDS = new Set([
  'id', 'path', 'order', 'from', 'until', 'modes', 'requiresFeatures',
]);
const TASK_MODES = new Set(['fresh', 'upgrade']);

function taskFragmentDefinition(fragment, at) {
  strictObject(fragment, at, TASK_FRAGMENT_FIELDS);
  id(fragment.id, `${at}.id`);
  string(fragment.path, `${at}.path`);
  if (!Number.isInteger(fragment.order) || fragment.order < 0) {
    fail(`${at}.order`, 'must be a non-negative integer');
  }
  if (fragment.from !== undefined) string(fragment.from, `${at}.from`);
  if (fragment.until !== undefined) string(fragment.until, `${at}.until`);
  fragment.modes = uniqueStrings(fragment.modes ?? ['fresh', 'upgrade'], `${at}.modes`);
  if (fragment.modes.length === 0) fail(`${at}.modes`, 'must not be empty');
  for (const mode of fragment.modes) {
    if (!TASK_MODES.has(mode)) fail(`${at}.modes`, `unknown task mode ${mode}`);
  }
  if (fragment.requiresFeatures !== undefined) {
    fragment.requiresFeatures = uniqueStrings(fragment.requiresFeatures, `${at}.requiresFeatures`).sort();
    if (fragment.requiresFeatures.length === 0) fail(`${at}.requiresFeatures`, 'must not be empty');
    for (const featureId of fragment.requiresFeatures) id(featureId, `${at}.requiresFeatures`);
  }
  return fragment;
}

function taskFragmentSet(value, at) {
  strictObject(value, at, PACK_TASK_FIELDS);
  const seen = new Set();
  const compile = (fragments, kind) => {
    if (!Array.isArray(fragments)) fail(`${at}.${kind}`, 'must be an array');
    return fragments.map((fragment, index) => {
      const compiled = taskFragmentDefinition(fragment, `${at}.${kind}[${index}]`);
      const key = `${kind}:${compiled.id}`;
      if (seen.has(key)) fail(`${at}.${kind}[${index}].id`, `duplicates ${JSON.stringify(compiled.id)}`);
      seen.add(key);
      return compiled;
    });
  };
  return {
    requirements: compile(value.requirements ?? [], 'requirements'),
    contracts: compile(value.contracts ?? [], 'contracts'),
  };
}

export function resolveTaskFragment(fragmentInput, { trackRoot, source = '<task-fragment>',
  sourceCache = new Map() } = {}) {
  if (!trackRoot) throw new Error('task fragment resolution requires trackRoot');
  const fragment = taskFragmentDefinition(structuredClone(fragmentInput), source);
  const root = resolve(trackRoot);
  const sourceRef = contained(root, root, fragment.path, `${source}.path`);
  let sourceText = sourceCache.get(sourceRef.relative);
  if (sourceText === undefined) {
    sourceText = readFileSync(sourceRef.absolute, 'utf8');
    sourceCache.set(sourceRef.relative, sourceText);
  }
  let start = 0;
  if (fragment.from !== undefined) {
    start = sourceText.indexOf(fragment.from);
    if (start < 0) fail(`${source}.from`, `marker not found in ${sourceRef.relative}`);
    if (sourceText.indexOf(fragment.from, start + 1) >= 0) {
      fail(`${source}.from`, `marker is not unique in ${sourceRef.relative}`);
    }
  }
  let end = sourceText.length;
  if (fragment.until !== undefined) {
    end = sourceText.indexOf(fragment.until, start);
    if (end < 0) fail(`${source}.until`, `marker not found after fragment start in ${sourceRef.relative}`);
    if (sourceText.indexOf(fragment.until, end + 1) >= 0) {
      fail(`${source}.until`, `marker is not unique in ${sourceRef.relative}`);
    }
  }
  if (end <= start) fail(source, 'must select non-empty text in source order');
  return {
    id: fragment.id,
    path: sourceRef.relative,
    order: fragment.order,
    from: fragment.from ?? null,
    until: fragment.until ?? null,
    modes: [...fragment.modes].sort(),
    ...(fragment.requiresFeatures === undefined ? {} : { requiresFeatures: fragment.requiresFeatures }),
    text: sourceText.slice(start, end),
  };
}

export function compilePackDefinition(input, { source = '<pack>' } = {}) {
  const pack = structuredClone(input);
  strictObject(pack, source, PACK_FIELDS);
  identityFields(pack, source, 'test-pack');
  if (pack.moduleType !== undefined && !MODULE_TYPES.has(pack.moduleType)) {
    fail(`${source}.moduleType`, 'must be feature or specification');
  }
  if (pack.description !== undefined) string(pack.description, `${source}.description`);
  pack.requiresPacks = uniqueStrings(pack.requiresPacks ?? [], `${source}.requiresPacks`);
  pack.conflictsWith = uniqueStrings(pack.conflictsWith ?? [], `${source}.conflictsWith`);
  pack.requiresPacks.forEach((ref, index) => exactRef(ref, `${source}.requiresPacks[${index}]`));
  pack.conflictsWith.forEach((ref, index) => exactRef(ref, `${source}.conflictsWith[${index}]`));
  pack.capabilities = uniqueStrings(pack.capabilities ?? [], `${source}.capabilities`);
  if (pack.capabilities.length === 0) fail(`${source}.capabilities`, 'must not be empty');
  pack.evidence = uniqueStrings(pack.evidence ?? [], `${source}.evidence`);
  if (pack.evidence.length === 0) fail(`${source}.evidence`, 'must not be empty');
  strictObject(pack.budget, `${source}.budget`, BUDGET_FIELDS);
  if (!['unmeasured', 'bounded'].includes(pack.budget.status)) {
    fail(`${source}.budget.status`, 'must be unmeasured or bounded');
  }
  if (pack.budget.status === 'bounded') {
    if (!Number.isInteger(pack.budget.maxRuntimeMs) || pack.budget.maxRuntimeMs < 1) {
      fail(`${source}.budget.maxRuntimeMs`, 'must be a positive integer for a bounded budget');
    }
  } else if (pack.budget.maxRuntimeMs !== undefined) {
    fail(`${source}.budget.maxRuntimeMs`, 'is allowed only for a bounded budget');
  }
  if (pack.state === 'qualified' && pack.budget.status !== 'bounded') {
    fail(`${source}.budget`, 'qualified packs require a bounded runtime budget');
  }
  pack.task = taskFragmentSet(pack.task, `${source}.task`);
  if (pack.task.requirements.length === 0) {
    fail(`${source}.task.requirements`, 'must not be empty');
  }
  if (!Array.isArray(pack.checks) || pack.checks.length === 0) {
    fail(`${source}.checks`, 'must be a non-empty array');
  }
  const taskFragments = [...pack.task.requirements, ...pack.task.contracts];
  if (pack.moduleType === 'feature'
    && taskFragments.some(fragment => fragment.requiresFeatures !== undefined)) {
    fail(`${source}.task`, 'feature module fragments cannot declare specification applicability');
  }
  if (pack.moduleType === 'specification'
    && pack.task.requirements.some(fragment => fragment.requiresFeatures === undefined)) {
    fail(`${source}.task.requirements`,
      'specification requirement fragments must declare applicable feature modules');
  }
  const checkIds = new Set();
  pack.checks.forEach((check, index) => {
    const at = `${source}.checks[${index}]`;
    strictObject(check, at, CHECK_REF_FIELDS);
    id(check.id, `${at}.id`);
    if (checkIds.has(check.id)) fail(`${at}.id`, `duplicate check group ${check.id}`);
    checkIds.add(check.id);
    if (check.stableId !== undefined) id(check.stableId, `${at}.stableId`);
    string(check.source, `${at}.source`);
    if (!Number.isInteger(check.feature) || check.feature < 1) {
      fail(`${at}.feature`, 'must be a positive integer');
    }
    if (check.criteria !== undefined) {
      check.criteria = uniqueStrings(check.criteria, `${at}.criteria`);
      if (check.criteria.length === 0) fail(`${at}.criteria`, 'must not be empty');
      for (const criterion of check.criteria) id(criterion, `${at}.criteria`);
    }
    if (!ROLES.has(check.role)) fail(`${at}.role`, 'must be feature, guarantee, or control');
    if (check.observations !== undefined) {
      check.observations = uniqueStrings(check.observations, `${at}.observations`).sort();
      if (check.observations.length === 0) fail(`${at}.observations`, 'must not be empty');
      for (const observation of check.observations) {
        if (!OBSERVATIONS.has(observation)) {
          fail(`${at}.observations`, `unknown observation class ${observation}`);
        }
      }
    }
    if (check.requiresFeatures !== undefined) {
      check.requiresFeatures = uniqueStrings(check.requiresFeatures, `${at}.requiresFeatures`).sort();
      if (check.requiresFeatures.length === 0) fail(`${at}.requiresFeatures`, 'must not be empty');
      for (const featureId of check.requiresFeatures) id(featureId, `${at}.requiresFeatures`);
    }
  });
  if (pack.moduleType === 'feature') {
    if (pack.checks.some(check => check.role === 'guarantee')) {
      fail(`${source}.checks`, 'feature modules cannot own guarantee checks');
    }
    if (pack.checks.some(check => check.observations?.includes('unmentioned'))) {
      fail(`${source}.checks`, 'feature modules cannot own unmentioned observations');
    }
    if (pack.checks.some(check => check.requiresFeatures !== undefined)) {
      fail(`${source}.checks`, 'feature modules cannot declare specification applicability');
    }
  }
  if (pack.moduleType === 'specification') {
    if (pack.checks.some(check => check.role === 'feature')) {
      fail(`${source}.checks`, 'specification modules cannot own feature checks');
    }
    if (pack.checks.some(check => check.observations === undefined)) {
      fail(`${source}.checks`, 'specification modules must declare requested/unmentioned observations');
    }
    if (pack.checks.some(check => check.requiresFeatures === undefined)) {
      fail(`${source}.checks`, 'specification checks must declare applicable feature modules');
    }
  }
  return pack;
}

const FIXTURE_FIELDS = new Set([
  'schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'warehouses',
  'items', 'accounts', 'empty',
]);
const ITEM_FIELDS = new Set(['name', 'price', 'category', 'stock']);
const ACCOUNT_FIELDS = new Set(['username', 'password', 'roles']);

export function compileFixtureDefinition(input, { source = '<fixture>' } = {}) {
  const fixture = structuredClone(input);
  strictObject(fixture, source, FIXTURE_FIELDS);
  identityFields(fixture, source, 'fixture-set');
  fixture.warehouses = uniqueStrings(fixture.warehouses, `${source}.warehouses`);
  if (fixture.warehouses.length === 0) fail(`${source}.warehouses`, 'must not be empty');
  if (!Array.isArray(fixture.items) || fixture.items.length === 0) {
    fail(`${source}.items`, 'must be a non-empty array');
  }
  const itemNames = new Set();
  fixture.items.forEach((item, index) => {
    const at = `${source}.items[${index}]`;
    strictObject(item, at, ITEM_FIELDS);
    string(item.name, `${at}.name`);
    if (itemNames.has(item.name)) fail(`${at}.name`, `duplicate item ${item.name}`);
    itemNames.add(item.name);
    string(item.price, `${at}.price`);
    if (!/^\d+\.\d{2}$/.test(item.price)) fail(`${at}.price`, 'must be a decimal string with two places');
    string(item.category, `${at}.category`);
    strictObject(item.stock, `${at}.stock`, new Set(fixture.warehouses));
    for (const warehouse of fixture.warehouses) {
      if (!Number.isInteger(item.stock[warehouse]) || item.stock[warehouse] < 0) {
        fail(`${at}.stock.${warehouse}`, 'must be a non-negative integer');
      }
    }
    for (const warehouse of fixture.warehouses) {
      if (!(warehouse in item.stock)) fail(`${at}.stock.${warehouse}`, 'is required');
    }
  });
  if (!Array.isArray(fixture.accounts)) fail(`${source}.accounts`, 'must be an array');
  const usernames = new Set();
  fixture.accounts.forEach((account, index) => {
    const at = `${source}.accounts[${index}]`;
    strictObject(account, at, ACCOUNT_FIELDS);
    string(account.username, `${at}.username`);
    if (usernames.has(account.username)) fail(`${at}.username`, `duplicate account ${account.username}`);
    usernames.add(account.username);
    string(account.password, `${at}.password`);
    account.roles = uniqueStrings(account.roles, `${at}.roles`);
    if (account.roles.length === 0) fail(`${at}.roles`, 'must not be empty');
  });
  fixture.empty = uniqueStrings(fixture.empty ?? [], `${source}.empty`);
  return fixture;
}

const RECIPE_FIELDS = new Set([
  'schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'track',
  'fixture', 'task', 'packs', 'execution', 'scoring', 'compatibility',
]);
const FILE_REF_FIELDS = new Set(['path', 'id', 'version']);
const TASK_FIELDS = new Set(['mode', 'baseRecipe', 'framing']);
const PACK_SELECTION_FIELDS = new Set(['path', 'id', 'version', 'includeRoles']);
const EXECUTION_FIELDS = new Set(['id', 'source']);
const SCORING_FIELDS = new Set(['mode', 'weights']);
const COMPATIBILITY_FIELDS = new Set(['legacyLevel', 'mode']);

function validateFileRef(ref, at) {
  strictObject(ref, at, FILE_REF_FIELDS);
  string(ref.path, `${at}.path`);
  id(ref.id, `${at}.id`);
  version(ref.version, `${at}.version`);
}

export function compileRecipeDefinition(input, { source = '<recipe>' } = {}) {
  const recipe = structuredClone(input);
  strictObject(recipe, source, RECIPE_FIELDS);
  identityFields(recipe, source, 'benchmark-recipe');
  id(recipe.track, `${source}.track`);
  validateFileRef(recipe.fixture, `${source}.fixture`);
  strictObject(recipe.task, `${source}.task`, TASK_FIELDS);
  if (!TASK_MODES.has(recipe.task.mode)) {
    fail(`${source}.task.mode`, 'must be fresh or upgrade');
  }
  if (recipe.task.mode === 'upgrade') {
    validateFileRef(recipe.task.baseRecipe, `${source}.task.baseRecipe`);
  } else if (recipe.task.baseRecipe !== undefined) {
    fail(`${source}.task.baseRecipe`, 'is allowed only for upgrade recipes');
  }
  recipe.task.framing = taskFragmentSet(recipe.task.framing, `${source}.task.framing`);
  if (recipe.task.framing.requirements.length === 0) {
    fail(`${source}.task.framing.requirements`, 'must not be empty');
  }
  if (!Array.isArray(recipe.packs) || recipe.packs.length === 0) fail(`${source}.packs`, 'must not be empty');
  const packIds = new Set();
  recipe.packs.forEach((selection, index) => {
    const at = `${source}.packs[${index}]`;
    strictObject(selection, at, PACK_SELECTION_FIELDS);
    validateFileRef({ path: selection.path, id: selection.id, version: selection.version }, at);
    if (packIds.has(selection.id)) fail(`${at}.id`, `duplicate selected pack ${selection.id}`);
    packIds.add(selection.id);
    selection.includeRoles = uniqueStrings(selection.includeRoles, `${at}.includeRoles`);
    if (selection.includeRoles.length === 0) fail(`${at}.includeRoles`, 'must not be empty');
    for (const role of selection.includeRoles) {
      if (!ROLES.has(role)) fail(`${at}.includeRoles`, `unknown role ${role}`);
    }
  });
  if (!Array.isArray(recipe.execution) || recipe.execution.length === 0) {
    fail(`${source}.execution`, 'must not be empty');
  }
  const executionIds = new Set();
  const executionSources = new Set();
  recipe.execution.forEach((entry, index) => {
    const at = `${source}.execution[${index}]`;
    strictObject(entry, at, EXECUTION_FIELDS);
    string(entry.id, `${at}.id`);
    string(entry.source, `${at}.source`);
    if (executionIds.has(entry.id)) fail(`${at}.id`, `duplicate execution id ${entry.id}`);
    if (executionSources.has(entry.source)) fail(`${at}.source`, `duplicate execution source ${entry.source}`);
    executionIds.add(entry.id);
    executionSources.add(entry.source);
  });
  strictObject(recipe.scoring, `${source}.scoring`, SCORING_FIELDS);
  if (!['legacy-source-points', 'explicit'].includes(recipe.scoring.mode)) {
    fail(`${source}.scoring.mode`, 'must be legacy-source-points or explicit');
  }
  if (recipe.scoring.mode === 'explicit') {
    if (!isObject(recipe.scoring.weights)) fail(`${source}.scoring.weights`, 'must be an object');
    for (const [key, points] of Object.entries(recipe.scoring.weights)) {
      string(key, `${source}.scoring.weights key`);
      if (!Number.isInteger(points) || points < 0) {
        fail(`${source}.scoring.weights.${key}`, 'must be a non-negative integer');
      }
    }
  } else if (recipe.scoring.weights !== undefined) {
    fail(`${source}.scoring.weights`, 'is allowed only with explicit scoring');
  }
  if (recipe.compatibility !== undefined) {
    strictObject(recipe.compatibility, `${source}.compatibility`, COMPATIBILITY_FIELDS);
    if (!Number.isInteger(recipe.compatibility.legacyLevel) || recipe.compatibility.legacyLevel < 1) {
      fail(`${source}.compatibility.legacyLevel`, 'must be a positive integer');
    }
    if (recipe.compatibility.mode !== undefined
        && !['legacy-parity', 'cumulative'].includes(recipe.compatibility.mode)) {
      fail(`${source}.compatibility.mode`, 'must be legacy-parity or cumulative');
    }
    if (recipe.compatibility.mode === 'cumulative' && recipe.task.mode !== 'upgrade') {
      fail(`${source}.compatibility.mode`, 'cumulative compatibility requires an upgrade recipe');
    }
  }
  if (recipe.scoring.mode === 'legacy-source-points' && recipe.compatibility === undefined) {
    fail(`${source}.scoring.mode`, 'legacy-source-points is allowed only for a declared compatibility recipe');
  }
  return recipe;
}

export function compileRecipeFile(recipePath, { trackRoot, availableCapabilities = null,
  recipeStack = [] } = {}) {
  const absoluteRecipe = realpathSync(resolve(recipePath));
  const root = resolve(trackRoot ?? dirname(dirname(dirname(absoluteRecipe))));
  const compositionRoot = resolve(root, 'composition');
  const recipeSource = relative(root, absoluteRecipe).replaceAll('\\', '/');
  if (recipeStack.includes(absoluteRecipe)) {
    fail(recipeSource, `recipe dependency cycle: ${[...recipeStack, absoluteRecipe]
      .map(path => relative(root, path).replaceAll('\\', '/')).join(' -> ')}`);
  }
  const recipe = compileRecipeDefinition(readJson(absoluteRecipe, 'recipe'), { source: recipeSource });
  if (recipe.track !== root.split(sep).at(-1)) {
    fail(`${recipeSource}.track`, `must match track directory ${root.split(sep).at(-1)}`);
  }

  const fixtureRef = contained(compositionRoot, dirname(absoluteRecipe), recipe.fixture.path,
    `${recipeSource}.fixture.path`);
  const fixture = compileFixtureDefinition(readJson(fixtureRef.absolute, 'fixture'), {
    source: relative(root, fixtureRef.absolute).replaceAll('\\', '/'),
  });
  if (fixture.id !== recipe.fixture.id || fixture.version !== recipe.fixture.version) {
    fail(`${recipeSource}.fixture`, `expected ${recipe.fixture.id}@${recipe.fixture.version}, found ${fixture.id}@${fixture.version}`);
  }
  let baseRecipe = null;
  if (recipe.task.mode === 'upgrade') {
    const at = `${recipeSource}.task.baseRecipe`;
    const ref = contained(compositionRoot, dirname(absoluteRecipe), recipe.task.baseRecipe.path, `${at}.path`);
    const plan = compileRecipeFile(ref.absolute, { trackRoot: root, availableCapabilities,
      recipeStack: [...recipeStack, absoluteRecipe] });
    if (plan.recipe.id !== recipe.task.baseRecipe.id || plan.recipe.version !== recipe.task.baseRecipe.version) {
      fail(at, `expected ${recipe.task.baseRecipe.id}@${recipe.task.baseRecipe.version}, found ${plan.recipe.id}@${plan.recipe.version}`);
    }
    if (recipe.state === 'qualified' && plan.recipe.state !== 'qualified') {
      fail(at, `qualified upgrade recipe selects ${plan.recipe.state} base ${plan.recipe.id}@${plan.recipe.version}`);
    }
    baseRecipe = { id: plan.recipe.id, version: plan.recipe.version, path: ref.relative };
  }

  const selectedPacks = [];
  const selectedByRef = new Map();
  for (let index = 0; index < recipe.packs.length; index += 1) {
    const selection = recipe.packs[index];
    const at = `${recipeSource}.packs[${index}]`;
    const packRef = contained(compositionRoot, dirname(absoluteRecipe), selection.path, `${at}.path`);
    const pack = compilePackDefinition(readJson(packRef.absolute, 'pack'), {
      source: relative(root, packRef.absolute).replaceAll('\\', '/'),
    });
    if (pack.id !== selection.id || pack.version !== selection.version) {
      fail(at, `expected ${selection.id}@${selection.version}, found ${pack.id}@${pack.version}`);
    }
    const ref = `${pack.id}@${pack.version}`;
    selectedByRef.set(ref, pack);
    selectedPacks.push({ selection, pack, path: packRef.relative });
  }
  for (const { pack } of selectedPacks) {
    for (const required of pack.requiresPacks) {
      if (!selectedByRef.has(required)) fail(`${pack.id}.requiresPacks`, `missing ${required}`);
      const dependency = selectedByRef.get(required);
      if (pack.moduleType === 'feature' && dependency.moduleType !== 'feature') {
        fail(`${pack.id}.requiresPacks`, `feature modules cannot depend on ${required}`);
      }
      if (pack.moduleType === 'specification' && dependency.moduleType === 'feature') {
        fail(`${pack.id}.requiresPacks`,
          `specification modules cannot add ${required}; use check applicability`);
      }
    }
    for (const conflict of pack.conflictsWith) {
      if (selectedByRef.has(conflict)) fail(`${pack.id}.conflictsWith`, `conflicts with selected ${conflict}`);
    }
  }
  const featureModuleIds = new Set(selectedPacks
    .filter(({ pack }) => pack.moduleType === 'feature').map(({ pack }) => pack.id));
  for (const { pack } of selectedPacks.filter(({ pack }) => pack.moduleType === 'specification')) {
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      for (const featureId of fragment.requiresFeatures ?? []) {
        if (!featureModuleIds.has(featureId)) {
          fail(`${pack.id}.${fragment.id}.requiresFeatures`,
            `references missing feature module ${featureId}`);
        }
      }
    }
    for (const check of pack.checks) {
      for (const featureId of check.requiresFeatures) {
        if (!featureModuleIds.has(featureId)) {
          fail(`${pack.id}.${check.id}.requiresFeatures`,
            `references missing feature module ${featureId}`);
        }
      }
    }
  }
  if (recipe.state === 'qualified') {
    if (fixture.state !== 'qualified') {
      fail(`${recipeSource}.fixture`, `qualified recipe selects ${fixture.state} fixture ${fixture.id}@${fixture.version}`);
    }
    for (const { pack } of selectedPacks) {
      if (pack.state !== 'qualified') {
        fail(`${recipeSource}.packs`, `qualified recipe selects ${pack.state} pack ${pack.id}@${pack.version}`);
      }
    }
  }
  const visitState = new Map();
  const visit = (ref, chain = []) => {
    if (visitState.get(ref) === 'done') return;
    if (visitState.get(ref) === 'visiting') {
      fail(`${recipeSource}.packs`, `dependency cycle: ${[...chain, ref].join(' -> ')}`);
    }
    visitState.set(ref, 'visiting');
    const pack = selectedByRef.get(ref);
    for (const required of pack.requiresPacks) visit(required, [...chain, ref]);
    visitState.set(ref, 'done');
  };
  for (const ref of selectedByRef.keys()) visit(ref);

  const fragmentSources = new Map();
  const composedFragments = { requirements: new Map(), contracts: new Map() };
  const addFragment = (kind, fragment, owner) => {
    if (!fragment.modes.includes(recipe.task.mode)) return;
    const at = `${owner}.task.${kind}.${fragment.id}`;
    const definition = resolveTaskFragment(fragment, { trackRoot: root, source: at,
      sourceCache: fragmentSources });
    const current = composedFragments[kind].get(fragment.id);
    if (current) {
      const comparable = value => JSON.stringify({ ...value, owners: undefined });
      if (comparable(current) !== comparable(definition)) {
        fail(at, `shared fragment ${fragment.id} does not match its other owner`);
      }
      current.owners.push(owner);
      return;
    }
    composedFragments[kind].set(fragment.id, { ...definition, owners: [owner] });
  };
  for (const fragment of recipe.task.framing.requirements) addFragment('requirements', fragment, 'recipe');
  for (const fragment of recipe.task.framing.contracts) addFragment('contracts', fragment, 'recipe');
  for (const { pack } of selectedPacks) {
    for (const fragment of pack.task.requirements) addFragment('requirements', fragment, pack.id);
    for (const fragment of pack.task.contracts) addFragment('contracts', fragment, pack.id);
  }
  const orderedFragments = kind => [...composedFragments[kind].values()]
    .sort((left, right) => left.order - right.order || left.id.localeCompare(right.id))
    .map(fragment => ({ ...fragment, owners: [...fragment.owners].sort() }));
  const taskRequirements = orderedFragments('requirements');
  const taskContracts = orderedFragments('contracts');
  if (taskRequirements.length === 0) fail(`${recipeSource}.task`, 'composes no requirements');

  const scenarioCache = new Map();
  const selectedFeatures = [];
  const claimedCriteria = new Set();
  const actionsForSteps = steps => {
    const actions = [];
    for (const step of steps) {
      actions.push(step.do);
      if (step.do === 'race') {
        for (const branch of step.branches) actions.push(...actionsForSteps(branch));
      }
    }
    return actions;
  };
  for (const { selection, pack, path } of selectedPacks) {
    const roles = new Set(selection.includeRoles);
    const included = pack.checks.filter(check => roles.has(check.role));
    for (const role of roles) {
      if (!included.some(check => check.role === role)) {
        fail(`${recipeSource}.packs.${pack.id}.includeRoles`, `pack has no ${role} check groups`);
      }
    }
    for (const check of included) {
      const scenarioRef = contained(root, root, check.source, `${path}.${check.id}.source`);
      let scenario = scenarioCache.get(scenarioRef.relative);
      if (!scenario) {
        scenario = compileScenarioDefinition(readJson(scenarioRef.absolute, 'scenario'), {
          source: scenarioRef.relative,
        });
        scenarioCache.set(scenarioRef.relative, scenario);
      }
      const feature = scenario.features.find(candidate => candidate.id === check.feature);
      if (!feature) fail(`${path}.${check.id}.feature`, `feature ${check.feature} not found in ${check.source}`);
      const criteria = check.criteria === undefined
        ? feature.criteria
        : check.criteria.map(criterionId => {
          const criterion = feature.criteria.find(candidate => candidate.id === criterionId);
          if (!criterion) fail(`${path}.${check.id}.criteria`,
            `criterion ${criterionId} not found in ${check.source} feature ${check.feature}`);
          return criterion;
        });
      for (const criterion of criteria) {
        const criterionRef = `${scenarioRef.relative}#${check.feature}#${criterion.id}`;
        if (claimedCriteria.has(criterionRef)) {
          fail(`${path}.${check.id}`, `criterion already selected: ${criterionRef}`);
        }
        claimedCriteria.add(criterionRef);
      }
      const selectedFeature = { ...feature, criteria };
      selectedFeatures.push({
        packId: pack.id,
        packVersion: pack.version,
        ...(pack.moduleType === undefined ? {} : { moduleType: pack.moduleType }),
        // `id` identifies this source slice inside the pack. `stableId` keeps
        // its published score keys unchanged when one criterion moves to a
        // focused, independently versioned scenario.
        checkGroupId: check.stableId ?? check.id,
        role: check.role,
        ...(check.observations === undefined ? {} : { observations: check.observations }),
        ...(check.requiresFeatures === undefined ? {} : { requiresFeatures: check.requiresFeatures }),
        source: scenarioRef.relative,
        feature: selectedFeature,
        actions: [...new Set([
          ...actionsForSteps(selectedFeature.setup),
          ...selectedFeature.criteria.flatMap(criterion => actionsForSteps(criterion.steps)),
        ])].sort(),
      });
    }
  }

  const bySource = new Map();
  for (const selected of selectedFeatures) {
    if (!bySource.has(selected.source)) bySource.set(selected.source, []);
    bySource.get(selected.source).push(selected);
  }
  const execution = recipe.execution.map(entry => {
    const normalized = contained(root, root, entry.source, `${recipeSource}.execution.${entry.id}.source`).relative;
    const selected = bySource.get(normalized);
    if (!selected) fail(`${recipeSource}.execution.${entry.id}`, `source has no selected check groups: ${normalized}`);
    bySource.delete(normalized);
    const scenario = scenarioCache.get(normalized);
    const scenarioOrder = scenario.features.map(feature => feature.id);
    const criterionOrder = featureId => scenario.features.find(feature => feature.id === featureId)
      .criteria.map(criterion => criterion.id);
    return { id: entry.id, source: normalized, checkGroups: selected.slice()
      .sort((a, b) => scenarioOrder.indexOf(a.feature.id) - scenarioOrder.indexOf(b.feature.id)
        || criterionOrder(a.feature.id).indexOf(a.feature.criteria[0].id)
          - criterionOrder(b.feature.id).indexOf(b.feature.criteria[0].id)) };
  });
  if (bySource.size) {
    fail(`${recipeSource}.execution`, `missing selected sources: ${[...bySource.keys()].sort().join(', ')}`);
  }

  const checks = [];
  const stableKeys = new Set();
  for (const suite of execution) {
    for (const group of suite.checkGroups) {
      for (const criterion of group.feature.criteria) {
        const stableKey = `${group.packId}.${group.checkGroupId}.${criterion.id}`;
        if (stableKeys.has(stableKey)) fail(recipeSource, `duplicate stable check key ${stableKey}`);
        stableKeys.add(stableKey);
        checks.push({
          stableKey,
          packId: group.packId,
          checkGroupId: group.checkGroupId,
          criterionId: criterion.id,
          role: group.role,
          ...(group.observations === undefined ? {} : { observations: group.observations }),
          ...(group.requiresFeatures === undefined ? {} : { requiresFeatures: group.requiresFeatures }),
          source: group.source,
          featureId: group.feature.id,
          description: criterion.desc,
          sourcePoints: criterion.points ?? 1,
        });
      }
    }
  }
  if (recipe.scoring.mode === 'explicit') {
    const weightKeys = new Set(Object.keys(recipe.scoring.weights));
    for (const key of stableKeys) if (!weightKeys.delete(key)) fail(`${recipeSource}.scoring.weights`, `missing ${key}`);
    if (weightKeys.size) fail(`${recipeSource}.scoring.weights`, `unknown checks: ${[...weightKeys].sort().join(', ')}`);
  }
  for (const check of checks) {
    check.points = recipe.scoring.mode === 'explicit'
      ? recipe.scoring.weights[check.stableKey] : check.sourcePoints;
  }

  const capabilities = [...new Set(selectedPacks.flatMap(({ pack }) => pack.capabilities))].sort();
  if (availableCapabilities !== null) {
    const available = new Set(availableCapabilities);
    const missing = capabilities.filter(capability => !available.has(capability));
    if (missing.length) fail(`${recipeSource}.packs`, `unsupported capabilities: ${missing.join(', ')}`);
  }
  return {
    compositionSchemaVersion: COMPOSITION_SCHEMA_VERSION,
    recipe: {
      id: recipe.id,
      version: recipe.version,
      state: recipe.state,
      title: recipe.title,
      track: recipe.track,
      compatibility: recipe.compatibility ?? null,
      task: {
        mode: recipe.task.mode,
        baseRecipe,
        requirements: taskRequirements,
        contracts: taskContracts,
        requirementText: taskRequirements.map(fragment => fragment.text).join(''),
        contractText: taskContracts.map(fragment => fragment.text).join(''),
      },
    },
    fixture,
    packs: selectedPacks.map(({ selection, pack, path }) => ({
      id: pack.id,
      version: pack.version,
      state: pack.state,
      title: pack.title,
      ...(pack.moduleType === undefined ? {} : { moduleType: pack.moduleType }),
      path,
      includeRoles: selection.includeRoles,
      requiresPacks: pack.requiresPacks,
      capabilities: pack.capabilities,
      evidence: pack.evidence,
      budget: pack.budget,
      task: {
        requirementIds: pack.task.requirements.filter(fragment => fragment.modes.includes(recipe.task.mode))
          .map(fragment => fragment.id),
        contractIds: pack.task.contracts.filter(fragment => fragment.modes.includes(recipe.task.mode))
          .map(fragment => fragment.id),
      },
      actions: [...new Set(selectedFeatures.filter(feature => feature.packId === pack.id)
        .flatMap(feature => feature.actions))].sort(),
    })),
    capabilities,
    execution,
    checks,
    scoring: {
      mode: recipe.scoring.mode,
      checks: checks.length,
      points: checks.reduce((total, check) => total + check.points, 0),
    },
  };
}

const PROMOTION_FIELDS = new Set([
  'schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'entries',
]);
const PROMOTION_ENTRY_FIELDS = new Set(['alias', 'status', 'recipe']);

export function compilePromotionDefinition(input, { source = '<promotion-catalog>' } = {}) {
  const catalog = structuredClone(input);
  strictObject(catalog, source, PROMOTION_FIELDS);
  identityFields(catalog, source, 'promotion-catalog');
  if (!Array.isArray(catalog.entries) || catalog.entries.length === 0) {
    fail(`${source}.entries`, 'must be a non-empty array');
  }
  const activeAliases = new Set();
  const candidates = new Set();
  catalog.entries.forEach((entry, index) => {
    const at = `${source}.entries[${index}]`;
    strictObject(entry, at, PROMOTION_ENTRY_FIELDS);
    string(entry.alias, `${at}.alias`);
    if (!/^L[1-9]\d*$/.test(entry.alias)) fail(`${at}.alias`, 'must look like L1, L2, and so on');
    if (!['candidate', 'promoted', 'retired'].includes(entry.status)) {
      fail(`${at}.status`, 'must be candidate, promoted, or retired');
    }
    validateFileRef(entry.recipe, `${at}.recipe`);
    if (entry.status === 'promoted') {
      if (activeAliases.has(entry.alias)) fail(`${at}.alias`, `duplicate promoted alias ${entry.alias}`);
      activeAliases.add(entry.alias);
    }
    const candidate = `${entry.alias}:${entry.recipe.id}@${entry.recipe.version}`;
    if (candidates.has(candidate)) fail(at, `duplicate alias target ${candidate}`);
    candidates.add(candidate);
  });
  return catalog;
}

export function compilePromotionFile(catalogPath, { trackRoot } = {}) {
  const absoluteCatalog = resolve(catalogPath);
  const root = resolve(trackRoot ?? dirname(dirname(absoluteCatalog)));
  const compositionRoot = resolve(root, 'composition');
  const source = relative(root, absoluteCatalog).replaceAll('\\', '/');
  const catalog = compilePromotionDefinition(readJson(absoluteCatalog, 'promotion catalog'), { source });
  const entries = catalog.entries.map((entry, index) => {
    const at = `${source}.entries[${index}].recipe`;
    const ref = contained(compositionRoot, dirname(absoluteCatalog), entry.recipe.path, `${at}.path`);
    const plan = compileRecipeFile(ref.absolute, { trackRoot: root });
    if (plan.recipe.id !== entry.recipe.id || plan.recipe.version !== entry.recipe.version) {
      fail(at, `expected ${entry.recipe.id}@${entry.recipe.version}, found ${plan.recipe.id}@${plan.recipe.version}`);
    }
    if (entry.status === 'promoted' && plan.recipe.state !== 'qualified') {
      fail(at, `cannot promote ${plan.recipe.id}@${plan.recipe.version} while it is ${plan.recipe.state}`);
    }
    if (entry.status !== 'retired' && plan.recipe.state === 'retired') {
      fail(at, `cannot use retired recipe ${plan.recipe.id}@${plan.recipe.version} as ${entry.status}`);
    }
    return { alias: entry.alias, status: entry.status, recipe: {
      id: plan.recipe.id, version: plan.recipe.version, path: ref.relative,
    } };
  });
  return {
    compositionSchemaVersion: COMPOSITION_SCHEMA_VERSION,
    catalog: { id: catalog.id, version: catalog.version, state: catalog.state, title: catalog.title },
    entries,
  };
}
