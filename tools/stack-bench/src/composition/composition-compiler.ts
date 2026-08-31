import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { dirname, relative, resolve, sep } from 'node:path';

import { compileScenarioDefinition } from './definition-compiler.js';

type UnknownRecord = Record<string, unknown>;

// Each compiler validates a cloned record field by field and returns that same
// record. The intersection is what lets the proven record be read back as the
// shape it was proven to be.
type Validated<T> = T & UnknownRecord;

import type { CompiledFeature, CompiledScenarioDefinition, CompiledStep } from './definition-compiler.js';

export interface TaskFragmentDefinition {
  id: string;
  path: string;
  order: number;
  from?: string;
  until?: string;
  modes?: string[];
  requiresFeatures?: string[];
}

export interface CompiledTaskFragment
  extends Omit<TaskFragmentDefinition, 'from' | 'until' | 'modes'> {
  from: string | null;
  until: string | null;
  modes: string[];
  text: string;
}

export interface CompiledOwnedTaskFragment extends CompiledTaskFragment {
  owners: string[];
  ownerConditions?: Array<{
    owner: string;
    modes: string[];
    requiresFeatures: string[];
  }>;
  requiresFeatures?: string[];
}

export interface PackCheck {
  id: string;
  stableId?: string;
  source: string;
  feature: number;
  criteria?: string[];
  role: string;
  observations?: string[];
  requiresFeatures?: string[];
}

export type CompiledPackBudget =
  | { status: 'unmeasured'; maxRuntimeMs?: never }
  | { status: 'bounded'; maxRuntimeMs: number };

export interface CompiledPackDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  stableId?: string;
  version: string;
  state: string;
  title: string;
  description?: string;
  moduleType?: string;
  requiresPacks: string[];
  conflictsWith: string[];
  capabilities: string[];
  evidence: string[];
  budget: CompiledPackBudget;
  task: ValidatedTaskFragmentSet;
  checks: PackCheck[];
}

export interface FixtureItem {
  name: string;
  price: string;
  category: string;
  stock: Record<string, number>;
}

export interface FixtureAccount {
  username: string;
  password: string;
  roles: string[];
}

export interface CompiledFixtureDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  warehouses: string[];
  items: FixtureItem[];
  accounts: FixtureAccount[];
  empty: string[];
}

export interface SelectedCheckGroup {
  packId: string;
  packVersion: string;
  stablePackId?: string;
  moduleType?: string;
  checkGroupId: string;
  role: string;
  observations?: string[];
  requiresFeatures?: string[];
  source: string;
  feature: CompiledFeature;
  actions: string[];
}

export interface SelectedCheck {
  stableKey: string;
  packId: string;
  stablePackId?: string;
  checkGroupId: string;
  criterionId: string;
  role: string;
  observations?: string[];
  requiresFeatures?: string[];
  source: string;
  featureId: number;
  description: string;
  sourcePoints: number;
  points: number;
}

export interface CompiledRecipePlan {
  compositionSchemaVersion: number;
  recipe: {
    id: string;
    version: string;
    state: string;
    title: string;
    track: string;
    sequence: { level: number } | null;
    task: {
      mode: string;
      baseRecipe: { id: string; version: string; path: string } | null;
      requirements: CompiledOwnedTaskFragment[];
      contracts: CompiledOwnedTaskFragment[];
      requirementText: string;
      contractText: string;
    };
  };
  fixture: CompiledFixtureDefinition;
  packs: Array<{
    id: string;
    stableId?: string;
    version: string;
    state: string;
    title: string;
    moduleType?: string;
    path: string;
    includeRoles: string[];
    includeCheckGroups?: string[];
    requiresPacks: string[];
    capabilities: string[];
    evidence: string[];
    budget: CompiledPackBudget;
    task: { requirementIds: string[]; contractIds: string[] };
    actions: string[];
  }>;
  capabilities: string[];
  execution: Array<{ id: string; source: string; checkGroups: SelectedCheckGroup[] }>;
  checks: SelectedCheck[];
  scoring: { mode: string; checks: number; points: number };
}

export interface PromotionEntry {
  alias: string;
  status: string;
  recipe: { id: string; version: string; path: string };
}

export interface CompiledPromotionDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  entries: PromotionEntry[];
}

export interface CompiledPromotionCatalog {
  compositionSchemaVersion: number;
  catalog: { id: string; version: string; state: string; title: string };
  entries: PromotionEntry[];
}

export type CompiledRecipeRelease = CompiledRecipePlan;

export const COMPOSITION_SCHEMA_VERSION = 1;

const ID = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/;
const STATES = new Set(['draft', 'qualified', 'retired']);
const ROLES = new Set(['feature', 'guarantee', 'control']);
const MODULE_TYPES = new Set(['feature', 'specification']);
const BUDGET_STATUSES = new Set(['unmeasured', 'bounded']);
const OBSERVATIONS = new Set(['requested', 'unmentioned']);

const isMember = (allowed: Set<string>, value: unknown): boolean =>
  typeof value === 'string' && allowed.has(value);

const isOneOf = (value: unknown, allowed: readonly string[]): boolean =>
  typeof value === 'string' && allowed.includes(value);

const isObject = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
function fail(at: string, message: string): never {
  throw new Error(`invalid benchmark composition at ${at}: ${message}`);
}

function strictObject(value: unknown, at: string,
  allowed: Set<string>): asserts value is UnknownRecord {
  if (!isObject(value)) fail(at, 'must be an object');
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) fail(`${at}.${key}`, 'unknown field');
  }
}

function string(value: unknown, at: string,
  { nonEmpty = true }: { nonEmpty?: boolean } = {}): string {
  if (typeof value !== 'string' || (nonEmpty && value.trim().length === 0)) {
    fail(at, `must be a${nonEmpty ? ' non-empty' : ''} string`);
  }
  return value;
}

function id(value: unknown, at: string): string {
  const text = string(value, at);
  if (!ID.test(text)) fail(at, 'must contain lowercase letters, numbers, dots, dashes, or underscores');
  return text;
}

function version(value: unknown, at: string): string {
  const text = string(value, at);
  if (!VERSION.test(text)) fail(at, 'must be an exact semantic version');
  return text;
}

function exactRef(value: unknown, at: string): string {
  const text = string(value, at);
  const split = text.lastIndexOf('@');
  if (split < 1) fail(at, 'must be an exact id@version reference');
  id(text.slice(0, split), `${at} id`);
  version(text.slice(split + 1), `${at} version`);
  return text;
}

function uniqueStrings(value: unknown, at: string): string[] {
  if (!Array.isArray(value)) fail(at, 'must be an array');
  const seen = new Set<string>();
  return value.map((item, index) => {
    const text = string(item, `${at}[${index}]`);
    if (seen.has(text)) fail(`${at}[${index}]`, `duplicates ${JSON.stringify(text)}`);
    seen.add(text);
    return text;
  });
}

function identityFields(value: UnknownRecord, at: string, kind: string): void {
  if (value.schemaVersion !== COMPOSITION_SCHEMA_VERSION) {
    fail(`${at}.schemaVersion`, `must be ${COMPOSITION_SCHEMA_VERSION}`);
  }
  if (value.kind !== kind) fail(`${at}.kind`, `must be ${JSON.stringify(kind)}`);
  id(value.id, `${at}.id`);
  version(value.version, `${at}.version`);
  if (!isMember(STATES, value.state)) fail(`${at}.state`, 'must be draft, qualified, or retired');
  string(value.title, `${at}.title`);
}

function readJson(path: string, label: string): unknown {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`cannot read ${label} ${path}: ${message}`, { cause: error });
  }
}

function contained(root: string, from: string, path: unknown,
  at: string): { absolute: string; relative: string } {
  const text = string(path, at);
  const lexicalRoot = resolve(root);
  const candidate = resolve(from, text);
  const lexicalRelative = relative(lexicalRoot, candidate);
  if (lexicalRelative === '..' || lexicalRelative.startsWith(`..${sep}`)) {
    fail(at, `escapes ${lexicalRoot}`);
  }
  if (!existsSync(candidate)) fail(at, `does not exist: ${text}`);
  const rootPath = realpathSync(lexicalRoot);
  const target = realpathSync(candidate);
  const rel = relative(rootPath, target);
  if (rel === '..' || rel.startsWith(`..${sep}`)) fail(at, `escapes ${rootPath}`);
  return { absolute: target, relative: rel.replaceAll('\\', '/') };
}

const PACK_FIELDS = new Set([
  'schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'description',
  'moduleType', 'stableId', 'requiresPacks', 'conflictsWith', 'capabilities', 'evidence', 'budget',
  'task', 'checks',
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
const RECIPE_TASK_MODES = new Set([...TASK_MODES, 'action']);

type ValidatedTaskFragment = {
  id: string;
  path: string;
  order: number;
  from?: string;
  until?: string;
  modes: string[];
  requiresFeatures?: string[];
};

type ValidatedTaskFragmentSet = {
  requirements: ValidatedTaskFragment[];
  contracts: ValidatedTaskFragment[];
};

function taskFragmentDefinition(fragment: unknown, at: string): ValidatedTaskFragment {
  strictObject(fragment, at, TASK_FRAGMENT_FIELDS);
  id(fragment.id, `${at}.id`);
  string(fragment.path, `${at}.path`);
  if (!Number.isInteger(fragment.order) || Number(fragment.order) < 0) {
    fail(`${at}.order`, 'must be a non-negative integer');
  }
  if (fragment.from !== undefined) string(fragment.from, `${at}.from`);
  if (fragment.until !== undefined) string(fragment.until, `${at}.until`);
  const modes = uniqueStrings(fragment.modes ?? ['fresh', 'upgrade'], `${at}.modes`);
  fragment.modes = modes;
  if (modes.length === 0) fail(`${at}.modes`, 'must not be empty');
  for (const mode of modes) {
    if (!isMember(TASK_MODES, mode)) fail(`${at}.modes`, `unknown task mode ${mode}`);
  }
  if (fragment.requiresFeatures !== undefined) {
    const requiresFeatures = uniqueStrings(fragment.requiresFeatures, `${at}.requiresFeatures`).sort();
    fragment.requiresFeatures = requiresFeatures;
    if (requiresFeatures.length === 0) fail(`${at}.requiresFeatures`, 'must not be empty');
    for (const featureId of requiresFeatures) id(featureId, `${at}.requiresFeatures`);
  }
  return fragment as ValidatedTaskFragment;
}

function taskFragmentSet(value: unknown, at: string): ValidatedTaskFragmentSet {
  strictObject(value, at, PACK_TASK_FIELDS);
  const seen = new Set<string>();
  const compile = (fragments: unknown, kind: string): ValidatedTaskFragment[] => {
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

export function resolveTaskFragment(fragmentInput: unknown,
  { trackRoot, source = '<task-fragment>', sourceCache = new Map<string, string>() }:
    { trackRoot: string; source?: string; sourceCache?: Map<string, string> }):
  CompiledTaskFragment {
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

export function compilePackDefinition(input: unknown,
  { source = '<pack>' }: { source?: string } = {}): CompiledPackDefinition {
  const pack = structuredClone(input);
  strictObject(pack, source, PACK_FIELDS);
  identityFields(pack, source, 'test-pack');
  if (pack.stableId !== undefined) id(pack.stableId, `${source}.stableId`);
  if (pack.moduleType !== undefined && !isMember(MODULE_TYPES, pack.moduleType)) {
    fail(`${source}.moduleType`, 'must be feature or specification');
  }
  if (pack.description !== undefined) string(pack.description, `${source}.description`);
  const requiresPacks = uniqueStrings(pack.requiresPacks ?? [], `${source}.requiresPacks`);
  const conflictsWith = uniqueStrings(pack.conflictsWith ?? [], `${source}.conflictsWith`);
  pack.requiresPacks = requiresPacks;
  pack.conflictsWith = conflictsWith;
  requiresPacks.forEach((ref, index) => exactRef(ref, `${source}.requiresPacks[${index}]`));
  conflictsWith.forEach((ref, index) => exactRef(ref, `${source}.conflictsWith[${index}]`));
  const capabilities = uniqueStrings(pack.capabilities ?? [], `${source}.capabilities`);
  pack.capabilities = capabilities;
  if (capabilities.length === 0) fail(`${source}.capabilities`, 'must not be empty');
  const evidence = uniqueStrings(pack.evidence ?? [], `${source}.evidence`);
  pack.evidence = evidence;
  if (evidence.length === 0) fail(`${source}.evidence`, 'must not be empty');
  const budget = pack.budget;
  strictObject(budget, `${source}.budget`, BUDGET_FIELDS);
  if (!isMember(BUDGET_STATUSES, budget.status)) {
    fail(`${source}.budget.status`, 'must be unmeasured or bounded');
  }
  if (budget.status === 'bounded') {
    if (!Number.isInteger(budget.maxRuntimeMs) || Number(budget.maxRuntimeMs) < 1) {
      fail(`${source}.budget.maxRuntimeMs`, 'must be a positive integer for a bounded budget');
    }
  } else if (budget.maxRuntimeMs !== undefined) {
    fail(`${source}.budget.maxRuntimeMs`, 'is allowed only for a bounded budget');
  }
  if (pack.state === 'qualified' && budget.status !== 'bounded') {
    fail(`${source}.budget`, 'qualified packs require a bounded runtime budget');
  }
  const task = taskFragmentSet(pack.task, `${source}.task`);
  pack.task = task;
  if (task.requirements.length === 0) {
    fail(`${source}.task.requirements`, 'must not be empty');
  }
  const checks = pack.checks;
  if (!Array.isArray(checks) || checks.length === 0) {
    fail(`${source}.checks`, 'must be a non-empty array');
  }
  const taskFragments = [...task.requirements, ...task.contracts];
  if (pack.moduleType === 'feature'
    && taskFragments.some(fragment => fragment.requiresFeatures !== undefined)) {
    fail(`${source}.task`, 'feature module fragments cannot declare specification applicability');
  }
  if (pack.moduleType === 'specification'
    && task.requirements.some(fragment => fragment.requiresFeatures === undefined)) {
    fail(`${source}.task.requirements`,
      'specification requirement fragments must declare applicable feature modules');
  }
  const checkIds = new Set<string>();
  const validatedChecks = checks.map((check: unknown, index: number) => {
    const at = `${source}.checks[${index}]`;
    strictObject(check, at, CHECK_REF_FIELDS);
    const checkId = id(check.id, `${at}.id`);
    if (checkIds.has(checkId)) fail(`${at}.id`, `duplicate check group ${checkId}`);
    checkIds.add(checkId);
    if (check.stableId !== undefined) id(check.stableId, `${at}.stableId`);
    string(check.source, `${at}.source`);
    if (!Number.isInteger(check.feature) || Number(check.feature) < 1) {
      fail(`${at}.feature`, 'must be a positive integer');
    }
    if (check.criteria !== undefined) {
      const criteria = uniqueStrings(check.criteria, `${at}.criteria`);
      check.criteria = criteria;
      if (criteria.length === 0) fail(`${at}.criteria`, 'must not be empty');
      for (const criterion of criteria) id(criterion, `${at}.criteria`);
    }
    if (!isMember(ROLES, check.role)) fail(`${at}.role`, 'must be feature, guarantee, or control');
    if (check.observations !== undefined) {
      const observations = uniqueStrings(check.observations, `${at}.observations`).sort();
      check.observations = observations;
      if (observations.length === 0) fail(`${at}.observations`, 'must not be empty');
      for (const observation of observations) {
        if (!isMember(OBSERVATIONS, observation)) {
          fail(`${at}.observations`, `unknown observation class ${observation}`);
        }
      }
    }
    if (check.requiresFeatures !== undefined) {
      const requiresFeatures = uniqueStrings(check.requiresFeatures, `${at}.requiresFeatures`).sort();
      check.requiresFeatures = requiresFeatures;
      if (requiresFeatures.length === 0) fail(`${at}.requiresFeatures`, 'must not be empty');
      for (const featureId of requiresFeatures) id(featureId, `${at}.requiresFeatures`);
    }
    return check;
  });
  if (pack.moduleType === 'feature') {
    if (validatedChecks.some(check => check.role === 'guarantee')) {
      fail(`${source}.checks`, 'feature modules cannot own guarantee checks');
    }
    if (validatedChecks.some(check => Array.isArray(check.observations)
      && check.observations.includes('unmentioned'))) {
      fail(`${source}.checks`, 'feature modules cannot own evaluations without prompting');
    }
    // A feature check can need another feature to prepare its grading scenario.
    // This does not add a product dependency.
  }
  if (pack.moduleType === 'specification') {
    if (validatedChecks.some(check => check.role === 'feature')) {
      fail(`${source}.checks`, 'specification modules cannot own feature checks');
    }
    if (validatedChecks.some(check => check.observations === undefined)) {
      fail(`${source}.checks`, 'specification modules must declare prompted and/or unprompted evaluation');
    }
    if (validatedChecks.some(check => check.requiresFeatures === undefined)) {
      fail(`${source}.checks`, 'specification checks must declare applicable feature modules');
    }
  }
  return pack as Validated<CompiledPackDefinition>;
}

const FIXTURE_FIELDS = new Set([
  'schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'warehouses',
  'items', 'accounts', 'empty',
]);
const ITEM_FIELDS = new Set(['name', 'price', 'category', 'stock']);
const ACCOUNT_FIELDS = new Set(['username', 'password', 'roles']);

export function compileFixtureDefinition(input: unknown,
  { source = '<fixture>' }: { source?: string } = {}): CompiledFixtureDefinition {
  const fixture = structuredClone(input);
  strictObject(fixture, source, FIXTURE_FIELDS);
  identityFields(fixture, source, 'fixture-set');
  const warehouses = uniqueStrings(fixture.warehouses, `${source}.warehouses`);
  fixture.warehouses = warehouses;
  if (warehouses.length === 0) fail(`${source}.warehouses`, 'must not be empty');
  const items = fixture.items;
  if (!Array.isArray(items) || items.length === 0) {
    fail(`${source}.items`, 'must be a non-empty array');
  }
  const itemNames = new Set<unknown>();
  items.forEach((item: unknown, index: number) => {
    const at = `${source}.items[${index}]`;
    strictObject(item, at, ITEM_FIELDS);
    const name = string(item.name, `${at}.name`);
    if (itemNames.has(name)) fail(`${at}.name`, `duplicate item ${name}`);
    itemNames.add(name);
    const price = string(item.price, `${at}.price`);
    if (!/^\d+\.\d{2}$/.test(price)) fail(`${at}.price`, 'must be a decimal string with two places');
    string(item.category, `${at}.category`);
    const stock = item.stock;
    strictObject(stock, `${at}.stock`, new Set(warehouses));
    for (const warehouse of warehouses) {
      if (!Number.isInteger(stock[warehouse]) || Number(stock[warehouse]) < 0) {
        fail(`${at}.stock.${warehouse}`, 'must be a non-negative integer');
      }
    }
    for (const warehouse of warehouses) {
      if (!(warehouse in stock)) fail(`${at}.stock.${warehouse}`, 'is required');
    }
  });
  const accounts = fixture.accounts;
  if (!Array.isArray(accounts)) fail(`${source}.accounts`, 'must be an array');
  const usernames = new Set<unknown>();
  accounts.forEach((account: unknown, index: number) => {
    const at = `${source}.accounts[${index}]`;
    strictObject(account, at, ACCOUNT_FIELDS);
    const username = string(account.username, `${at}.username`);
    if (usernames.has(username)) fail(`${at}.username`, `duplicate account ${username}`);
    usernames.add(username);
    string(account.password, `${at}.password`);
    const roles = uniqueStrings(account.roles, `${at}.roles`);
    account.roles = roles;
    if (roles.length === 0) fail(`${at}.roles`, 'must not be empty');
  });
  fixture.empty = uniqueStrings(fixture.empty ?? [], `${source}.empty`);
  return fixture as Validated<CompiledFixtureDefinition>;
}

const RECIPE_FIELDS = new Set([
  'schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'track',
  'fixture', 'task', 'packs', 'execution', 'scoring', 'sequence',
]);
const FILE_REF_FIELDS = new Set(['path', 'id', 'version']);
const TASK_FIELDS = new Set(['mode', 'baseRecipe', 'framing']);
const PACK_SELECTION_FIELDS = new Set(['path', 'id', 'version', 'includeRoles',
  'includeCheckGroups']);
const EXECUTION_FIELDS = new Set(['id', 'source']);
const SCORING_FIELDS = new Set(['mode', 'weights']);
const SEQUENCE_FIELDS = new Set(['level']);

type FileRef = { path: string; id: string; version: string };

type ValidatedRecipe = {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  track: string;
  fixture: FileRef;
  task: ({ mode: 'upgrade'; baseRecipe: FileRef }
    | { mode: 'fresh' | 'action'; baseRecipe?: undefined })
    & { framing: ValidatedTaskFragmentSet };
  packs: Array<FileRef & { includeRoles: string[]; includeCheckGroups?: string[] }>;
  execution: 'all-selected-sources' | Array<{ id: string; source: string }>;
  scoring: { mode: 'explicit'; weights: Record<string, number> }
    | { mode: 'source-points'; weights?: undefined };
  sequence?: { level: number };
} & UnknownRecord;

type FragmentKind = 'requirements' | 'contracts';

type ComposedFragment = CompiledOwnedTaskFragment;

type SelectedPack = {
  selection: ValidatedRecipe['packs'][number];
  pack: CompiledPackDefinition;
  path: string;
};

function validateFileRef(ref: unknown, at: string): asserts ref is FileRef {
  strictObject(ref, at, FILE_REF_FIELDS);
  string(ref.path, `${at}.path`);
  id(ref.id, `${at}.id`);
  version(ref.version, `${at}.version`);
}

export function compileRecipeDefinition(input: unknown,
  { source = '<recipe>' }: { source?: string } = {}): ValidatedRecipe {
  const recipe = structuredClone(input);
  strictObject(recipe, source, RECIPE_FIELDS);
  identityFields(recipe, source, 'benchmark-recipe');
  id(recipe.track, `${source}.track`);
  validateFileRef(recipe.fixture, `${source}.fixture`);
  const task = recipe.task;
  strictObject(task, `${source}.task`, TASK_FIELDS);
  if (!isMember(RECIPE_TASK_MODES, task.mode)) {
    fail(`${source}.task.mode`, 'must be fresh, upgrade, or action');
  }
  if (task.mode === 'upgrade') {
    validateFileRef(task.baseRecipe, `${source}.task.baseRecipe`);
  } else if (task.baseRecipe !== undefined) {
    fail(`${source}.task.baseRecipe`, 'is allowed only for upgrade recipes');
  }
  const scoring = recipe.scoring;
  if (task.mode !== 'action' && recipe.execution === 'all-selected-sources') {
    fail(source, 'automatic execution requires an action recipe');
  }
  const framing = taskFragmentSet(task.framing, `${source}.task.framing`);
  task.framing = framing;
  if (framing.requirements.length === 0) {
    fail(`${source}.task.framing.requirements`, 'must not be empty');
  }
  const packs = recipe.packs;
  if (!Array.isArray(packs) || packs.length === 0) fail(`${source}.packs`, 'must not be empty');
  const packIds = new Set<unknown>();
  packs.forEach((selection: unknown, index: number) => {
    const at = `${source}.packs[${index}]`;
    strictObject(selection, at, PACK_SELECTION_FIELDS);
    validateFileRef({ path: selection.path, id: selection.id, version: selection.version }, at);
    if (packIds.has(selection.id)) fail(`${at}.id`, `duplicate selected pack ${selection.id}`);
    packIds.add(selection.id);
    const includeRoles = uniqueStrings(selection.includeRoles, `${at}.includeRoles`);
    selection.includeRoles = includeRoles;
    for (const role of includeRoles) {
      if (!isMember(ROLES, role)) fail(`${at}.includeRoles`, `unknown role ${role}`);
    }
    if (selection.includeCheckGroups !== undefined) {
      selection.includeCheckGroups = uniqueStrings(selection.includeCheckGroups,
        `${at}.includeCheckGroups`);
    }
  });
  const execution = recipe.execution;
  if (execution !== 'all-selected-sources') {
    if (!Array.isArray(execution) || execution.length === 0) {
      fail(`${source}.execution`, 'must be a non-empty array or "all-selected-sources"');
    }
    const executionIds = new Set<unknown>();
    const executionSources = new Set<unknown>();
    execution.forEach((entry: unknown, index: number) => {
      const at = `${source}.execution[${index}]`;
      strictObject(entry, at, EXECUTION_FIELDS);
      string(entry.id, `${at}.id`);
      string(entry.source, `${at}.source`);
      if (executionIds.has(entry.id)) fail(`${at}.id`, `duplicate execution id ${entry.id}`);
      if (executionSources.has(entry.source)) fail(`${at}.source`, `duplicate execution source ${entry.source}`);
      executionIds.add(entry.id);
      executionSources.add(entry.source);
    });
  }
  strictObject(scoring, `${source}.scoring`, SCORING_FIELDS);
  if (!isOneOf(scoring.mode, ['source-points', 'explicit'])) {
    fail(`${source}.scoring.mode`, 'must be source-points or explicit');
  }
  const weights = scoring.weights;
  if (scoring.mode === 'explicit') {
    if (!isObject(weights)) fail(`${source}.scoring.weights`, 'must be an object');
    for (const [key, points] of Object.entries(weights)) {
      string(key, `${source}.scoring.weights key`);
      if (!Number.isInteger(points) || Number(points) < 0) {
        fail(`${source}.scoring.weights.${key}`, 'must be a non-negative integer');
      }
    }
  } else if (weights !== undefined) {
    fail(`${source}.scoring.weights`, 'is allowed only with explicit scoring');
  }
  const sequence = recipe.sequence;
  if (sequence !== undefined) {
    strictObject(sequence, `${source}.sequence`, SEQUENCE_FIELDS);
    if (!Number.isInteger(sequence.level) || Number(sequence.level) < 1) {
      fail(`${source}.sequence.level`, 'must be a positive integer');
    }
    if (Number(sequence.level) === 1 && task.mode !== 'fresh') {
      fail(`${source}.task.mode`, 'sequence level 1 must use fresh mode');
    }
    if (Number(sequence.level) > 1 && task.mode !== 'upgrade') {
      fail(`${source}.task.mode`, 'sequence levels after 1 must use upgrade mode');
    }
  } else if (task.mode !== 'action') {
    fail(`${source}.sequence`, 'is required for fresh and upgrade recipes');
  }
  return recipe as ValidatedRecipe;
}

export function compileRecipeFile(recipePath: string,
  { trackRoot, availableCapabilities = null, recipeStack = [] }:
    { trackRoot?: string; availableCapabilities?: string[] | null; recipeStack?: string[] } = {}):
  CompiledRecipePlan {
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
  let baseRecipe: { id: string; version: string; path: string } | null = null;
  if (recipe.task.mode === 'upgrade') {
    const at = `${recipeSource}.task.baseRecipe`;
    const base = recipe.task.baseRecipe;
    const ref = contained(compositionRoot, dirname(absoluteRecipe), base.path, `${at}.path`);
    const plan = compileRecipeFile(ref.absolute, { trackRoot: root, availableCapabilities,
      recipeStack: [...recipeStack, absoluteRecipe] });
    if (plan.recipe.id !== base.id || plan.recipe.version !== base.version) {
      fail(at, `expected ${base.id}@${base.version}, found ${plan.recipe.id}@${plan.recipe.version}`);
    }
    if (recipe.state === 'qualified' && plan.recipe.state !== 'qualified') {
      fail(at, `qualified upgrade recipe selects ${plan.recipe.state} base ${plan.recipe.id}@${plan.recipe.version}`);
    }
    baseRecipe = { id: plan.recipe.id, version: plan.recipe.version, path: ref.relative };
  }

  const selectedPacks: SelectedPack[] = [];
  const selectedByRef = new Map<string, CompiledPackDefinition>();
  for (const [index, selection] of recipe.packs.entries()) {
    const at = `${recipeSource}.packs[${index}]`;
    const packRef = contained(compositionRoot, dirname(absoluteRecipe), selection.path, `${at}.path`);
    const pack = compilePackDefinition(readJson(packRef.absolute, 'pack'), {
      source: relative(root, packRef.absolute).replaceAll('\\', '/'),
    });
    if (pack.id !== selection.id || pack.version !== selection.version) {
      fail(at, `expected ${selection.id}@${selection.version}, found ${pack.id}@${pack.version}`);
    }
    if (selection.includeRoles.length === 0
      && (recipe.task.mode !== 'action' || pack.moduleType === undefined)) {
      fail(`${at}.includeRoles`, 'can be empty only for an action catalog dependency');
    }
    const ref = `${pack.id}@${pack.version}`;
    for (const groupId of selection.includeCheckGroups ?? []) {
      const group = pack.checks.find(check => check.id === groupId);
      if (!group) {
        fail(`${at}.includeCheckGroups`, `unknown check group ${groupId}`);
      }
      if (!selection.includeRoles.includes(group.role)) {
        fail(`${at}.includeCheckGroups`, `check group ${groupId} has excluded role ${group.role}`);
      }
    }
    selectedByRef.set(ref, pack);
    selectedPacks.push({ selection, pack, path: packRef.relative });
  }
  for (const { pack } of selectedPacks) {
    for (const required of pack.requiresPacks) {
      const dependency = selectedByRef.get(required);
      if (!dependency) fail(`${pack.id}.requiresPacks`, `missing ${required}`);
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
  const missingFeatureModules = (requiresFeatures: readonly string[] | undefined): string[] =>
    (requiresFeatures ?? []).filter(featureId => !featureModuleIds.has(featureId));
  // Check groups scope grading. Feature dependencies scope prompt fragments.
  const omitForScopedSelection = (selection: { includeCheckGroups?: string[] },
    requiresFeatures: readonly string[] | undefined): boolean =>
    selection.includeCheckGroups !== undefined && missingFeatureModules(requiresFeatures).length > 0;
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
  const visitState = new Map<string, 'visiting' | 'done'>();
  const visit = (ref: string, chain: readonly string[] = []): void => {
    if (visitState.get(ref) === 'done') return;
    if (visitState.get(ref) === 'visiting') {
      fail(`${recipeSource}.packs`, `dependency cycle: ${[...chain, ref].join(' -> ')}`);
    }
    visitState.set(ref, 'visiting');
    const pack = selectedByRef.get(ref);
    if (!pack) fail(`${recipeSource}.packs`, `missing ${ref}`);
    for (const required of pack.requiresPacks) visit(required, [...chain, ref]);
    visitState.set(ref, 'done');
  };
  for (const ref of selectedByRef.keys()) visit(ref);

  const fragmentSources = new Map<string, string>();
  const composedFragments: Record<FragmentKind, Map<string, ComposedFragment>> = {
    requirements: new Map(), contracts: new Map(),
  };
  const moduleTypeById = new Map<string, string | undefined>(
    selectedPacks.map(({ pack }) => [pack.id, pack.moduleType]));
  const addFragment = (kind: FragmentKind, fragment: ValidatedTaskFragment, owner: string): void => {
    if (recipe.task.mode !== 'action' && !fragment.modes.includes(recipe.task.mode)) return;
    const at = `${owner}.task.${kind}.${fragment.id}`;
    const definition = resolveTaskFragment(fragment, { trackRoot: root, source: at,
      sourceCache: fragmentSources });
    const current = composedFragments[kind].get(fragment.id);
    if (current) {
      const comparable = (value: object): string => JSON.stringify({ ...value, owners: undefined,
        order: undefined, modes: undefined, requiresFeatures: undefined,
        ownerConditions: undefined });
      if (comparable(current) !== comparable(definition)) {
        fail(at, `shared fragment ${fragment.id} does not match its other owner`);
      }
      const condition = (value: { modes: string[]; requiresFeatures?: string[] }):
        { modes: string[]; requiresFeatures: string[] } => ({
        modes: value.modes,
        requiresFeatures: value.requiresFeatures ?? [],
      });
      const differs = JSON.stringify(condition(current)) !== JSON.stringify(condition(definition));
      if ((differs || current.order !== definition.order)
        && (moduleTypeById.get(owner) !== 'feature'
          || current.owners.some(existingOwner => moduleTypeById.get(existingOwner) !== 'feature'))) {
        fail(at, `shared fragment ${fragment.id} does not match its other owner`);
      }
      if (current.ownerConditions) {
        current.ownerConditions.push({ owner, ...condition(definition) });
      } else if (differs) {
        current.ownerConditions = current.owners.map(existingOwner => ({
          owner: existingOwner, ...condition(current),
        }));
        current.ownerConditions.push({ owner, ...condition(definition) });
        delete current.requiresFeatures;
      }
      current.order = Math.min(current.order, definition.order);
      current.modes = [...new Set([...current.modes, ...definition.modes])].sort();
      current.owners.push(owner);
      return;
    }
    composedFragments[kind].set(fragment.id, { ...definition, owners: [owner] });
  };
  for (const fragment of recipe.task.framing.requirements) addFragment('requirements', fragment, 'recipe');
  for (const fragment of recipe.task.framing.contracts) addFragment('contracts', fragment, 'recipe');
  for (const { pack, selection } of selectedPacks) {
    if (selection.includeRoles.length === 0) continue;
    for (const fragment of pack.task.requirements) {
      if (omitForScopedSelection(selection, fragment.requiresFeatures)) continue;
      const missing = missingFeatureModules(fragment.requiresFeatures);
      if (missing.length) {
        fail(`${pack.id}.${fragment.id}.requiresFeatures`,
          `references missing feature module ${missing.join(', ')}`);
      }
      addFragment('requirements', fragment, pack.id);
    }
    for (const fragment of pack.task.contracts) {
      if (omitForScopedSelection(selection, fragment.requiresFeatures)) continue;
      const missing = missingFeatureModules(fragment.requiresFeatures);
      if (missing.length) {
        fail(`${pack.id}.${fragment.id}.requiresFeatures`,
          `references missing feature module ${missing.join(', ')}`);
      }
      addFragment('contracts', fragment, pack.id);
    }
  }
  const orderedFragments = (kind: FragmentKind): ComposedFragment[] =>
    [...composedFragments[kind].values()]
      .sort((left, right) => left.order - right.order || left.id.localeCompare(right.id))
      .map(fragment => ({ ...fragment, owners: [...fragment.owners].sort() }));
  const taskRequirements = orderedFragments('requirements');
  const taskContracts = orderedFragments('contracts');
  if (taskRequirements.length === 0) fail(`${recipeSource}.task`, 'composes no requirements');

  const scenarioCache = new Map<string, CompiledScenarioDefinition>();
  const selectedFeatures: SelectedCheckGroup[] = [];
  const claimedCriteria = new Set<string>();
  const actionsForSteps = (steps: readonly CompiledStep[]): string[] => {
    const actions: string[] = [];
    for (const step of steps) {
      actions.push(step.do);
      if (step.do === 'race') {
        const branches = step.branches;
        if (Array.isArray(branches)) {
          for (const branch of branches) actions.push(...actionsForSteps(branch));
        }
      }
    }
    return actions;
  };
  for (const { selection, pack, path } of selectedPacks) {
    const roles = new Set(selection.includeRoles);
    const selectedGroups = selection.includeCheckGroups === undefined
      ? null : new Set(selection.includeCheckGroups);
    const included = pack.checks.filter(check => roles.has(check.role)
      && (selectedGroups === null || selectedGroups.has(check.id)));
    for (const role of roles) {
      if (!included.some(check => check.role === role)) {
        fail(`${recipeSource}.packs.${pack.id}.includeRoles`, `pack has no ${role} check groups`);
      }
    }
    for (const check of included) {
      const missing = missingFeatureModules(check.requiresFeatures);
      if (missing.length) {
        fail(`${pack.id}.${check.id}.requiresFeatures`,
          `references missing feature module ${missing.join(', ')}`);
      }
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
        ...(pack.stableId === undefined ? {} : { stablePackId: pack.stableId }),
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

  const bySource = new Map<string, SelectedCheckGroup[]>();
  for (const selected of selectedFeatures) {
    const group = bySource.get(selected.source);
    if (group) group.push(selected);
    else bySource.set(selected.source, [selected]);
  }
  const executionEntries = recipe.execution === 'all-selected-sources'
    ? [...bySource.keys()].sort().map((source, index) => ({
      id: `selected-source-${String(index + 1).padStart(3, '0')}`,
      source,
    }))
    : recipe.execution;
  const execution = executionEntries.map(entry => {
    const normalized = contained(root, root, entry.source, `${recipeSource}.execution.${entry.id}.source`).relative;
    const selected = bySource.get(normalized);
    if (!selected) fail(`${recipeSource}.execution.${entry.id}`, `source has no selected check groups: ${normalized}`);
    bySource.delete(normalized);
    const scenario = scenarioCache.get(normalized);
    if (!scenario) fail(`${recipeSource}.execution.${entry.id}`, `source was not compiled: ${normalized}`);
    const scenarioOrder = scenario.features.map(feature => feature.id);
    const criterionOrder = (featureId: number): string[] =>
      scenario.features.find(feature => feature.id === featureId)
        ?.criteria.map(criterion => criterion.id) ?? [];
    const firstCriterion = (group: SelectedCheckGroup): string =>
      group.feature.criteria[0]?.id ?? '';
    return { id: entry.id, source: normalized, checkGroups: selected.slice()
      .sort((a, b) => scenarioOrder.indexOf(a.feature.id) - scenarioOrder.indexOf(b.feature.id)
        || criterionOrder(a.feature.id).indexOf(firstCriterion(a))
          - criterionOrder(b.feature.id).indexOf(firstCriterion(b))) };
  });
  if (bySource.size) {
    fail(`${recipeSource}.execution`, `missing selected sources: ${[...bySource.keys()].sort().join(', ')}`);
  }

  const checks: Array<Omit<SelectedCheck, 'points'> & { points?: number }> = [];
  const stableKeys = new Set<string>();
  for (const suite of execution) {
    for (const group of suite.checkGroups) {
      for (const criterion of group.feature.criteria) {
        const stableKey = `${group.stablePackId ?? group.packId}.${group.checkGroupId}.${criterion.id}`;
        if (stableKeys.has(stableKey)) fail(recipeSource, `duplicate stable check key ${stableKey}`);
        stableKeys.add(stableKey);
        checks.push({
          stableKey,
          packId: group.packId,
          ...(group.stablePackId === undefined ? {} : { stablePackId: group.stablePackId }),
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
  const scoring = recipe.scoring;
  for (const check of checks) {
    check.points = scoring.mode === 'explicit'
      ? scoring.weights[check.stableKey] : check.sourcePoints;
  }
  const scoredChecks = checks as SelectedCheck[];

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
      sequence: recipe.sequence ?? null,
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
      ...(pack.stableId === undefined ? {} : { stableId: pack.stableId }),
      ...(pack.moduleType === undefined ? {} : { moduleType: pack.moduleType }),
      path,
      includeRoles: selection.includeRoles,
      ...(selection.includeCheckGroups === undefined
        ? {} : { includeCheckGroups: selection.includeCheckGroups }),
      requiresPacks: pack.requiresPacks,
      capabilities: pack.capabilities,
      evidence: pack.evidence,
      budget: pack.budget,
      task: {
        requirementIds: taskRequirements.filter(fragment => fragment.owners.includes(pack.id))
          .map(fragment => fragment.id),
        contractIds: taskContracts.filter(fragment => fragment.owners.includes(pack.id))
          .map(fragment => fragment.id),
      },
      actions: [...new Set(selectedFeatures.filter(feature => feature.packId === pack.id)
        .flatMap(feature => feature.actions))].sort(),
    })),
    capabilities,
    execution,
    checks: scoredChecks,
    scoring: {
      mode: scoring.mode,
      checks: scoredChecks.length,
      points: scoredChecks.reduce((total, check) => total + check.points, 0),
    },
  };
}

const PROMOTION_FIELDS = new Set([
  'schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'entries',
]);
const PROMOTION_ENTRY_FIELDS = new Set(['alias', 'status', 'recipe']);

export function compilePromotionDefinition(input: unknown,
  { source = '<promotion-catalog>' }: { source?: string } = {}): CompiledPromotionDefinition {
  const catalog = structuredClone(input);
  strictObject(catalog, source, PROMOTION_FIELDS);
  identityFields(catalog, source, 'promotion-catalog');
  if (!Array.isArray(catalog.entries)) fail(`${source}.entries`, 'must be an array');
  if (catalog.entries.length === 0 && catalog.state !== 'draft') {
    fail(`${source}.entries`, 'must be non-empty once the catalog is qualified');
  }
  const activeAliases = new Set<unknown>();
  const candidates = new Set<string>();
  catalog.entries.forEach((entry, index) => {
    const at = `${source}.entries[${index}]`;
    strictObject(entry, at, PROMOTION_ENTRY_FIELDS);
    const alias = string(entry.alias, `${at}.alias`);
    if (!/^L[1-9]\d*$/.test(alias)) fail(`${at}.alias`, 'must look like L1, L2, and so on');
    if (!isOneOf(entry.status, ['candidate', 'promoted', 'retired'])) {
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
  return catalog as Validated<CompiledPromotionDefinition>;
}

export function compilePromotionFile(catalogPath: string,
  { trackRoot }: { trackRoot?: string } = {}): CompiledPromotionCatalog {
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
