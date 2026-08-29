import { canonicalDefinitionJson, canonicalizeDefinition }
  from '../composition/definition-plan.mjs';
import { criterionEvidence, evidencePassed } from '../evidence/check-evidence.js';
import { sha256 } from '../evidence/provenance.js';

export const EXECUTION_SHARD_SCHEMA_VERSION = 1;

function fail(message) {
  throw new Error(`invalid grade shard input: ${message}`);
}

function object(value, at) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail(`${at} must be an object`);
  return value;
}

function nonEmpty(value, at) {
  if (typeof value !== 'string' || !value.trim()) fail(`${at} must be a non-empty string`);
  return value;
}

function checkDescriptor(check, at) {
  object(check, at);
  const stableKey = nonEmpty(check.stableKey, `${at}.stableKey`);
  const executionId = nonEmpty(check.executionId, `${at}.executionId`);
  if (!Number.isSafeInteger(check.points) || check.points < 0) {
    fail(`${at}.points must be a non-negative integer`);
  }
  return { stableKey, executionId, points: check.points };
}

function exactKeys(items, at) {
  if (!Array.isArray(items)) fail(`${at} must be an array`);
  const keys = items.map((item, index) => nonEmpty(item?.stableKey, `${at}[${index}].stableKey`));
  if (new Set(keys).size !== keys.length) fail(`${at} contains duplicate checks`);
  return keys;
}

function same(left, right) {
  return canonicalDefinitionJson(left) === canonicalDefinitionJson(right);
}

function planBody(plan) {
  return {
    schemaVersion: plan.schemaVersion,
    identities: plan.identities,
    units: plan.units,
    shards: plan.shards,
  };
}

function validatePlan(input) {
  const plan = object(input, 'plan');
  if (plan.schemaVersion !== EXECUTION_SHARD_SCHEMA_VERSION) fail('plan schemaVersion is unsupported');
  object(plan.identities, 'plan.identities');
  if (Object.keys(plan.identities).length === 0) fail('plan.identities must not be empty');
  if (!Array.isArray(plan.units) || plan.units.length === 0) fail('plan.units must not be empty');
  if (!Array.isArray(plan.shards) || plan.shards.length === 0) fail('plan.shards must not be empty');
  nonEmpty(plan.contentSha256, 'plan.contentSha256');
  if (!/^[a-f0-9]{64}$/.test(plan.contentSha256)) fail('plan.contentSha256 must be a SHA-256 digest');
  const expectedHash = sha256(canonicalDefinitionJson(planBody(plan)));
  if (plan.contentSha256 !== expectedHash) fail('plan contentSha256 does not match its content');

  const unitById = new Map();
  const plannedChecks = new Set();
  for (const [index, unit] of plan.units.entries()) {
    object(unit, `plan.units[${index}]`);
    if (unit.ordinal !== index + 1) fail(`plan.units[${index}].ordinal is not contiguous`);
    const executionId = nonEmpty(unit.executionId, `plan.units[${index}].executionId`);
    const source = nonEmpty(unit.source, `plan.units[${index}].source`);
    if (unitById.has(executionId)) fail(`plan.units contains duplicate execution ${executionId}`);
    if (!Array.isArray(unit.checks) || unit.checks.length === 0) {
      fail(`plan.units[${index}].checks must not be empty`);
    }
    for (const [checkIndex, check] of unit.checks.entries()) {
      object(check, `plan.units[${index}].checks[${checkIndex}]`);
      const key = nonEmpty(check.stableKey,
        `plan.units[${index}].checks[${checkIndex}].stableKey`);
      if (!Number.isSafeInteger(check.points) || check.points < 0) {
        fail(`plan.units[${index}].checks[${checkIndex}].points must be a non-negative integer`);
      }
      if (plannedChecks.has(key)) fail(`plan.units contains duplicate check ${key}`);
      plannedChecks.add(key);
    }
    unitById.set(executionId, unit);
  }

  const shardIds = new Set();
  const assignedUnits = new Set();
  for (const [index, shard] of plan.shards.entries()) {
    object(shard, `plan.shards[${index}]`);
    const id = nonEmpty(shard.id, `plan.shards[${index}].id`);
    if (shardIds.has(id)) fail(`plan.shards contains duplicate shard ${id}`);
    if (!Array.isArray(shard.units) || shard.units.length === 0) {
      fail(`plan.shards[${index}].units must not be empty`);
    }
    let checkCount = 0;
    for (const executionId of shard.units) {
      nonEmpty(executionId, `plan.shards[${index}].units`);
      if (!unitById.has(executionId)) fail(`plan shard ${id} has unknown execution ${executionId}`);
      if (assignedUnits.has(executionId)) fail(`plan assigns execution ${executionId} more than once`);
      assignedUnits.add(executionId);
      checkCount += unitById.get(executionId).checks.length;
    }
    if (shard.checkCount !== checkCount) fail(`plan shard ${id} has an incorrect checkCount`);
    shardIds.add(id);
  }
  const missingUnits = [...unitById.keys()].filter(id => !assignedUnits.has(id));
  if (missingUnits.length) fail(`plan does not assign execution ${missingUnits.join(', ')}`);
  return plan;
}

// Build deterministic work units at the existing recipe execution/source boundary.
// This function does not start workers or assign runtime resources.
export function planExecutionSourceShards({ execution, checks, identities },
  { maxWorkers = 1 } = {}) {
  if (!Array.isArray(execution) || execution.length === 0) fail('execution must not be empty');
  if (!Array.isArray(checks) || checks.length === 0) fail('checks must not be empty');
  if (!Number.isSafeInteger(maxWorkers) || maxWorkers < 1) fail('maxWorkers must be a positive integer');
  const frozenIdentities = canonicalizeDefinition(object(identities, 'identities'));

  const executionById = new Map();
  for (const [index, entry] of execution.entries()) {
    object(entry, `execution[${index}]`);
    const id = nonEmpty(entry.id, `execution[${index}].id`);
    const source = nonEmpty(entry.source, `execution[${index}].source`);
    if (executionById.has(id)) fail(`execution contains duplicate id ${id}`);
    executionById.set(id, { id, source, order: index });
  }

  const selected = checks.map((check, index) => checkDescriptor(check, `checks[${index}]`));
  const stableKeys = selected.map(check => check.stableKey);
  if (new Set(stableKeys).size !== stableKeys.length) fail('checks contains duplicate stable keys');
  const checksByExecution = new Map();
  for (const check of selected) {
    if (!executionById.has(check.executionId)) {
      fail(`check ${check.stableKey} maps to unknown execution ${check.executionId}`);
    }
    const grouped = checksByExecution.get(check.executionId) ?? [];
    grouped.push({ stableKey: check.stableKey, points: check.points });
    checksByExecution.set(check.executionId, grouped);
  }

  const units = [...executionById.values()]
    .filter(entry => checksByExecution.has(entry.id))
    .map((entry, index) => ({
      ordinal: index + 1,
      executionId: entry.id,
      source: entry.source,
      checks: checksByExecution.get(entry.id),
    }));
  const workerCount = Math.min(maxWorkers, units.length);
  const shards = Array.from({ length: workerCount }, (_, index) => ({
    id: `grade-shard-${String(index + 1).padStart(3, '0')}`,
    units: [],
    checkCount: 0,
  }));
  for (const unit of units) {
    const shard = shards.reduce((least, candidate) =>
      candidate.checkCount < least.checkCount ? candidate : least, shards[0]);
    shard.units.push(unit.executionId);
    shard.checkCount += unit.checks.length;
  }

  const body = canonicalizeDefinition({
    schemaVersion: EXECUTION_SHARD_SCHEMA_VERSION,
    identities: frozenIdentities,
    units,
    shards,
  });
  return { ...body, contentSha256: sha256(canonicalDefinitionJson(body)) };
}

function validateReportChecks(report, expected, at) {
  object(report, at);
  const selectionKeys = exactKeys(report.selection?.checks, `${at}.selection.checks`);
  const evidence = [];
  const featureIds = new Set();
  let evidenceTotal = 0;
  let evidenceMax = 0;
  if (!Array.isArray(report.features)) fail(`${at}.features must be an array`);
  for (const [featureIndex, feature] of report.features.entries()) {
    const featureAt = `${at}.features[${featureIndex}]`;
    object(feature, featureAt);
    const featureId = feature.id;
    if (!Number.isSafeInteger(featureId) || featureId < 1) {
      fail(`${featureAt}.id must be a positive integer`);
    }
    if (featureIds.has(featureId)) fail(`${at}.features contains duplicate feature ${featureId}`);
    featureIds.add(featureId);
    if (!Array.isArray(feature.criteria) || feature.criteria.length === 0) {
      fail(`${featureAt}.criteria must not be empty`);
    }
    let featureTotal = 0;
    let featureMax = 0;
    for (const [criterionIndex, criterion] of feature.criteria.entries()) {
      const criterionAt = `${featureAt}.criteria[${criterionIndex}]`;
      object(criterion, criterionAt);
      evidence.push({ stableKey: criterion.stableKey, points: criterion.points });
      if (!Number.isSafeInteger(criterion.points) || criterion.points < 0) {
        fail(`${criterionAt}.points must be a non-negative integer`);
      }
      let passed;
      try {
        passed = evidencePassed(criterionEvidence(criterion));
      } catch (error) {
        fail(`${criterionAt} has invalid evidence: ${error.message}`);
      }
      featureMax += criterion.points;
      if (passed) featureTotal += criterion.points;
    }
    if (feature.max !== featureMax || feature.score !== featureTotal) {
      fail(`${featureAt} reports ${String(feature.score)}/${String(feature.max)}, `
        + `but its criterion evidence is ${featureTotal}/${featureMax}`);
    }
    evidenceTotal += featureTotal;
    evidenceMax += featureMax;
  }
  const evidenceKeys = exactKeys(evidence, `${at}.features criteria`);
  const expectedKeys = expected.map(check => check.stableKey);
  const sorted = keys => [...keys].sort();
  if (!same(sorted(selectionKeys), sorted(expectedKeys))) {
    fail(`${at}.selection.checks does not match the exact assigned checks`);
  }
  if (!same(sorted(evidenceKeys), sorted(expectedKeys))) {
    fail(`${at}.features criteria does not match the exact assigned checks`);
  }
  const points = new Map(expected.map(check => [check.stableKey, check.points]));
  for (const item of [...report.selection.checks, ...evidence]) {
    if (item.points !== points.get(item.stableKey)) {
      fail(`${at} changes points for ${item.stableKey}`);
    }
  }
  const expectedMax = expected.reduce((total, check) => total + check.points, 0);
  if (evidenceMax !== expectedMax) {
    fail(`${at} criterion denominator ${evidenceMax} does not match assigned points ${expectedMax}`);
  }
  if (report.total !== evidenceTotal || report.max !== expectedMax) {
    fail(`${at} reports ${String(report.total)}/${String(report.max)}, `
      + `but its criterion evidence is ${evidenceTotal}/${expectedMax}`);
  }
}

// Merge completed workers into recipe execution order. Worker completion order and
// unit order are not semantic, but identity and check coverage must be exact.
export function mergeExecutionSourceShardResults(inputPlan, workerResults) {
  const plan = validatePlan(inputPlan);
  if (!Array.isArray(workerResults)) fail('workerResults must be an array');
  const expectedShards = new Map(plan.shards.map(shard => [shard.id, shard]));
  const expectedUnits = new Map(plan.units.map(unit => [unit.executionId, unit]));
  const workers = new Map();
  for (const [index, worker] of workerResults.entries()) {
    object(worker, `workerResults[${index}]`);
    const shardId = nonEmpty(worker.shardId, `workerResults[${index}].shardId`);
    if (!expectedShards.has(shardId)) fail(`worker result has unexpected shard ${shardId}`);
    if (workers.has(shardId)) fail(`worker result duplicates shard ${shardId}`);
    if (worker.planSha256 !== plan.contentSha256) fail(`worker ${shardId} has a different plan identity`);
    object(worker.identities, `worker ${shardId}.identities`);
    if (!same(worker.identities, plan.identities)) fail(`worker ${shardId} has tampered identities`);
    if (!Array.isArray(worker.units)) fail(`worker ${shardId}.units must be an array`);
    workers.set(shardId, worker);
  }
  const missingShards = [...expectedShards.keys()].filter(id => !workers.has(id));
  if (missingShards.length) fail(`worker results are missing ${missingShards.join(', ')}`);

  const merged = new Map();
  for (const [shardId, expectedShard] of expectedShards) {
    const worker = workers.get(shardId);
    const assigned = new Set(expectedShard.units);
    const seen = new Set();
    for (const [index, result] of worker.units.entries()) {
      object(result, `worker ${shardId}.units[${index}]`);
      const executionId = nonEmpty(result.executionId,
        `worker ${shardId}.units[${index}].executionId`);
      if (!assigned.has(executionId)) fail(`worker ${shardId} returned unexpected execution ${executionId}`);
      if (seen.has(executionId) || merged.has(executionId)) {
        fail(`worker results duplicate execution ${executionId}`);
      }
      const expected = expectedUnits.get(executionId);
      if (result.source !== expected.source) fail(`worker ${shardId} changed source for ${executionId}`);
      validateReportChecks(result.report, expected.checks,
        `worker ${shardId} execution ${executionId}.report`);
      seen.add(executionId);
      merged.set(executionId, canonicalizeDefinition({
        executionId,
        source: expected.source,
        report: result.report,
      }));
    }
    const missing = expectedShard.units.filter(id => !seen.has(id));
    if (missing.length) fail(`worker ${shardId} is missing execution ${missing.join(', ')}`);
  }

  const units = plan.units.map(unit => merged.get(unit.executionId));
  return canonicalizeDefinition({
    schemaVersion: EXECUTION_SHARD_SCHEMA_VERSION,
    planSha256: plan.contentSha256,
    identities: plan.identities,
    units,
  });
}
