import { isDeepStrictEqual } from 'node:util';

export const DEPENDENCY_MODE_SCHEMA_VERSION = 2;
export const DEPENDENCY_MODE_POLICY = 'dependency-gated';

const ID = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/;
const HASH = /^[a-f0-9]{64}$/;
const NODE_STATUSES = new Set(['locked', 'active', 'passed', 'exhausted', 'regressed', 'blocked']);
const TERMINAL_OUTCOMES = new Set(['passed', 'partial', 'failed']);
const INCONCLUSIVE_CATEGORIES = new Set([
  'provider_failure', 'harness_failure', 'interrupted', 'inconclusive_evidence',
]);
const RELEASE_STATES = new Set(['draft', 'qualified']);

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const fail = (at, message) => { throw new Error(`invalid dependency mode at ${at}: ${message}`); };

function strictObject(value, at, fields) {
  if (!object(value)) fail(at, 'must be an object');
  for (const key of Object.keys(value)) {
    if (!fields.has(key)) fail(`${at}.${key}`, 'unknown field');
  }
}

function validateSourceEvidence(value, at) {
  strictObject(value, at, new Set(['kind', 'id', 'sha256']));
  if (value.kind !== 'grade_bundle' || typeof value.id !== 'string' || !value.id
    || !HASH.test(value.sha256 ?? '')) {
    throw new Error(`${at} must identify one grade bundle artifact`);
  }
}

function nonEmptyString(value, at) {
  if (typeof value !== 'string' || value.trim().length === 0) fail(at, 'must be a non-empty string');
  return value;
}

function identifier(value, at) {
  nonEmptyString(value, at);
  if (!ID.test(value)) fail(at, 'must contain lowercase letters, numbers, dots, dashes, or underscores');
  return value;
}

function semanticVersion(value, at) {
  nonEmptyString(value, at);
  if (!VERSION.test(value)) fail(at, 'must be an exact semantic version');
  return value;
}

function positiveInteger(value, at) {
  if (!Number.isSafeInteger(value) || value < 1) fail(at, 'must be a positive integer within the safe range');
  return value;
}

function uniqueStrings(value, at, { exactRefs = false, nonEmpty = true } = {}) {
  if (!Array.isArray(value) || (nonEmpty && value.length === 0)) {
    fail(at, `must be ${nonEmpty ? 'a non-empty' : 'an'} array`);
  }
  const seen = new Set();
  return value.map((item, index) => {
    nonEmptyString(item, `${at}[${index}]`);
    if (seen.has(item)) fail(`${at}[${index}]`, `duplicates ${JSON.stringify(item)}`);
    seen.add(item);
    if (exactRefs) {
      const split = item.lastIndexOf('@');
      if (split < 1) fail(`${at}[${index}]`, 'must be an exact id@version reference');
      identifier(item.slice(0, split), `${at}[${index}] id`);
      semanticVersion(item.slice(split + 1), `${at}[${index}] version`);
    } else {
      identifier(item, `${at}[${index}]`);
    }
    return item;
  });
}

function compileStrikeBudgets(value, levels) {
  strictObject(value, 'strikes', new Set(['default', 'levels']));
  if (value.default !== undefined) positiveInteger(value.default, 'strikes.default');
  strictObject(value.levels ?? {}, 'strikes.levels', new Set(levels.map(String)));
  const overrides = new Map();
  for (const [level, budget] of Object.entries(value.levels ?? {})) {
    if (!/^[1-9]\d*$/.test(level)) fail(`strikes.levels.${level}`, 'level key must be a positive integer');
    overrides.set(Number(level), positiveInteger(budget, `strikes.levels.${level}`));
  }
  return Object.fromEntries(levels.map(level => {
    const budget = overrides.get(level) ?? value.default;
    if (budget === undefined) fail(`strikes.levels.${level}`, 'is required when no default is set');
    return [String(level), budget];
  }));
}

function assertAcyclic(nodesById) {
  const visiting = new Set();
  const visited = new Set();
  const visit = (node, chain) => {
    if (visited.has(node.id)) return;
    if (visiting.has(node.id)) fail('nodes', `dependency cycle: ${[...chain, node.id].join(' -> ')}`);
    visiting.add(node.id);
    for (const dependency of node.dependencies) visit(nodesById.get(dependency), [...chain, node.id]);
    visiting.delete(node.id);
    visited.add(node.id);
  };
  for (const node of nodesById.values()) visit(node, []);
}

export function compileDependencyMode(input, { source = '<dependency-mode>' } = {}) {
  const definition = structuredClone(input);
  strictObject(definition, source, new Set([
    'schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'policy', 'strikes', 'nodes',
    'questlines',
  ]));
  if (definition.schemaVersion !== DEPENDENCY_MODE_SCHEMA_VERSION) {
    fail(`${source}.schemaVersion`, `must be ${DEPENDENCY_MODE_SCHEMA_VERSION}`);
  }
  if (definition.kind !== 'progression-mode') fail(`${source}.kind`, 'must be "progression-mode"');
  identifier(definition.id, `${source}.id`);
  semanticVersion(definition.version, `${source}.version`);
  if (!RELEASE_STATES.has(definition.state)) {
    fail(`${source}.state`, 'must be "draft" or "qualified"');
  }
  nonEmptyString(definition.title, `${source}.title`);
  if (definition.policy !== DEPENDENCY_MODE_POLICY) {
    fail(`${source}.policy`, `must be ${JSON.stringify(DEPENDENCY_MODE_POLICY)}`);
  }
  if (!Array.isArray(definition.nodes) || definition.nodes.length === 0) {
    fail(`${source}.nodes`, 'must be a non-empty array');
  }

  const nodeIds = new Set();
  const checkIds = new Set();
  definition.nodes.forEach((node, index) => {
    const at = `${source}.nodes[${index}]`;
    strictObject(node, at, new Set([
      'id', 'title', 'questline', 'level', 'featureRefs', 'promptModules', 'gradingChecks',
      'dependencies', 'dependencyReasons',
    ]));
    identifier(node.id, `${at}.id`);
    if (nodeIds.has(node.id)) fail(`${at}.id`, `duplicates ${JSON.stringify(node.id)}`);
    nodeIds.add(node.id);
    nonEmptyString(node.title, `${at}.title`);
    identifier(node.questline, `${at}.questline`);
    node.featureRefs = uniqueStrings(node.featureRefs, `${at}.featureRefs`, { exactRefs: true }).sort();
    node.promptModules = uniqueStrings(node.promptModules, `${at}.promptModules`,
      { exactRefs: true, nonEmpty: false }).sort();
    if (!Array.isArray(node.gradingChecks) || node.gradingChecks.length === 0) {
      fail(`${at}.gradingChecks`, 'must be a non-empty array');
    }
    const localChecks = new Set();
    node.gradingChecks.forEach((check, checkIndex) => {
      const checkAt = `${at}.gradingChecks[${checkIndex}]`;
      strictObject(check, checkAt, new Set(['id', 'points']));
      identifier(check.id, `${checkAt}.id`);
      if (localChecks.has(check.id)) fail(`${checkAt}.id`, `duplicates ${JSON.stringify(check.id)}`);
      if (checkIds.has(check.id)) fail(`${checkAt}.id`, `is already owned by another node`);
      localChecks.add(check.id);
      checkIds.add(check.id);
      positiveInteger(check.points, `${checkAt}.points`);
    });
    node.gradingChecks.sort((left, right) => left.id.localeCompare(right.id));
    if (!Array.isArray(node.dependencies)) fail(`${at}.dependencies`, 'must be an array');
    const compiledNode = node.level !== undefined || node.dependencyReasons !== undefined;
    if (compiledNode && (node.level === undefined || node.dependencyReasons === undefined)) {
      fail(at, 'compiled level and dependency reasons must appear together');
    }
    if (compiledNode) positiveInteger(node.level, `${at}.level`);
    const dependencies = new Set();
    node.dependencies.forEach((dependency, dependencyIndex) => {
      const dependencyAt = `${at}.dependencies[${dependencyIndex}]`;
      const dependencyId = compiledNode ? dependency : dependency?.id;
      if (compiledNode) identifier(dependencyId, dependencyAt);
      else {
        strictObject(dependency, dependencyAt, new Set(['id', 'reason']));
        identifier(dependencyId, `${dependencyAt}.id`);
        nonEmptyString(dependency.reason, `${dependencyAt}.reason`);
      }
      if (dependencies.has(dependencyId)) {
        fail(compiledNode ? dependencyAt : `${dependencyAt}.id`,
          `duplicates ${JSON.stringify(dependencyId)}`);
      }
      dependencies.add(dependencyId);
    });
    node.dependencies = [...dependencies].sort();
    if (compiledNode) {
      strictObject(node.dependencyReasons, `${at}.dependencyReasons`, new Set(node.dependencies));
      for (const dependencyId of node.dependencies) {
        nonEmptyString(node.dependencyReasons[dependencyId],
          `${at}.dependencyReasons.${dependencyId}`);
        node.dependencyReasons[dependencyId] = node.dependencyReasons[dependencyId].trim();
      }
    } else {
      const authoredDependencies = input.nodes[index].dependencies;
      node.dependencyReasons = Object.fromEntries(node.dependencies.map(dependencyId => [
        dependencyId,
        authoredDependencies.find(dependency => dependency.id === dependencyId).reason.trim(),
      ]));
    }
  });

  const nodesById = new Map(definition.nodes.map(node => [node.id, node]));
  for (const node of definition.nodes) {
    const at = `${source}.nodes.${node.id}.dependencies`;
    for (const dependency of node.dependencies) {
      if (!nodesById.has(dependency)) fail(at, `unknown parent ${JSON.stringify(dependency)}`);
    }
  }
  assertAcyclic(nodesById);
  const declaredLevels = new Map(definition.nodes
    .filter(node => node.level !== undefined).map(node => [node.id, node.level]));
  definition.nodes.forEach(node => { delete node.level; });
  const levelFor = node => {
    if (node.level !== undefined) return node.level;
    node.level = node.dependencies.length === 0
      ? 1
      : 1 + Math.max(...node.dependencies.map(parentId => levelFor(nodesById.get(parentId))));
    return node.level;
  };
  definition.nodes.forEach(levelFor);
  for (const [nodeId, declaredLevel] of declaredLevels) {
    if (nodesById.get(nodeId).level !== declaredLevel) {
      fail(`${source}.nodes.${nodeId}.level`, 'does not match calculated dependency depth');
    }
  }
  definition.nodes.sort((left, right) => left.level - right.level || left.id.localeCompare(right.id));
  const levels = [...new Set(definition.nodes.map(node => node.level))].sort((a, b) => a - b);
  definition.strikes = { levels: compileStrikeBudgets(definition.strikes, levels) };

  if (!Array.isArray(definition.questlines) || definition.questlines.length === 0) {
    fail(`${source}.questlines`, 'must be a non-empty array');
  }
  const questlineIds = new Set();
  definition.questlines.forEach((questline, index) => {
    const at = `${source}.questlines[${index}]`;
    strictObject(questline, at, new Set(['id', 'title', 'nodes']));
    identifier(questline.id, `${at}.id`);
    if (questlineIds.has(questline.id)) fail(`${at}.id`, `duplicates ${JSON.stringify(questline.id)}`);
    questlineIds.add(questline.id);
    nonEmptyString(questline.title, `${at}.title`);
  });
  for (const node of definition.nodes) {
    if (!questlineIds.has(node.questline)) {
      fail(`${source}.nodes.${node.id}.questline`, `unknown questline ${JSON.stringify(node.questline)}`);
    }
  }
  definition.questlines.forEach(questline => {
    const owned = definition.nodes.filter(node => node.questline === questline.id)
      .map(node => node.id);
    if (owned.length === 0) {
      fail(`${source}.questlines.${questline.id}`, 'owns no nodes');
    }
    if (questline.nodes !== undefined) {
      const declared = uniqueStrings(questline.nodes, `${source}.questlines.${questline.id}.nodes`);
      if (!isDeepStrictEqual(declared, owned)) {
        fail(`${source}.questlines.${questline.id}.nodes`, 'does not match node ownership');
      }
    }
    questline.nodes = owned;
  });
  definition.questlines.sort((left, right) => left.id.localeCompare(right.id));
  return definition;
}

function nodesAt(definition, level) {
  return definition.nodes.filter(node => node.level === level);
}

function currentPromptNodeIds(state) {
  if (state.phase !== 'active') return [];
  const current = nodesAt(state.definition, state.level)
    .filter(node => ['active', 'regressed'].includes(state.nodes[node.id].status))
    .map(node => node.id);
  const required = new Set([
    ...current,
    ...state.definition.nodes.filter(node => node.level < state.level
      && state.nodes[node.id].status === 'regressed').map(node => node.id),
  ]);
  const nodesById = new Map(state.definition.nodes.map(node => [node.id, node]));
  const includeRegressedDependencies = nodeId => {
    for (const parentId of nodesById.get(nodeId).dependencies) {
      if (state.nodes[parentId].status === 'regressed') required.add(parentId);
      includeRegressedDependencies(parentId);
    }
  };
  current.forEach(includeRegressedDependencies);
  return [...required].sort((left, right) => {
    const leftNode = nodesById.get(left);
    const rightNode = nodesById.get(right);
    return leftNode.level - rightNode.level || left.localeCompare(right);
  });
}

function gradingNodeIds(state) {
  const promptIds = currentPromptNodeIds(state);
  const selected = new Set([
    ...promptIds,
    ...state.definition.nodes
      .filter(node => node.level <= state.level && state.nodes[node.id].status === 'passed')
      .map(node => node.id),
  ]);
  const nodesById = new Map(state.definition.nodes.map(node => [node.id, node]));
  const includeDependencies = nodeId => {
    for (const parentId of nodesById.get(nodeId).dependencies) {
      selected.add(parentId);
      includeDependencies(parentId);
    }
  };
  promptIds.forEach(includeDependencies);
  return [...selected].sort((left, right) => {
    const leftNode = nodesById.get(left);
    const rightNode = nodesById.get(right);
    return leftNode.level - rightNode.level || left.localeCompare(right);
  });
}

function selectionFor(state, nodeIds, field) {
  const selected = new Set(nodeIds);
  return state.definition.nodes.filter(node => selected.has(node.id)).flatMap(node => node[field]);
}

function selectedPromptWork(state) {
  const nodeIds = currentPromptNodeIds(state);
  return {
    nodeIds,
    featureRefs: [...new Set(selectionFor(state, nodeIds, 'featureRefs'))].sort(),
    promptModules: [...new Set(selectionFor(state, nodeIds, 'promptModules'))].sort(),
  };
}

function selectedGradingWork(state) {
  const nodeIds = gradingNodeIds(state);
  const selected = new Set(nodeIds);
  return {
    nodeIds,
    checks: state.definition.nodes.filter(node => selected.has(node.id)).flatMap(node =>
      node.gradingChecks.map(check => ({ ...check, nodeId: node.id }))),
  };
}

export function promptSelection(state) {
  assertReplayConsistent(state);
  return selectedPromptWork(state);
}

export function gradingSelection(state) {
  assertReplayConsistent(state);
  return selectedGradingWork(state);
}

function terminalOutcome(state, reason, blockedLevel = null) {
  const statuses = Object.values(state.nodes).map(node => node.status);
  const passed = statuses.filter(status => status === 'passed').length;
  const kind = passed === statuses.length ? 'passed' : passed > 0 ? 'partial' : 'failed';
  return { kind, reason, level: state.level,
    ...(blockedLevel === null ? {} : { blockedLevel }) };
}

function openLevel(state, level) {
  const nodes = nodesAt(state.definition, level);
  if (nodes.length === 0) {
    state.phase = 'terminal';
    state.terminalOutcome = terminalOutcome(state, 'graph-complete');
    return;
  }
  let active = 0;
  let passed = 0;
  for (const node of nodes) {
    const unlocked = node.dependencies.every(parentId => state.nodes[parentId].status === 'passed');
    if (state.nodes[node.id].status === 'exhausted') continue;
    if (state.nodes[node.id].status === 'passed' && unlocked) {
      passed += 1;
      continue;
    }
    state.nodes[node.id].status = unlocked ? 'active' : 'blocked';
    if (unlocked) active += 1;
  }
  if (active > 0) {
    state.level = level;
    state.phase = 'active';
    state.terminalOutcome = null;
  } else if (passed > 0) {
    openLevel(state, level + 1);
  } else {
    state.phase = 'terminal';
    state.terminalOutcome = terminalOutcome(state, 'no-unlocked-nodes', level);
  }
}

function initialDependencyState(definition) {
  const state = {
    schemaVersion: DEPENDENCY_MODE_SCHEMA_VERSION,
    policy: DEPENDENCY_MODE_POLICY,
    definition,
    phase: 'active',
    terminalOutcome: null,
    level: 1,
    nodes: Object.fromEntries(definition.nodes.map(node => [node.id, {
      status: 'locked',
      exhaustedAtLevel: null,
      checks: Object.fromEntries(node.gradingChecks.map(check => [check.id, null])),
    }])),
    strikes: Object.fromEntries(Object.entries(definition.strikes.levels).map(([level, budget]) =>
      [level, { initialBudget: budget, granted: 0, budget, used: 0 }])),
    attempts: [],
    grants: [],
    events: [],
  };
  openLevel(state, 1);
  return state;
}

export function initializeDependencyMode(input) {
  return initialDependencyState(compileDependencyMode(input));
}

function assertState(state) {
  if (!object(state) || state.schemaVersion !== DEPENDENCY_MODE_SCHEMA_VERSION
    || state.policy !== DEPENDENCY_MODE_POLICY) {
    throw new Error('invalid dependency mode state');
  }
  const definition = compileDependencyMode(state.definition);
  if (!['active', 'terminal'].includes(state.phase)) throw new Error('invalid dependency mode state phase');
  if (state.phase === 'active' && state.terminalOutcome !== null) {
    throw new Error('active dependency mode state cannot have a terminal outcome');
  }
  if (state.phase === 'terminal'
    && (!object(state.terminalOutcome) || !TERMINAL_OUTCOMES.has(state.terminalOutcome.kind)
      || !['graph-complete', 'no-unlocked-nodes'].includes(state.terminalOutcome.reason)
      || !Number.isInteger(state.terminalOutcome.level)
      || (state.terminalOutcome.blockedLevel !== undefined
        && !Number.isInteger(state.terminalOutcome.blockedLevel)))) {
    throw new Error('terminal dependency mode state requires a valid outcome');
  }
  if (!Number.isInteger(state.level) || !definition.strikes.levels[String(state.level)]) {
    throw new Error('invalid dependency mode state level');
  }
  const expectedNodeIds = new Set(definition.nodes.map(node => node.id));
  const actualNodeIds = Object.keys(state.nodes ?? {});
  if (actualNodeIds.length !== expectedNodeIds.size
    || actualNodeIds.some(nodeId => !expectedNodeIds.has(nodeId))) {
    throw new Error('invalid dependency mode state node set');
  }
  for (const node of definition.nodes) {
    const nodeState = state.nodes[node.id];
    if (!object(nodeState) || !NODE_STATUSES.has(nodeState.status) || !object(nodeState.checks)
      || (nodeState.exhaustedAtLevel !== null
        && (!Number.isInteger(nodeState.exhaustedAtLevel)
          || !definition.strikes.levels[String(nodeState.exhaustedAtLevel)]))
      || (nodeState.status === 'exhausted') !== (nodeState.exhaustedAtLevel !== null)) {
      throw new Error(`invalid dependency mode state for node ${node.id}`);
    }
    const expectedChecks = new Set(node.gradingChecks.map(check => check.id));
    const actualChecks = Object.keys(nodeState.checks);
    if (actualChecks.length !== expectedChecks.size
      || actualChecks.some(checkId => !expectedChecks.has(checkId))
      || actualChecks.some(checkId => ![null, 'pass', 'fail'].includes(nodeState.checks[checkId]))) {
      throw new Error(`invalid dependency mode check state for node ${node.id}`);
    }
  }
  const expectedLevels = new Set(Object.keys(definition.strikes.levels));
  const actualLevels = Object.keys(state.strikes ?? {});
  if (actualLevels.length !== expectedLevels.size
    || actualLevels.some(level => !expectedLevels.has(level))) {
    throw new Error('invalid dependency mode strike level set');
  }
  for (const [level, budget] of Object.entries(definition.strikes.levels)) {
    const counter = state.strikes?.[level];
    if (!object(counter) || counter.initialBudget !== budget
      || !Number.isInteger(counter.granted) || counter.granted < 0
      || counter.budget !== budget + counter.granted || !Number.isInteger(counter.used)
      || counter.used < 0 || counter.used > counter.budget) {
      throw new Error(`invalid dependency mode strike state for level ${level}`);
    }
  }
  if (!Array.isArray(state.attempts)) throw new Error('invalid dependency mode attempt history');
  const attemptIds = new Set();
  state.attempts.forEach((attempt, index) => {
    if (!object(attempt) || typeof attempt.attemptId !== 'string' || !attempt.attemptId
      || attemptIds.has(attempt.attemptId) || !Number.isInteger(attempt.level)
      || !definition.strikes.levels[String(attempt.level)]
      || !['conclusive', 'inconclusive'].includes(attempt.outcome)
      || (attempt.sourceSha256 !== undefined && !HASH.test(attempt.sourceSha256))
      || (attempt.selectionSha256 !== undefined && !HASH.test(attempt.selectionSha256))
      || (attempt.runId !== undefined && (typeof attempt.runId !== 'string' || !attempt.runId))
      || (attempt.outcome === 'inconclusive'
        && (typeof attempt.reason !== 'string' || !attempt.reason
          || !INCONCLUSIVE_CATEGORIES.has(attempt.category)))) {
      throw new Error(`invalid dependency mode attempt at index ${index}`);
    }
    if (attempt.evidence !== undefined) validateSourceEvidence(attempt.evidence,
      `attempts[${index}].evidence`);
    attemptIds.add(attempt.attemptId);
  });
  if (!Array.isArray(state.grants) || !Array.isArray(state.events)) {
    throw new Error('invalid dependency mode continuation history');
  }
  return definition;
}

function validateConclusiveResult(state, result) {
  if (!Array.isArray(result.nodes)) throw new Error('conclusive result nodes must be an array');
  const nodeIds = gradingNodeIds(state);
  const selectedNodes = new Set(nodeIds);
  const selected = {
    nodeIds,
    checks: state.definition.nodes.filter(node => selectedNodes.has(node.id)).flatMap(node =>
      node.gradingChecks.map(check => ({ ...check, nodeId: node.id }))),
  };
  const expectedNodes = new Map(selected.nodeIds.map(nodeId => [nodeId,
    new Set(selected.checks.filter(check => check.nodeId === nodeId).map(check => check.id))]));
  const actualNodes = new Map();
  for (const [index, nodeResult] of result.nodes.entries()) {
    strictObject(nodeResult, `result.nodes[${index}]`, new Set(['id', 'checks']));
    nonEmptyString(nodeResult.id, `result.nodes[${index}].id`);
    if (!expectedNodes.has(nodeResult.id)) throw new Error(`result includes unselected node ${nodeResult.id}`);
    if (actualNodes.has(nodeResult.id)) throw new Error(`result repeats node ${nodeResult.id}`);
    if (!Array.isArray(nodeResult.checks)) throw new Error(`result node ${nodeResult.id} checks must be an array`);
    const checks = new Map();
    for (const [checkIndex, check] of nodeResult.checks.entries()) {
      strictObject(check, `result.nodes[${index}].checks[${checkIndex}]`, new Set(['id', 'outcome']));
      nonEmptyString(check.id, `result.nodes[${index}].checks[${checkIndex}].id`);
      if (!expectedNodes.get(nodeResult.id).has(check.id)) {
        throw new Error(`result includes unselected check ${check.id} for ${nodeResult.id}`);
      }
      if (checks.has(check.id)) throw new Error(`result repeats check ${check.id}`);
      if (!['pass', 'fail'].includes(check.outcome)) {
        throw new Error(`result check ${check.id} outcome must be pass or fail`);
      }
      checks.set(check.id, check.outcome);
    }
    const missing = [...expectedNodes.get(nodeResult.id)].filter(checkId => !checks.has(checkId));
    if (missing.length) throw new Error(`result node ${nodeResult.id} is missing checks: ${missing.join(', ')}`);
    actualNodes.set(nodeResult.id, checks);
  }
  const missingNodes = [...expectedNodes.keys()].filter(nodeId => !actualNodes.has(nodeId));
  if (missingNodes.length) throw new Error(`result is missing nodes: ${missingNodes.join(', ')}`);
  return actualNodes;
}

function applyDependencyResult(inputState, inputResult) {
  if (inputState.phase !== 'active') throw new Error('cannot record a result after progression terminates');
  const result = structuredClone(inputResult);
  strictObject(result, 'result', new Set([
    'attemptId', 'runId', 'outcome', 'category', 'reason', 'nodes',
    'sourceSha256', 'selectionSha256', 'evidence',
  ]));
  nonEmptyString(result.attemptId, 'result.attemptId');
  if (result.sourceSha256 !== undefined && !HASH.test(result.sourceSha256)) {
    throw new Error('result.sourceSha256 must be a SHA-256 identity');
  }
  if (result.selectionSha256 !== undefined && !HASH.test(result.selectionSha256)) {
    throw new Error('result.selectionSha256 must be a SHA-256 identity');
  }
  if (result.runId !== undefined && (typeof result.runId !== 'string' || !result.runId)) {
    throw new Error('result.runId must be a non-empty string');
  }
  if (result.evidence !== undefined) {
    validateSourceEvidence(result.evidence, 'result.evidence');
    if (!result.runId
      || result.attemptId !== `${result.runId}-progression-${inputState.attempts.length + 1}`) {
      throw new Error('result.attemptId does not match its owned progression sequence');
    }
  }
  if (inputState.attempts.some(attempt => attempt.attemptId === result.attemptId)) {
    throw new Error(`duplicate attempt id ${result.attemptId}`);
  }
  if (!['conclusive', 'inconclusive'].includes(result.outcome)) {
    throw new Error('result.outcome must be conclusive or inconclusive');
  }
  const state = structuredClone(inputState);
  if (result.outcome === 'inconclusive') {
    nonEmptyString(result.reason, 'result.reason');
    if (!INCONCLUSIVE_CATEGORIES.has(result.category)) {
      throw new Error(`result.category must be one of ${[...INCONCLUSIVE_CATEGORIES].join(', ')}`);
    }
    if (result.nodes !== undefined) throw new Error('inconclusive results cannot contain node grades');
    state.attempts.push({ attemptId: result.attemptId, level: state.level,
      outcome: result.outcome, category: result.category, reason: result.reason,
      ...(result.runId ? { runId: result.runId } : {}),
      ...(result.evidence ? { evidence: result.evidence } : {}),
      ...(result.sourceSha256 ? { sourceSha256: result.sourceSha256 } : {}),
      ...(result.selectionSha256 ? { selectionSha256: result.selectionSha256 } : {}) });
    return state;
  }
  if (result.reason !== undefined || result.category !== undefined) {
    throw new Error('conclusive results cannot contain inconclusive details');
  }

  const actual = validateConclusiveResult(state, result);
  const nodesById = new Map(state.definition.nodes.map(node => [node.id, node]));
  const selectedNodeIds = gradingNodeIds(state);
  for (const nodeId of selectedNodeIds) {
    const node = nodesById.get(nodeId);
    const outcomes = actual.get(nodeId);
    state.nodes[nodeId].checks = Object.fromEntries(node.gradingChecks.map(check =>
      [check.id, outcomes.get(check.id)]));
  }
  for (const nodeId of selectedNodeIds) {
    const node = nodesById.get(nodeId);
    const checksPass = node.gradingChecks.every(check => state.nodes[nodeId].checks[check.id] === 'pass');
    const dependenciesPass = node.dependencies.every(parentId =>
      state.nodes[parentId].status === 'passed');
    state.nodes[nodeId].exhaustedAtLevel = null;
    if (checksPass && dependenciesPass) {
      state.nodes[nodeId].status = 'passed';
    } else if (node.level < state.level || state.nodes[nodeId].status === 'passed') {
      state.nodes[nodeId].status = 'regressed';
    } else {
      state.nodes[nodeId].status = 'active';
    }
  }
  state.attempts.push({ attemptId: result.attemptId, level: state.level, outcome: result.outcome,
    ...(result.runId ? { runId: result.runId } : {}),
    ...(result.evidence ? { evidence: result.evidence } : {}),
    ...(result.sourceSha256 ? { sourceSha256: result.sourceSha256 } : {}),
    ...(result.selectionSha256 ? { selectionSha256: result.selectionSha256 } : {}) });

  const unresolved = nodesAt(state.definition, state.level)
    .filter(node => ['active', 'regressed'].includes(state.nodes[node.id].status));
  if (unresolved.length > 0) {
    const counter = state.strikes[String(state.level)];
    counter.used += 1;
    if (counter.used < counter.budget) return state;
    for (const nodeId of currentPromptNodeIds(state)) {
      state.nodes[nodeId].status = 'exhausted';
      state.nodes[nodeId].exhaustedAtLevel = state.level;
    }
  }
  openLevel(state, state.level + 1);
  return state;
}

function applyStrikeGrant(inputState, inputGrant) {
  if (inputState.phase !== 'terminal') throw new Error('strikes can be granted only after progression terminates');
  const grant = structuredClone(inputGrant);
  strictObject(grant, 'grant', new Set(['grantId', 'level', 'strikes']));
  nonEmptyString(grant.grantId, 'grant.grantId');
  positiveInteger(grant.level, 'grant.level');
  positiveInteger(grant.strikes, 'grant.strikes');
  if (inputState.grants.some(item => item.grantId === grant.grantId)) {
    throw new Error(`duplicate grant id ${grant.grantId}`);
  }
  const counter = inputState.strikes[String(grant.level)];
  if (!counter) throw new Error(`grant level ${grant.level} is outside the graph`);
  if (!Number.isSafeInteger(counter.budget + grant.strikes)) {
    throw new Error(`grant level ${grant.level} exceeds the safe strike limit`);
  }
  const eligible = inputState.definition.nodes.filter(node =>
    inputState.nodes[node.id].status === 'exhausted'
    && inputState.nodes[node.id].exhaustedAtLevel === grant.level);
  if (eligible.length === 0) {
    throw new Error(`grant level ${grant.level} has no exhausted repair target`);
  }
  const state = structuredClone(inputState);
  state.level = grant.level;
  state.phase = 'active';
  state.terminalOutcome = null;
  state.strikes[String(grant.level)].granted += grant.strikes;
  state.strikes[String(grant.level)].budget += grant.strikes;
  for (const node of eligible) {
    state.nodes[node.id].status = node.level < grant.level ? 'regressed' : 'active';
    state.nodes[node.id].exhaustedAtLevel = null;
  }
  for (const node of state.definition.nodes.filter(node => node.level > grant.level)) {
    const nodeState = state.nodes[node.id];
    nodeState.checks = Object.fromEntries(node.gradingChecks.map(check => [check.id, null]));
    const counter = state.strikes[String(node.level)];
    if (counter.used >= counter.budget) {
      nodeState.status = 'exhausted';
      nodeState.exhaustedAtLevel = node.level;
    } else {
      nodeState.status = 'locked';
      nodeState.exhaustedAtLevel = null;
    }
  }
  state.grants.push(grant);
  return state;
}

function validateEvent(event, index) {
  strictObject(event, `events[${index}]`, new Set(['sequence', 'type', 'result', 'grant']));
  if (event.sequence !== index + 1) throw new Error(`event sequence ${event.sequence} must be ${index + 1}`);
  if (event.type === 'attempt-recorded') {
    if (event.result === undefined || event.grant !== undefined) {
      throw new Error(`attempt event ${event.sequence} must contain only a result`);
    }
  } else if (event.type === 'strikes-granted') {
    if (event.grant === undefined || event.result !== undefined) {
      throw new Error(`grant event ${event.sequence} must contain only a grant`);
    }
  } else {
    throw new Error(`unknown dependency mode event ${JSON.stringify(event.type)}`);
  }
}

export function replayDependencyMode(inputDefinition, inputEvents = []) {
  const definition = compileDependencyMode(inputDefinition);
  if (!Array.isArray(inputEvents)) throw new Error('dependency mode events must be an array');
  let state = initialDependencyState(definition);
  inputEvents.forEach((inputEvent, index) => {
    const event = structuredClone(inputEvent);
    validateEvent(event, index);
    state = event.type === 'attempt-recorded'
      ? applyDependencyResult(state, event.result)
      : applyStrikeGrant(state, event.grant);
    state.events.push(event);
  });
  assertState(state);
  return state;
}

function assertReplayConsistent(state) {
  assertState(state);
  const replayed = replayDependencyMode(state.definition, state.events);
  if (!isDeepStrictEqual(replayed, state)) {
    throw new Error('dependency mode snapshot contradicts its event history');
  }
  return state.definition;
}

export function resumeDependencyMode(snapshot) {
  assertReplayConsistent(snapshot);
  return replayDependencyMode(snapshot.definition, snapshot.events);
}

export function recordDependencyResult(inputState, inputResult) {
  assertReplayConsistent(inputState);
  const event = { sequence: inputState.events.length + 1, type: 'attempt-recorded',
    result: structuredClone(inputResult) };
  return replayDependencyMode(inputState.definition, [...inputState.events, event]);
}

export function grantDependencyStrikes(inputState, grant) {
  assertReplayConsistent(inputState);
  const event = { sequence: inputState.events.length + 1, type: 'strikes-granted',
    grant: structuredClone(grant) };
  return replayDependencyMode(inputState.definition, [...inputState.events, event]);
}

export function nextDependencyAction(state) {
  assertReplayConsistent(state);
  if (state.phase === 'terminal') return { type: 'terminal', outcome: { ...state.terminalOutcome } };
  const prompt = selectedPromptWork(state);
  const grading = selectedGradingWork(state);
  const hasConclusiveAttempt = state.attempts.some(attempt =>
    attempt.level === state.level && attempt.outcome === 'conclusive');
  const strikes = state.strikes[String(state.level)];
  return {
    type: hasConclusiveAttempt ? 'repair' : 'build',
    level: state.level,
    strikes: { ...strikes, remaining: strikes.budget - strikes.used },
    prompt,
    grading,
  };
}

export function activeDependencyNodes(state) {
  assertReplayConsistent(state);
  return currentPromptNodeIds(state);
}

function nodePoints(definition, state, nodeId) {
  const node = definition.nodes.find(candidate => candidate.id === nodeId);
  const passedPoints = node.gradingChecks.reduce((total, check) =>
    total + (state.nodes[nodeId].checks[check.id] === 'pass' ? check.points : 0), 0);
  const failedPoints = node.gradingChecks.reduce((total, check) =>
    total + (state.nodes[nodeId].checks[check.id] === 'fail' ? check.points : 0), 0);
  const gradedPoints = passedPoints + failedPoints;
  const availablePoints = node.gradingChecks.reduce((total, check) => total + check.points, 0);
  return { passedPoints, failedPoints, gradedPoints,
    ungradedPoints: availablePoints - gradedPoints, availablePoints };
}

const percentage = ({ passedPoints, availablePoints }) => (passedPoints / availablePoints) * 100;

export function scoreDependencyMode(state) {
  const definition = assertReplayConsistent(state);
  const questlines = definition.questlines.map(questline => {
    const points = questline.nodes.reduce((total, nodeId) => {
      const node = nodePoints(definition, state, nodeId);
      total.passedPoints += node.passedPoints;
      total.failedPoints += node.failedPoints;
      total.gradedPoints += node.gradedPoints;
      total.ungradedPoints += node.ungradedPoints;
      total.availablePoints += node.availablePoints;
      return total;
    }, { passedPoints: 0, failedPoints: 0, gradedPoints: 0, ungradedPoints: 0,
      availablePoints: 0 });
    const final = state.phase === 'terminal';
    return { id: questline.id, title: questline.title, ...points,
      percentage: final ? percentage(points) : null,
      provisionalPercentage: null };
  });
  const uniqueChecks = definition.nodes.reduce((total, node) => {
    const points = nodePoints(definition, state, node.id);
    total.passedPoints += points.passedPoints;
    total.failedPoints += points.failedPoints;
    total.gradedPoints += points.gradedPoints;
    total.ungradedPoints += points.ungradedPoints;
    total.availablePoints += points.availablePoints;
    return total;
  }, { passedPoints: 0, failedPoints: 0, gradedPoints: 0, ungradedPoints: 0,
    availablePoints: 0 });
  const final = state.phase === 'terminal';
  const inconclusiveAttempts = state.attempts.filter(attempt => attempt.outcome === 'inconclusive').length;
  const inconclusiveByCategory = Object.fromEntries([...INCONCLUSIVE_CATEGORIES]
    .map(category => [category, state.attempts.filter(attempt => attempt.category === category).length]));
  return {
    status: final ? 'final' : 'provisional',
    terminalOutcome: final ? { ...state.terminalOutcome } : null,
    attempts: { total: state.attempts.length, inconclusive: inconclusiveAttempts,
      inconclusiveByCategory,
      conclusive: state.attempts.length - inconclusiveAttempts },
    questlines,
    averagePercentage: final
      ? questlines.reduce((total, questline) => total + questline.percentage, 0) / questlines.length
      : null,
    uniqueChecks: { ...uniqueChecks,
      percentage: final ? percentage(uniqueChecks) : null,
      provisionalPercentage: null },
  };
}

export const dependencyModePolicy = Object.freeze({
  id: DEPENDENCY_MODE_POLICY,
  compile: compileDependencyMode,
  initialize: initializeDependencyMode,
  activeNodes: activeDependencyNodes,
  promptSelection,
  gradingSelection,
  recordResult: recordDependencyResult,
  grantStrikes: grantDependencyStrikes,
  resume: resumeDependencyMode,
  nextAction: nextDependencyAction,
  score: scoreDependencyMode,
});
