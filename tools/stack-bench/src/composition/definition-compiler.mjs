// Compile human-authored benchmark definitions into a strict, normalized plan.
//
// This module is the explicit contract for the scenario language. It preserves
// the public JSON shape while rejecting unknown or malformed input before a run
// can acquire backend resources.

export const DEFINITION_SCHEMA_VERSION = 1;

const string = value => typeof value === 'string';
const nonEmptyString = value => string(value) && value.trim().length > 0;
const number = value => typeof value === 'number' && Number.isFinite(value);
const integer = value => Number.isInteger(value);
const boolean = value => typeof value === 'boolean';
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const array = value => Array.isArray(value);
const stringArray = value => array(value) && value.every(nonEmptyString);
const anyArray = value => array(value);
const scalar = value => value === null || ['string', 'number', 'boolean'].includes(typeof value);
const relativePath = value => nonEmptyString(value)
  && !/^(?:[A-Za-z]:[\\/]|[\\/])/.test(value)
  && !value.split(/[\\/]/).includes('..');
const processTimeout = value => number(value) && value > 0 && value <= 85_000;

function fields(required, optional = {}) {
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

export const ACTION_DEFINITIONS = Object.freeze({
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
    { contains: string, notContains: string, nonEmpty: boolean, count: integer,
      absent: boolean, ...locator, ...within }),
  expectActorsWith: fields({ ...actors, testid: nonEmptyString, contains: string,
    equals: integer, maxEach: integer }),
  expectActionOutcome: fields({ ...actor, outcome: nonEmptyString }, { routeProvenBy: nonEmptyString }),
  expectAgreement: fields({ ...actors, testid: nonEmptyString },
    { numeric: boolean, ...locator, ...within }),
  expectAllPresent: fields({ ...actor, prefix: string, count: integer, within: number }),
  expectCallOutcomes: fields({ accepted: integer }),
  expectElementCount: fields({ ...actor, testid: nonEmptyString, contains: string,
    equals: integer, within: number }),
  expectForgeryRejected: fields(actor),
  expectNotReceived: fields({ ...actor, contains: string }),
  expectNumber: fields({ ...actor, testid: nonEmptyString },
    { equals: number, atLeast: number, atMost: number, relativeTo: nonEmptyString, plus: number,
      ...locator, ...within }),
  expectOrderMatches: fields({ ...actors, prefix: string }),
  expectReceived: fields({ ...actor, contains: string, within: number }),
  expectReplayRejected: fields(actor),
  expectStable: fields({ ...actor, testid: nonEmptyString, samples: integer, intervalMs: number }),
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

const fail = (at, message) => { throw new Error(`invalid benchmark definition at ${at}: ${message}`); };

function strictObject(value, at, allowed) {
  if (!object(value)) fail(at, 'must be an object');
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) fail(`${at}.${key}`, 'unknown field');
  }
}

function validateLocator(value, at) {
  strictObject(value, at, new Set(['testid', 'contains']));
  if (!nonEmptyString(value.testid)) fail(`${at}.testid`, 'must be a non-empty string');
  if (value.contains !== undefined && !string(value.contains)) fail(`${at}.contains`, 'must be a string');
}

function validateSwap(value, at) {
  strictObject(value, at, new Set(['find', 'with']));
  if (!string(value.find)) fail(`${at}.find`, 'must be a string');
  if (!string(value.with)) fail(`${at}.with`, 'must be a string');
}

function validateNamedTarget(value, at) {
  strictObject(value, at, new Set(['testid', 'contains', 'attribute', 'valueType']));
  if (!nonEmptyString(value.testid)) fail(`${at}.testid`, 'must be a non-empty string');
  if (value.contains !== undefined && !string(value.contains)) fail(`${at}.contains`, 'must be a string');
  if (!nonEmptyString(value.attribute)) fail(`${at}.attribute`, 'must be a non-empty string');
  if (value.valueType !== undefined && !['number', 'string'].includes(value.valueType)) {
    fail(`${at}.valueType`, 'must be "number" or "string"');
  }
}

function validateActionInput(value, at) {
  strictObject(value, at, new Set(['testid', 'contains', 'attribute']));
  if (!nonEmptyString(value.testid)) fail(`${at}.testid`, 'must be a non-empty string');
  if (value.contains !== undefined && !string(value.contains)) fail(`${at}.contains`, 'must be a string');
  if (!nonEmptyString(value.attribute)) fail(`${at}.attribute`, 'must be a non-empty string');
}

function validateNamedActionParams(value, at) {
  if (!array(value)) fail(at, 'must be an array');
  const names = new Set();
  value.forEach((param, index) => {
    const where = `${at}[${index}]`;
    strictObject(param, where, new Set(['name', 'in', 'placeholder', 'wireType']));
    if (!nonEmptyString(param.name)) fail(`${where}.name`, 'must be a non-empty string');
    if (names.has(param.name)) fail(`${where}.name`, `duplicates ${JSON.stringify(param.name)}`);
    names.add(param.name);
    if (!['path', 'body'].includes(param.in)) fail(`${where}.in`, 'must be "path" or "body"');
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

function validateInlineNamedAction(value, at) {
  strictObject(value, at, new Set(['id', 'path', 'reducer', 'args', 'params']));
  for (const key of ['id', 'path', 'reducer']) {
    if (!nonEmptyString(value[key])) fail(`${at}.${key}`, 'must be a non-empty string');
  }
  if (!value.path.startsWith('/')) fail(`${at}.path`, 'must be an absolute HTTP path');
  if (!anyArray(value.args)) fail(`${at}.args`, 'must be an array');
  if (value.params !== undefined) {
    validateNamedActionParams(value.params, `${at}.params`);
    value.params.filter(param => param.in === 'path').forEach((param, index) => {
      if (!value.path.includes(param.placeholder)) {
        fail(`${at}.params[${index}].placeholder`, `does not appear in path ${JSON.stringify(value.path)}`);
      }
    });
  }
}

function validateSenders(value, at) {
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

function validateTargets(value, actors, at) {
  if (!array(value) || value.length === 0) fail(at, 'must be a non-empty array');
  value.forEach((target, index) => {
    const where = `${at}[${index}]`;
    strictObject(target, where, new Set(['actor', 'in']));
    if (!nonEmptyString(target.actor)) fail(`${where}.actor`, 'must be a non-empty string');
    if (!actors.includes(target.actor)) fail(`${where}.actor`, 'must name an actor in the action population');
    if (target.in !== undefined) validateLocator(target.in, `${where}.in`);
  });
}

function validateStep(step, at) {
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
    if (!validator(value)) fail(`${at}.${name}`, 'has the wrong type or value');
  }
  if (step.in) validateLocator(step.in, `${at}.in`);
  if (step.swap) validateSwap(step.swap, `${at}.swap`);
  if (step.namedAction) validateInlineNamedAction(step.namedAction, `${at}.namedAction`);
  if (step.namedTarget) validateNamedTarget(step.namedTarget, `${at}.namedTarget`);
  if (step.do === 'callAction') {
    validateActionInput(step.input, `${at}.input`);
    if (step.namedAction) {
      validateInlineNamedAction(step.namedAction, `${at}.namedAction`);
      if (step.namedAction.id !== step.action) {
        fail(`${at}.namedAction.id`, 'must match action');
      }
      if (!step.namedAction.params?.length) {
        fail(`${at}.namedAction.params`, 'must be a non-empty array');
      }
    }
    if (step.authentication !== undefined && !['actor', 'none'].includes(step.authentication)) {
      fail(`${at}.authentication`, 'must be "actor" or "none"');
    }
  }
  if (step.do === 'expectActionOutcome'
      && !['accepted', 'refused'].includes(step.outcome)) {
    fail(`${at}.outcome`, 'must be "accepted" or "refused"');
  }
  if (step.do === 'replayAs' && Boolean(step.namedAction) !== Boolean(step.namedTarget)) {
    fail(at, 'replayAs namedAction and namedTarget must be supplied together');
  }
  if (step.do === 'sendConcurrently') validateSenders(step.senders, `${at}.senders`);
  if (step.do === 'clickConcurrently' && step.targets) {
    validateTargets(step.targets, step.actors, `${at}.targets`);
  }
  if (step.do === 'race') {
    if (step.branches.length < 2) fail(`${at}.branches`, 'must contain at least two branches');
    step.branches.forEach((branch, branchIndex) => {
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

export function compileActionInput(input, { source = '<action>', expectedAction = null } = {}) {
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

export function compileScenarioDefinition(input, { source = '<scenario>', expectedLevel = null } = {}) {
  const scenario = structuredClone(input);
  strictObject(scenario, source, SCENARIO_FIELDS);
  if (scenario.schemaVersion !== undefined && scenario.schemaVersion !== DEFINITION_SCHEMA_VERSION) {
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
      // Downstream execution must never have to repeat source defaults. Leaving
      // this implicit made `result.score += criterion.points` add `undefined`,
      // producing NaN in memory and null in the JSON grade for ordinary
      // one-point criteria that omitted the field.
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
  return scenario;
}

const TRACK_FIELDS = new Set([
  'schemaVersion', 'actions', 'internal', 'plannedThrough', 'portOffset', 'reseedOnReset',
  'restartProbe', 'slug', 'suites', 'title', 'validatedThrough',
]);
const SUITE_FIELDS = new Set(['id', 'inherit', 'spec']);
const NAMED_ACTION_FIELDS = new Set(['args', 'id', 'params', 'path', 'reducer']);
// Legacy v0 inferred persistence from these suite names. Keep that inference
// in exactly one compatibility boundary; schema-v1 manifests must say what
// persists so a new suite name cannot silently change level semantics.
const LEGACY_CUMULATIVE_SUITE_IDS = new Set(['contention', 'invariants', 'systems']);

export function compileTrackManifest(input, { source = '<track>' } = {}) {
  const manifest = structuredClone(input);
  const isLegacy = manifest.schemaVersion === undefined;
  strictObject(manifest, source, TRACK_FIELDS);
  if (manifest.schemaVersion !== undefined && manifest.schemaVersion !== DEFINITION_SCHEMA_VERSION) {
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
      if (suite.inherit === undefined) {
        if (!isLegacy) fail(`${at}.inherit`, 'is required in schema v1');
        suite.inherit = LEGACY_CUMULATIVE_SUITE_IDS.has(suite.id)
          ? 'all-higher-levels' : 'none';
      } else if (!['none', 'all-higher-levels'].includes(suite.inherit)) {
        fail(`${at}.inherit`, 'must be none or all-higher-levels');
      }
    });
  }
  if (manifest.actions !== undefined) {
    if (!array(manifest.actions)) fail(`${source}.actions`, 'must be an array');
    const ids = new Set();
    manifest.actions.forEach((action, index) => {
      const at = `${source}.actions[${index}]`;
      strictObject(action, at, NAMED_ACTION_FIELDS);
      for (const key of ['id', 'path', 'reducer']) {
        if (!nonEmptyString(action[key])) fail(`${at}.${key}`, 'must be a non-empty string');
      }
      if (!action.path.startsWith('/')) fail(`${at}.path`, 'must be an absolute HTTP path');
      if (!array(action.args)) fail(`${at}.args`, 'must be an array');
      if (action.params !== undefined) {
        validateNamedActionParams(action.params, `${at}.params`);
        action.params.filter(param => param.in === 'path').forEach((param, paramIndex) => {
          if (!action.path.includes(param.placeholder)) {
            fail(`${at}.params[${paramIndex}].placeholder`,
              `does not appear in path ${JSON.stringify(action.path)}`);
          }
        });
      }
      if (ids.has(action.id)) fail(`${at}.id`, `duplicate named action ${action.id}`);
      ids.add(action.id);
    });
  }
  manifest.schemaVersion = DEFINITION_SCHEMA_VERSION;
  return manifest;
}
