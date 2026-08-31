// Validate and normalize scenario definitions before resource acquisition.

type UnknownRecord = Record<string, unknown>;

export interface CompiledStep {
  do: string;
  action?: string;
  actor?: string;
  actors?: string[];
  as?: string;
  branches?: CompiledStep[][];
  count?: number;
  equals?: number | string[];
  from?: string;
  fromActor?: string;
  input?: UnknownRecord;
  item?: string;
  namedAction?: UnknownRecord;
  nonEmpty?: boolean;
  outcome?: string;
  plus?: number;
  quantity?: number;
  relativeTo?: string;
  settleMs?: number;
  testid?: string;
  text?: string;
  contains?: string;
  absent?: boolean;
  warehouse?: string;
  in?: {
    testid?: string;
    contains?: string;
    [key: string]: unknown;
  };
  [key: string]: unknown;
}

export interface CompiledCriterion {
  id: string;
  desc: string;
  note?: string;
  points: number;
  provenBy?: string;
  steps: CompiledStep[];
  statedBy?: string;
  withheld?: string;
  [key: string]: unknown;
}

export interface CompiledFeature {
  id: number;
  name: string;
  actors?: string[];
  max?: number;
  setup: CompiledStep[];
  criteria: CompiledCriterion[];
  [key: string]: unknown;
}

export interface CompiledScenarioDefinition {
  schemaVersion: number;
  level: number;
  features: CompiledFeature[];
  [key: string]: unknown;
}

export interface CompiledTrackSuite {
  id: string;
  inherit: 'none' | 'all-higher-levels';
  spec: string;
}

export interface CompiledTrackManifest extends Record<string, unknown> {
  schemaVersion: number;
  suites: Record<string, CompiledTrackSuite[]>;
  title?: string;
  slug?: string;
  internal?: boolean;
  validatedThrough?: number;
  plannedThrough?: number;
  portOffset?: number;
  restartProbe?: string;
  reseedOnReset?: boolean;
  databaseProvenance?: { action: string; markerParameter: string; body: Record<string, string> };
  actions?: unknown[];
}

export const DEFINITION_SCHEMA_VERSION = 1;

const string = (value: unknown): value is string => typeof value === 'string';
const nonEmptyString = (value: unknown): value is string =>
  string(value) && value.trim().length > 0;
const number = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value);
const integer = (value: unknown): value is number => Number.isInteger(value);
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const array = (value: unknown): value is unknown[] => Array.isArray(value);
const stringArray = (value: unknown): value is string[] =>
  array(value) && value.every(nonEmptyString);
const anyArray = (value: unknown): value is unknown[] => array(value);
const scalar = (value: unknown): boolean =>
  value === null || ['string', 'number', 'boolean'].includes(typeof value);
const relativePath = (value: unknown): boolean => nonEmptyString(value)
  && !/^(?:[A-Za-z]:[\\/]|[\\/])/.test(value)
  && !value.split(/[\\/]/).includes('..');
const processTimeout = (value: unknown): boolean =>
  number(value) && value > 0 && value <= 85_000;

type FieldPredicate = (value: unknown) => boolean;

interface ActionDefinition {
  required: Record<string, FieldPredicate>;
  fields: Record<string, FieldPredicate>;
}

const oneOf = (value: unknown, allowed: readonly string[]): boolean =>
  string(value) && allowed.includes(value);

function fields(required: Record<string, FieldPredicate>,
  optional: Record<string, FieldPredicate> = {}) {
  return { required, fields: Object.fromEntries([
    ['do', nonEmptyString],
    ...Object.entries(required),
    ...Object.entries(optional),
  ]) };
}

const actor = { actor: nonEmptyString };
const actors = { actors: stringArray };
const settle = { settleMs: number };
const within = { within: number };
const locator = { in: object };

export const ACTION_DEFINITIONS: Readonly<Record<string, ActionDefinition>> = Object.freeze({
  callAction: fields({ ...actor, action: nonEmptyString, input: object },
    { from: nonEmptyString, authentication: nonEmptyString, namedAction: object, ...settle }),
  callConcurrently: fields({ ...actors, action: nonEmptyString, settleMs: number },
    { args: anyArray, body: object }),
  clearInput: fields(actor),
  click: fields({ ...actor, testid: nonEmptyString },
    { contains: string, ...locator, ...settle }),
  clickConcurrently: fields({ ...actors, testid: nonEmptyString, settleMs: number },
    { ...locator, targets: anyArray, readyWithin: number, within: number }),
  closeClient: fields(actor),
  createRoom: fields({ ...actor, room: nonEmptyString }, { private: boolean }),
  dbSetStock: fields({ item: nonEmptyString, warehouse: nonEmptyString,
    quantity: integer, settleMs: number }),
  ensureSignedIn: fields({ ...actor, name: nonEmptyString },
    { password: string, exact: boolean, readyTestid: nonEmptyString, ...settle }),
  ensureRegistered: fields({ ...actor, name: nonEmptyString },
    { password: string, exact: boolean, readyTestid: nonEmptyString, ...settle }),
  enterRoom: fields({ ...actor, room: nonEmptyString }),
  expect: fields({ ...actor, testid: nonEmptyString },
    { contains: string, notContains: string, value: string, nonEmpty: boolean, count: integer,
      absent: boolean, ...locator, ...within }),
  expectActorsWith: fields({ ...actors, testid: nonEmptyString, contains: string,
    equals: integer, maxEach: integer }),
  expectActionOutcome: fields({ ...actor, outcome: nonEmptyString }, { routeProvenBy: nonEmptyString }),
  expectAgreement: fields({ ...actors, testid: nonEmptyString },
    { numeric: boolean, ...locator, ...within }),
  expectAllPresent: fields({ ...actor, prefix: string, count: integer, within: number }),
  expectCallOutcomes: fields({ accepted: integer }),
  expectElementCount: fields({ ...actor, testid: nonEmptyString, equals: integer },
    { contains: string, ...locator, within: number }),
  expectForgeryRejected: fields(actor),
  expectNotReceived: fields({ ...actor, contains: string }),
  expectNumber: fields({ ...actor, testid: nonEmptyString },
    { equals: number, atLeast: number, atMost: number, relativeTo: nonEmptyString, plus: number,
      ...locator, ...within }),
  expectOrderMatches: fields({ ...actors, prefix: string }),
  expectSequence: fields({ ...actor, testid: nonEmptyString, equals: stringArray },
    { ...locator, ...within }),
  expectReceived: fields({ ...actor, contains: string, within: number }),
  expectReplayRejected: fields(actor),
  expectStable: fields({ ...actor, testid: nonEmptyString, samples: integer, intervalMs: number }),
  expectUnavailable: fields({ ...actor, testid: nonEmptyString },
    { contains: string, ...locator, ...within }),
  fill: fields({ ...actor, testid: nonEmptyString, text: string },
    { enter: boolean, ...locator, ...settle }),
  forgeWrite: fields({ ...actor, fromActor: nonEmptyString, settleMs: number },
    { field: nonEmptyString, text: string, value: scalar }),
  freshClient: fields(actor),
  openClient: fields(actor, settle),
  pressKey: fields({ ...actor, key: nonEmptyString }, settle),
  race: fields({ branches: anyArray, settleMs: number }),
  register: fields({ ...actor, name: nonEmptyString }),
  recordNumber: fields({ ...actor, testid: nonEmptyString, as: nonEmptyString }, locator),
  reload: fields({ ...actor, settleMs: number }),
  replayAs: fields({ ...actor, from: nonEmptyString, match: string },
    { swap: object, namedAction: object, namedTarget: object, ...settle }),
  replayConcurrently: fields({ ...actors, settleMs: number },
    { match: string, method: nonEmptyString }),
  restartBackend: fields({ settleMs: number }),
  runScript: fields({ script: relativePath, args: anyArray }, { ...settle, timeoutMs: processTimeout }),
  scheduleMessage: fields({ ...actor, text: string, secondsAhead: number }),
  send: fields({ ...actor, text: string }),
  sendConcurrently: fields({ senders: anyArray, delayMs: number }),
  sendMany: fields({ ...actor, prefix: string, count: integer, delayMs: number }),
  setOffline: fields(actor, { offline: boolean, ...settle }),
  signIn: fields({ ...actor, name: nonEmptyString },
    { password: string, exact: boolean, expectFailure: boolean }),
  signUp: fields({ ...actor, name: nonEmptyString },
    { password: string, exact: boolean, expectFailure: boolean }),
  startAppServer: fields({}, settle),
  stopAppServer: fields({}, settle),
  typeInto: fields({ ...actor, text: string }),
  wait: fields({ ...actor, ms: number }),
});

export const ACTION_IDS = Object.freeze(Object.keys(ACTION_DEFINITIONS).sort());

function fail(at: string, message: string): never {
  throw new Error(`invalid benchmark definition at ${at}: ${message}`);
}

function strictObject(value: unknown, at: string, allowed: Set<string>): asserts value is UnknownRecord {
  if (!object(value)) fail(at, 'must be an object');
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) fail(`${at}.${key}`, 'unknown field');
  }
}

function validateLocator(value: unknown, at: string): void {
  strictObject(value, at, new Set(['testid', 'contains', 'containsAll']));
  if (!nonEmptyString(value.testid)) fail(`${at}.testid`, 'must be a non-empty string');
  if (value.contains !== undefined && !string(value.contains)) fail(`${at}.contains`, 'must be a string');
  if (value.containsAll !== undefined
    && (!stringArray(value.containsAll) || value.containsAll.length === 0)) {
    fail(`${at}.containsAll`, 'must be a non-empty string array');
  }
  if (value.contains !== undefined && value.containsAll !== undefined) {
    fail(at, 'cannot use both contains and containsAll');
  }
}

function validateSwap(value: unknown, at: string): void {
  strictObject(value, at, new Set(['find', 'with']));
  if (!string(value.find)) fail(`${at}.find`, 'must be a string');
  if (!string(value.with)) fail(`${at}.with`, 'must be a string');
}

function validateNamedTarget(value: unknown, at: string): void {
  strictObject(value, at, new Set(['testid', 'contains', 'attribute', 'valueType']));
  if (!nonEmptyString(value.testid)) fail(`${at}.testid`, 'must be a non-empty string');
  if (value.contains !== undefined && !string(value.contains)) fail(`${at}.contains`, 'must be a string');
  if (!nonEmptyString(value.attribute)) fail(`${at}.attribute`, 'must be a non-empty string');
  if (value.valueType !== undefined && !oneOf(value.valueType, ['number', 'string'])) {
    fail(`${at}.valueType`, 'must be "number" or "string"');
  }
}

function validateActionInput(value: unknown, at: string): void {
  strictObject(value, at, new Set(['testid', 'contains', 'attribute']));
  if (!nonEmptyString(value.testid)) fail(`${at}.testid`, 'must be a non-empty string');
  if (value.contains !== undefined && !string(value.contains)) fail(`${at}.contains`, 'must be a string');
  if (!nonEmptyString(value.attribute)) fail(`${at}.attribute`, 'must be a non-empty string');
}

function validateNamedActionParams(value: unknown,
  at: string): asserts value is UnknownRecord[] {
  if (!array(value)) fail(at, 'must be an array');
  const names = new Set<string>();
  value.forEach((param, index) => {
    const where = `${at}[${index}]`;
    strictObject(param, where, new Set(['name', 'in', 'placeholder', 'wireType']));
    const name = param.name;
    if (!nonEmptyString(name)) fail(`${where}.name`, 'must be a non-empty string');
    if (names.has(name)) fail(`${where}.name`, `duplicates ${JSON.stringify(name)}`);
    names.add(name);
    if (!oneOf(param.in, ['path', 'body'])) fail(`${where}.in`, 'must be "path" or "body"');
    if (param.in === 'path') {
      if (!nonEmptyString(param.placeholder)) fail(`${where}.placeholder`, 'is required for a path parameter');
    } else if (param.placeholder !== undefined) {
      fail(`${where}.placeholder`, 'is allowed only for a path parameter');
    }
    if (param.wireType !== undefined && param.wireType !== 'u64') {
      fail(`${where}.wireType`, 'must be "u64"');
    }
  });
}

function validateInlineNamedAction(value: unknown,
  at: string): asserts value is UnknownRecord & { params?: UnknownRecord[] } {
  strictObject(value, at, new Set(['id', 'path', 'method', 'reducer', 'args', 'params']));
  const path = value.path;
  if (!nonEmptyString(value.id)) fail(`${at}.id`, 'must be a non-empty string');
  if (!nonEmptyString(path)) fail(`${at}.path`, 'must be a non-empty string');
  if (!nonEmptyString(value.reducer)) fail(`${at}.reducer`, 'must be a non-empty string');
  if (!path.startsWith('/')) fail(`${at}.path`, 'must be an absolute HTTP path');
  if (value.method !== undefined
      && !oneOf(value.method, ['DELETE', 'PATCH', 'POST', 'PUT'])) {
    fail(`${at}.method`, 'must be "DELETE", "PATCH", "POST", or "PUT"');
  }
  if (!anyArray(value.args)) fail(`${at}.args`, 'must be an array');
  const params = value.params;
  if (params !== undefined) {
    validateNamedActionParams(params, `${at}.params`);
    params.filter(param => param.in === 'path').forEach((param, index) => {
      if (!path.includes(String(param.placeholder))) {
        fail(`${at}.params[${index}].placeholder`, `does not appear in path ${JSON.stringify(path)}`);
      }
    });
  }
}

function validateSenders(value: unknown, at: string): void {
  if (!array(value) || value.length === 0) fail(at, 'must be a non-empty array');
  value.forEach((sender, index) => {
    const where = `${at}[${index}]`;
    strictObject(sender, where, new Set(['actor', 'prefix', 'count', 'delayMs']));
    if (!nonEmptyString(sender.actor)) fail(`${where}.actor`, 'must be a non-empty string');
    if (!string(sender.prefix)) fail(`${where}.prefix`, 'must be a string');
    if (!integer(sender.count) || sender.count < 1) fail(`${where}.count`, 'must be a positive integer');
    if (sender.delayMs !== undefined && !number(sender.delayMs)) {
      fail(`${where}.delayMs`, 'must be a number');
    }
  });
}

function validateTargets(value: unknown, actors: readonly string[], at: string): void {
  if (!array(value) || value.length === 0) fail(at, 'must be a non-empty array');
  value.forEach((target, index) => {
    const where = `${at}[${index}]`;
    strictObject(target, where, new Set(['actor', 'in']));
    const targetActor = target.actor;
    if (!nonEmptyString(targetActor)) fail(`${where}.actor`, 'must be a non-empty string');
    if (!actors.includes(targetActor)) fail(`${where}.actor`, 'must name an actor in the action population');
    if (target.in !== undefined) validateLocator(target.in, `${where}.in`);
  });
}

function validateStep(step: unknown, at: string): asserts step is CompiledStep {
  if (!object(step)) fail(at, 'must be an object');
  if (!nonEmptyString(step.do)) fail(`${at}.do`, 'must be a non-empty string');
  const definition = ACTION_DEFINITIONS[step.do];
  if (!definition) fail(`${at}.do`, `unknown action ${JSON.stringify(step.do)}`);
  strictObject(step, at, new Set(Object.keys(definition.fields)));
  for (const [name, validator] of Object.entries(definition.required)) {
    if (step[name] === undefined) fail(`${at}.${name}`, 'is required');
    if (!validator(step[name])) fail(`${at}.${name}`, 'has the wrong type or value');
  }
  for (const [name, value] of Object.entries(step)) {
    const validator = definition.fields[name];
    if (validator && !validator(value)) fail(`${at}.${name}`, 'has the wrong type or value');
  }
  if (step.in) validateLocator(step.in, `${at}.in`);
  if (step.swap) validateSwap(step.swap, `${at}.swap`);
  if (step.namedAction) validateInlineNamedAction(step.namedAction, `${at}.namedAction`);
  if (step.namedTarget) validateNamedTarget(step.namedTarget, `${at}.namedTarget`);
  if (step.do === 'callAction') {
    validateActionInput(step.input, `${at}.input`);
    const namedAction = step.namedAction;
    if (namedAction) {
      validateInlineNamedAction(namedAction, `${at}.namedAction`);
      if (namedAction.id !== step.action) fail(`${at}.namedAction.id`, 'must match action');
      if (!namedAction.params?.length) {
        fail(`${at}.namedAction.params`, 'must be a non-empty array');
      }
    }
    if (step.authentication !== undefined && !oneOf(step.authentication, ['actor', 'none'])) {
      fail(`${at}.authentication`, 'must be "actor" or "none"');
    }
  }
  if (step.do === 'expectActionOutcome'
      && !oneOf(step.outcome, ['accepted', 'refused'])) {
    fail(`${at}.outcome`, 'must be "accepted" or "refused"');
  }
  if (step.do === 'replayAs' && Boolean(step.namedAction) !== Boolean(step.namedTarget)) {
    fail(at, 'replayAs namedAction and namedTarget must be supplied together');
  }
  if (step.do === 'sendConcurrently') validateSenders(step.senders, `${at}.senders`);
  if (step.do === 'clickConcurrently' && step.targets) {
    const population = step.actors;
    if (!stringArray(population)) fail(`${at}.actors`, 'must be an array of strings');
    validateTargets(step.targets, population, `${at}.targets`);
  }
  if (step.do === 'race') {
    const branches = step.branches;
    if (!array(branches) || branches.length < 2) {
      fail(`${at}.branches`, 'must contain at least two branches');
    }
    branches.forEach((branch, branchIndex) => {
      if (!array(branch) || branch.length === 0) {
        fail(`${at}.branches[${branchIndex}]`, 'must be a non-empty step array');
      }
      branch.forEach((nested, stepIndex) =>
        validateStep(nested, `${at}.branches[${branchIndex}][${stepIndex}]`));
    });
  }
  if (step.do === 'expectNumber'
    && !['equals', 'atLeast', 'atMost', 'relativeTo'].some(name => step[name] !== undefined)) {
    fail(at, 'expectNumber requires equals, atLeast, atMost, or relativeTo');
  }
  if (step.do === 'setOffline' && step.offline === undefined) step.offline = true;
}

export function compileActionInput(input: unknown,
  { source = '<action>', expectedAction = null }:
    { source?: string; expectedAction?: string | null } = {}): CompiledStep {
  const action = structuredClone(input);
  validateStep(action, source);
  if (expectedAction !== null && action.do !== expectedAction) {
    fail(`${source}.do`, `declares ${JSON.stringify(action.do)}, expected ${JSON.stringify(expectedAction)}`);
  }
  return action;
}

const SCENARIO_FIELDS = new Set([
  'schemaVersion', 'blocked_on', 'features', 'level', 'name', 'note', 'status', 'track',
  'why_it_matters', 'writeUrlPattern',
]);
const FEATURE_FIELDS = new Set(['actors', 'criteria', 'id', 'max', 'name', 'note', 'setup']);
const CRITERION_FIELDS = new Set([
  'desc', 'id', 'note', 'points', 'provenBy', 'statedBy', 'steps', 'withheld',
]);

export function compileScenarioDefinition(input: unknown,
  { source = '<scenario>', expectedLevel = null }:
    { source?: string; expectedLevel?: number | null } = {}): CompiledScenarioDefinition {
  const scenario = structuredClone(input);
  strictObject(scenario, source, SCENARIO_FIELDS);
  if (scenario.schemaVersion !== DEFINITION_SCHEMA_VERSION) {
    fail(`${source}.schemaVersion`, `unsupported version ${scenario.schemaVersion}`);
  }
  if (!integer(scenario.level) || scenario.level < 1) fail(`${source}.level`, 'must be a positive integer');
  if (expectedLevel !== null && scenario.level !== Number(expectedLevel)) {
    fail(`${source}.level`, `declares L${scenario.level}, expected L${Number(expectedLevel)}`);
  }
  if (scenario.name !== undefined && !nonEmptyString(scenario.name)) {
    fail(`${source}.name`, 'must be a non-empty string');
  }
  if (!array(scenario.features) || scenario.features.length === 0) {
    fail(`${source}.features`, 'must be a non-empty array');
  }
  const featureIds = new Set();
  const criterionKeys = new Set();
  scenario.features.forEach((feature, featureIndex) => {
    const featureAt = `${source}.features[${featureIndex}]`;
    strictObject(feature, featureAt, FEATURE_FIELDS);
    if (!integer(feature.id) || feature.id < 1) fail(`${featureAt}.id`, 'must be a positive integer');
    if (featureIds.has(feature.id)) fail(`${featureAt}.id`, `duplicate feature id ${feature.id}`);
    featureIds.add(feature.id);
    if (!nonEmptyString(feature.name)) fail(`${featureAt}.name`, 'must be a non-empty string');
    if (feature.actors !== undefined && !stringArray(feature.actors)) {
      fail(`${featureAt}.actors`, 'must be an array of non-empty strings');
    }
    if (feature.max !== undefined && (!integer(feature.max) || feature.max < 0)) {
      fail(`${featureAt}.max`, 'must be a non-negative integer');
    }
    if (!array(feature.setup)) fail(`${featureAt}.setup`, 'must be an array');
    feature.setup.forEach((step, stepIndex) => validateStep(step, `${featureAt}.setup[${stepIndex}]`));
    if (!array(feature.criteria) || feature.criteria.length === 0) {
      fail(`${featureAt}.criteria`, 'must be a non-empty array');
    }
    let points = 0;
    feature.criteria.forEach((criterion, criterionIndex) => {
      const criterionAt = `${featureAt}.criteria[${criterionIndex}]`;
      strictObject(criterion, criterionAt, CRITERION_FIELDS);
      if (!nonEmptyString(criterion.id)) fail(`${criterionAt}.id`, 'must be a non-empty string');
      const key = `${feature.id}:${criterion.id}`;
      if (criterionKeys.has(key)) fail(`${criterionAt}.id`, `duplicate criterion key ${key}`);
      criterionKeys.add(key);
      if (!nonEmptyString(criterion.desc)) fail(`${criterionAt}.desc`, 'must be a non-empty string');
      const criterionPoints = criterion.points ?? 1;
      if (!integer(criterionPoints) || criterionPoints < 0) {
        fail(`${criterionAt}.points`, 'must be a non-negative integer');
      }
      // Materialize the default so downstream scoring always receives a number.
      criterion.points = criterionPoints;
      points += criterionPoints;
      if (!array(criterion.steps)) fail(`${criterionAt}.steps`, 'must be an array');
      if (criterion.steps.length === 0 && (criterionPoints !== 0 || !nonEmptyString(criterion.withheld))) {
        fail(`${criterionAt}.steps`, 'may be empty only for an explicitly withheld zero-point criterion');
      }
      criterion.steps.forEach((step, stepIndex) =>
        validateStep(step, `${criterionAt}.steps[${stepIndex}]`));
    });
    if (feature.max !== undefined && feature.max !== points) {
      fail(`${featureAt}.max`, `is ${feature.max}, but criteria total ${points}`);
    }
  });
  scenario.schemaVersion = DEFINITION_SCHEMA_VERSION;
  return scenario as CompiledScenarioDefinition;
}

const TRACK_FIELDS = new Set([
  'schemaVersion', 'actions', 'databaseProvenance', 'internal', 'plannedThrough', 'portOffset', 'reseedOnReset',
  'restartProbe', 'slug', 'suites', 'title', 'validatedThrough',
]);
const SUITE_FIELDS = new Set(['id', 'inherit', 'spec']);
const NAMED_ACTION_FIELDS = new Set(['args', 'id', 'params', 'path', 'reducer']);
const DATABASE_PROVENANCE_FIELDS = new Set(['action', 'body', 'markerParameter']);
export function compileTrackManifest(input: unknown,
  { source = '<track>' }: { source?: string } = {}): CompiledTrackManifest {
  const manifest = structuredClone(input);
  if (!object(manifest)) fail(source, 'must be an object');
  strictObject(manifest, source, TRACK_FIELDS);
  if (manifest.schemaVersion !== DEFINITION_SCHEMA_VERSION) {
    fail(`${source}.schemaVersion`, `unsupported version ${manifest.schemaVersion}`);
  }
  if (!nonEmptyString(manifest.title)) fail(`${source}.title`, 'must be a non-empty string');
  if (manifest.slug !== undefined && !string(manifest.slug)) fail(`${source}.slug`, 'must be a string');
  for (const key of ['validatedThrough', 'plannedThrough', 'portOffset']) {
    if (!integer(manifest[key]) || manifest[key] < 0) fail(`${source}.${key}`, 'must be a non-negative integer');
  }
  if (!nonEmptyString(manifest.restartProbe) || !manifest.restartProbe.startsWith('/')) {
    fail(`${source}.restartProbe`, 'must be an absolute HTTP path');
  }
  if (manifest.internal !== undefined && !boolean(manifest.internal)) fail(`${source}.internal`, 'must be boolean');
  if (manifest.reseedOnReset !== undefined && !boolean(manifest.reseedOnReset)) {
    fail(`${source}.reseedOnReset`, 'must be boolean');
  }
  if (manifest.databaseProvenance !== undefined) {
    strictObject(manifest.databaseProvenance, `${source}.databaseProvenance`,
      DATABASE_PROVENANCE_FIELDS);
    for (const field of ['action', 'markerParameter']) {
      if (!nonEmptyString(manifest.databaseProvenance[field])) {
        fail(`${source}.databaseProvenance.${field}`, 'must be a non-empty string');
      }
    }
    if (!object(manifest.databaseProvenance.body)
      || Object.keys(manifest.databaseProvenance.body).length === 0) {
      fail(`${source}.databaseProvenance.body`, 'must be a non-empty object');
    }
    for (const [field, value] of Object.entries(manifest.databaseProvenance.body)) {
      if (!nonEmptyString(field) || typeof value !== 'string') {
        fail(`${source}.databaseProvenance.body.${field}`, 'must be a string');
      }
    }
  }
  if (!object(manifest.suites) || Object.keys(manifest.suites).length === 0) {
    fail(`${source}.suites`, 'must be a non-empty level map');
  }
  for (const [level, suites] of Object.entries(manifest.suites)) {
    if (!/^[1-9]\d*$/.test(level)) fail(`${source}.suites.${level}`, 'level key must be a positive integer');
    if (!array(suites) || suites.length === 0) fail(`${source}.suites.${level}`, 'must be a non-empty array');
    const ids = new Set();
    suites.forEach((suite, index) => {
      const at = `${source}.suites.${level}[${index}]`;
      strictObject(suite, at, SUITE_FIELDS);
      if (!nonEmptyString(suite.id)) fail(`${at}.id`, 'must be a non-empty string');
      if (ids.has(suite.id)) fail(`${at}.id`, `duplicate suite id ${suite.id}`);
      ids.add(suite.id);
      if (!nonEmptyString(suite.spec)) fail(`${at}.spec`, 'must be a non-empty string');
      if (!oneOf(suite.inherit, ['none', 'all-higher-levels'])) {
        fail(`${at}.inherit`, 'must be none or all-higher-levels');
      }
    });
  }
  if (manifest.actions !== undefined) {
    if (!array(manifest.actions)) fail(`${source}.actions`, 'must be an array');
    const ids = new Set<string>();
    manifest.actions.forEach((action, index) => {
      const at = `${source}.actions[${index}]`;
      strictObject(action, at, NAMED_ACTION_FIELDS);
      const actionPath = action.path;
      for (const [key, field] of
        [['id', action.id], ['path', actionPath], ['reducer', action.reducer]] as const) {
        if (!nonEmptyString(field)) fail(`${at}.${key}`, 'must be a non-empty string');
      }
      if (!nonEmptyString(actionPath) || !actionPath.startsWith('/')) {
        fail(`${at}.path`, 'must be an absolute HTTP path');
      }
      if (!array(action.args)) fail(`${at}.args`, 'must be an array');
      const actionParams = action.params;
      if (actionParams !== undefined) {
        validateNamedActionParams(actionParams, `${at}.params`);
        actionParams.filter(param => param.in === 'path')
          .forEach((param, paramIndex) => {
          if (!actionPath.includes(String(param.placeholder))) {
            fail(`${at}.params[${paramIndex}].placeholder`,
              `does not appear in path ${JSON.stringify(actionPath)}`);
          }
        });
      }
      const actionId = String(action.id);
      if (ids.has(actionId)) fail(`${at}.id`, `duplicate named action ${actionId}`);
      ids.add(actionId);
    });
  }
  const provenance = manifest.databaseProvenance;
  if (object(provenance)) {
    const actionName = provenance.action;
    const action = manifest.actions?.find(candidate =>
      object(candidate) && candidate.id === actionName);
    if (!object(action)) {
      fail(`${source}.databaseProvenance.action`, 'must name one declared action');
    }
    if (!object(provenance.body)
      || !Object.hasOwn(provenance.body, String(provenance.markerParameter))) {
      fail(`${source}.databaseProvenance.markerParameter`,
        'must name one field in the provenance body');
    }
  }
  manifest.schemaVersion = DEFINITION_SCHEMA_VERSION;
  return manifest as CompiledTrackManifest;
}
