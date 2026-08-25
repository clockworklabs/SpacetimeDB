export const DEPENDENCY_MODE_SCHEMA_VERSION = 1;
export const DEPENDENCY_MODE_POLICY = 'dependency-gated';

const ID = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/;
const NODE_STATUSES = new Set(['locked', 'active', 'passed', 'exhausted', 'regressed', 'blocked']);

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const fail = (at, message) => { throw new Error(`invalid dependency mode at ${at}: ${message}`); };

function strictObject(value, at, fields) {
  if (!object(value)) fail(at, 'must be an object');
  for (const key of Object.keys(value)) {
    if (!fields.has(key)) fail(`${at}.${key}`, 'unknown field');
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
  if (!Number.isInteger(value) || value < 1) fail(at, 'must be a positive integer');
  return value;
}

function nonNegativeInteger(value, at) {
  if (!Number.isInteger(value) || value < 0) fail(at, 'must be a non-negative integer');
  return value;
}

function uniqueStrings(value, at, { exactRefs = false } = {}) {
  if (!Array.isArray(value) || value.length === 0) fail(at, 'must be a non-empty array');
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
    'schemaVersion', 'kind', 'id', 'version', 'policy', 'strikes', 'nodes', 'questlines',
  ]));
  if (definition.schemaVersion !== DEPENDENCY_MODE_SCHEMA_VERSION) {
    fail(`${source}.schemaVersion`, `must be ${DEPENDENCY_MODE_SCHEMA_VERSION}`);
  }
  if (definition.kind !== 'progression-mode') fail(`${source}.kind`, 'must be "progression-mode"');
  identifier(definition.id, `${source}.id`);
  semanticVersion(definition.version, `${source}.version`);
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
      'id', 'level', 'featureRefs', 'promptModules', 'gradingChecks', 'dependencies',
    ]));
    identifier(node.id, `${at}.id`);
    if (nodeIds.has(node.id)) fail(`${at}.id`, `duplicates ${JSON.stringify(node.id)}`);
    nodeIds.add(node.id);
    positiveInteger(node.level, `${at}.level`);
    node.featureRefs = uniqueStrings(node.featureRefs, `${at}.featureRefs`, { exactRefs: true }).sort();
    node.promptModules = uniqueStrings(node.promptModules, `${at}.promptModules`).sort();
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
      nonNegativeInteger(check.points, `${checkAt}.points`);
    });
    if (!node.gradingChecks.some(check => check.points > 0)) {
      fail(`${at}.gradingChecks`, 'must contain at least one scored check');
    }
    node.gradingChecks.sort((left, right) => left.id.localeCompare(right.id));
    if (!Array.isArray(node.dependencies)) fail(`${at}.dependencies`, 'must be an array');
    const dependencies = new Set();
    node.dependencies.forEach((dependency, dependencyIndex) => {
      identifier(dependency, `${at}.dependencies[${dependencyIndex}]`);
      if (dependencies.has(dependency)) {
        fail(`${at}.dependencies[${dependencyIndex}]`, `duplicates ${JSON.stringify(dependency)}`);
      }
      dependencies.add(dependency);
    });
    node.dependencies = [...dependencies].sort();
  });

  definition.nodes.sort((left, right) => left.level - right.level || left.id.localeCompare(right.id));
  const levels = [...new Set(definition.nodes.map(node => node.level))].sort((a, b) => a - b);
  if (levels[0] !== 1 || levels.some((level, index) => level !== index + 1)) {
    fail(`${source}.nodes`, 'levels must start at 1 and be contiguous');
  }
  const nodesById = new Map(definition.nodes.map(node => [node.id, node]));
  for (const node of definition.nodes) {
    const at = `${source}.nodes.${node.id}.dependencies`;
    for (const dependency of node.dependencies) {
      if (!nodesById.has(dependency)) fail(at, `unknown parent ${JSON.stringify(dependency)}`);
    }
  }
  assertAcyclic(nodesById);
  for (const node of definition.nodes) {
    const at = `${source}.nodes.${node.id}.dependencies`;
    if (node.level === 1 && node.dependencies.length !== 0) fail(at, 'level 1 nodes cannot have dependencies');
    if (node.level > 1 && node.dependencies.length === 0) fail(at, 'nodes after level 1 require a parent');
    for (const dependency of node.dependencies) {
      const parent = nodesById.get(dependency);
      if (parent.level !== node.level - 1) {
        fail(at, `parent ${JSON.stringify(dependency)} must be in level ${node.level - 1}`);
      }
    }
  }
  definition.strikes = { levels: compileStrikeBudgets(definition.strikes, levels) };

  if (!Array.isArray(definition.questlines) || definition.questlines.length === 0) {
    fail(`${source}.questlines`, 'must be a non-empty array');
  }
  const questlineIds = new Set();
  const coveredNodes = new Set();
  definition.questlines.forEach((questline, index) => {
    const at = `${source}.questlines[${index}]`;
    strictObject(questline, at, new Set(['id', 'title', 'nodes']));
    identifier(questline.id, `${at}.id`);
    if (questlineIds.has(questline.id)) fail(`${at}.id`, `duplicates ${JSON.stringify(questline.id)}`);
    questlineIds.add(questline.id);
    nonEmptyString(questline.title, `${at}.title`);
    questline.nodes = uniqueStrings(questline.nodes, `${at}.nodes`);
    const path = questline.nodes.map((nodeId, nodeIndex) => {
      const node = nodesById.get(nodeId);
      if (!node) fail(`${at}.nodes[${nodeIndex}]`, `unknown node ${JSON.stringify(nodeId)}`);
      coveredNodes.add(nodeId);
      return node;
    });
    if (path[0].level !== 1) fail(`${at}.nodes[0]`, 'questlines must start at level 1');
    for (let nodeIndex = 1; nodeIndex < path.length; nodeIndex += 1) {
      const parent = path[nodeIndex - 1];
      const child = path[nodeIndex];
      if (child.level !== parent.level + 1 || !child.dependencies.includes(parent.id)) {
        fail(`${at}.nodes[${nodeIndex}]`, `does not directly depend on ${JSON.stringify(parent.id)}`);
      }
    }
  });
  const orphaned = definition.nodes.map(node => node.id).filter(nodeId => !coveredNodes.has(nodeId));
  if (orphaned.length) fail(`${source}.questlines`, `do not cover nodes: ${orphaned.join(', ')}`);
  definition.questlines.sort((left, right) => left.id.localeCompare(right.id));
  return definition;
}

function nodesAt(definition, level) {
  return definition.nodes.filter(node => node.level === level);
}

function currentPromptNodeIds(state) {
  if (state.phase !== 'active') return [];
  const current = nodesAt(state.definition, state.level)
    .filter(node => state.nodes[node.id].status === 'active')
    .map(node => node.id);
  const required = new Set(current);
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
  const selected = new Set(promptIds);
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

export function promptSelection(state) {
  assertState(state);
  const nodeIds = currentPromptNodeIds(state);
  return {
    nodeIds,
    featureRefs: [...new Set(selectionFor(state, nodeIds, 'featureRefs'))].sort(),
    promptModules: [...new Set(selectionFor(state, nodeIds, 'promptModules'))].sort(),
  };
}

export function gradingSelection(state) {
  assertState(state);
  const nodeIds = gradingNodeIds(state);
  const selected = new Set(nodeIds);
  return {
    nodeIds,
    checks: state.definition.nodes.filter(node => selected.has(node.id)).flatMap(node =>
      node.gradingChecks.map(check => ({ ...check, nodeId: node.id }))),
  };
}

function openLevel(state, level) {
  const nodes = nodesAt(state.definition, level);
  if (nodes.length === 0) {
    state.phase = 'complete';
    return;
  }
  state.level = level;
  let active = 0;
  for (const node of nodes) {
    const unlocked = node.dependencies.every(parentId => state.nodes[parentId].status === 'passed');
    state.nodes[node.id].status = unlocked ? 'active' : 'blocked';
    if (unlocked) active += 1;
  }
  state.phase = active > 0 ? 'active' : 'complete';
}

export function initializeDependencyMode(input) {
  const definition = compileDependencyMode(input);
  const state = {
    schemaVersion: DEPENDENCY_MODE_SCHEMA_VERSION,
    policy: DEPENDENCY_MODE_POLICY,
    definition,
    phase: 'active',
    level: 1,
    nodes: Object.fromEntries(definition.nodes.map(node => [node.id, {
      status: 'locked',
      checks: Object.fromEntries(node.gradingChecks.map(check => [check.id, null])),
    }])),
    strikes: Object.fromEntries(Object.entries(definition.strikes.levels).map(([level, budget]) =>
      [level, { budget, used: 0 }])),
    attempts: [],
  };
  openLevel(state, 1);
  return state;
}

function assertState(state) {
  if (!object(state) || state.schemaVersion !== DEPENDENCY_MODE_SCHEMA_VERSION
    || state.policy !== DEPENDENCY_MODE_POLICY) {
    throw new Error('invalid dependency mode state');
  }
  const definition = compileDependencyMode(state.definition);
  if (!['active', 'complete'].includes(state.phase)) throw new Error('invalid dependency mode state phase');
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
    if (!object(nodeState) || !NODE_STATUSES.has(nodeState.status) || !object(nodeState.checks)) {
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
    if (!object(counter) || counter.budget !== budget || !Number.isInteger(counter.used)
      || counter.used < 0 || counter.used > budget) {
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
      || (attempt.outcome === 'inconclusive'
        && (typeof attempt.reason !== 'string' || !attempt.reason))) {
      throw new Error(`invalid dependency mode attempt at index ${index}`);
    }
    attemptIds.add(attempt.attemptId);
  });
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

export function recordDependencyResult(inputState, inputResult) {
  assertState(inputState);
  if (inputState.phase !== 'active') throw new Error('cannot record a result after progression completes');
  const result = structuredClone(inputResult);
  strictObject(result, 'result', new Set(['attemptId', 'outcome', 'reason', 'nodes']));
  nonEmptyString(result.attemptId, 'result.attemptId');
  if (inputState.attempts.some(attempt => attempt.attemptId === result.attemptId)) {
    throw new Error(`duplicate attempt id ${result.attemptId}`);
  }
  if (!['conclusive', 'inconclusive'].includes(result.outcome)) {
    throw new Error('result.outcome must be conclusive or inconclusive');
  }
  const state = structuredClone(inputState);
  if (result.outcome === 'inconclusive') {
    nonEmptyString(result.reason, 'result.reason');
    if (result.nodes !== undefined) throw new Error('inconclusive results cannot contain node grades');
    state.attempts.push({ attemptId: result.attemptId, level: state.level,
      outcome: result.outcome, reason: result.reason });
    return state;
  }
  if (result.reason !== undefined) throw new Error('conclusive results cannot contain an inconclusive reason');

  const actual = validateConclusiveResult(state, result);
  const nodesById = new Map(state.definition.nodes.map(node => [node.id, node]));
  for (const nodeId of gradingNodeIds(state)) {
    const node = nodesById.get(nodeId);
    const outcomes = actual.get(nodeId);
    state.nodes[nodeId].checks = Object.fromEntries(node.gradingChecks.map(check =>
      [check.id, outcomes.get(check.id)]));
  }
  for (const nodeId of gradingNodeIds(state)) {
    const node = nodesById.get(nodeId);
    const checksPass = node.gradingChecks.every(check => state.nodes[nodeId].checks[check.id] === 'pass');
    const dependenciesPass = node.dependencies.every(parentId => state.nodes[parentId].status === 'passed');
    if (checksPass && dependenciesPass) {
      state.nodes[nodeId].status = 'passed';
    } else if (node.level < state.level || state.nodes[nodeId].status === 'passed') {
      state.nodes[nodeId].status = 'regressed';
    } else {
      state.nodes[nodeId].status = 'active';
    }
  }
  state.attempts.push({ attemptId: result.attemptId, level: state.level, outcome: result.outcome });

  const unresolved = nodesAt(state.definition, state.level)
    .filter(node => state.nodes[node.id].status === 'active');
  if (unresolved.length > 0) {
    const counter = state.strikes[String(state.level)];
    counter.used += 1;
    if (counter.used < counter.budget) return state;
    for (const node of unresolved) state.nodes[node.id].status = 'exhausted';
  }
  openLevel(state, state.level + 1);
  return state;
}

export function nextDependencyAction(state) {
  assertState(state);
  if (state.phase === 'complete') return { type: 'complete' };
  const prompt = promptSelection(state);
  const grading = gradingSelection(state);
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
  assertState(state);
  return currentPromptNodeIds(state);
}

function nodePoints(definition, state, nodeId) {
  const node = definition.nodes.find(candidate => candidate.id === nodeId);
  const passedPoints = node.gradingChecks.reduce((total, check) =>
    total + (state.nodes[nodeId].checks[check.id] === 'pass' ? check.points : 0), 0);
  const availablePoints = node.gradingChecks.reduce((total, check) => total + check.points, 0);
  return { passedPoints, availablePoints };
}

const percentage = ({ passedPoints, availablePoints }) => (passedPoints / availablePoints) * 100;

export function scoreDependencyMode(state) {
  const definition = assertState(state);
  const questlines = definition.questlines.map(questline => {
    const points = questline.nodes.reduce((total, nodeId) => {
      const node = nodePoints(definition, state, nodeId);
      total.passedPoints += node.passedPoints;
      total.availablePoints += node.availablePoints;
      return total;
    }, { passedPoints: 0, availablePoints: 0 });
    return { id: questline.id, title: questline.title, ...points, percentage: percentage(points) };
  });
  const uniqueChecks = definition.nodes.reduce((total, node) => {
    const points = nodePoints(definition, state, node.id);
    total.passedPoints += points.passedPoints;
    total.availablePoints += points.availablePoints;
    return total;
  }, { passedPoints: 0, availablePoints: 0 });
  return {
    questlines,
    averagePercentage: questlines.reduce((total, questline) => total + questline.percentage, 0)
      / questlines.length,
    uniqueChecks: { ...uniqueChecks, percentage: percentage(uniqueChecks) },
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
  nextAction: nextDependencyAction,
  score: scoreDependencyMode,
});
