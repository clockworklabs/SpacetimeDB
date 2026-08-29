import {
  canonicalDefinitionJson,
  canonicalizeDefinition,
} from '../composition/definition-plan.js';
import type { CanonicalDefinition } from '../composition/definition-plan.js';
import { criterionEvidence, evidencePassed } from '../evidence/check-evidence.js';
import { sha256 } from '../evidence/provenance.js';

export const EXECUTION_SHARD_SCHEMA_VERSION = 1;

type UnknownRecord = Record<string, unknown>;
type IdentityMap = Record<string, CanonicalDefinition>;

export interface ExecutionSource {
  id: string;
  source: string;
}

export interface ExecutionCheck {
  stableKey: string;
  executionId: string;
  points: number;
}

export interface PlannedCheck {
  stableKey: string;
  points: number;
}

export interface ExecutionShardUnit {
  ordinal: number;
  executionId: string;
  source: string;
  checks: PlannedCheck[];
}

export interface ExecutionShard {
  id: string;
  units: string[];
  checkCount: number;
}

export interface ExecutionShardPlan {
  schemaVersion: 1;
  identities: IdentityMap;
  units: ExecutionShardUnit[];
  shards: ExecutionShard[];
  contentSha256: string;
}

export interface ExecutionShardPlanningInput {
  execution: ExecutionSource[];
  checks: ExecutionCheck[];
  identities: IdentityMap;
}

export interface ExecutionShardPlanningOptions {
  maxWorkers?: number;
}

export interface ExecutionShardWorkerUnit {
  executionId: string;
  source: string;
  report: UnknownRecord;
}

export interface ExecutionShardWorkerResult {
  shardId: string;
  planSha256: string;
  identities: IdentityMap;
  units: ExecutionShardWorkerUnit[];
}

export interface MergedExecutionShardResult {
  schemaVersion: 1;
  planSha256: string;
  identities: IdentityMap;
  units: ExecutionShardWorkerUnit[];
}

function fail(message: string): never {
  throw new Error(`invalid grade shard input: ${message}`);
}

function object(value: unknown, at: string): UnknownRecord {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    fail(`${at} must be an object`);
  }
  return value as UnknownRecord;
}

function nonEmpty(value: unknown, at: string): string {
  if (typeof value !== 'string' || !value.trim()) fail(`${at} must be a non-empty string`);
  return value;
}

function nonNegativeInteger(value: unknown, at: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    fail(`${at} must be a non-negative integer`);
  }
  return value as number;
}

function checkDescriptor(checkInput: unknown, at: string): ExecutionCheck {
  const check = object(checkInput, at);
  return {
    stableKey: nonEmpty(check.stableKey, `${at}.stableKey`),
    executionId: nonEmpty(check.executionId, `${at}.executionId`),
    points: nonNegativeInteger(check.points, `${at}.points`),
  };
}

function exactKeys(items: unknown, at: string): string[] {
  if (!Array.isArray(items)) fail(`${at} must be an array`);
  const keys = items.map((item, index) => {
    const entry = object(item, `${at}[${index}]`);
    return nonEmpty(entry.stableKey, `${at}[${index}].stableKey`);
  });
  if (new Set(keys).size !== keys.length) fail(`${at} contains duplicate checks`);
  return keys;
}

function same(left: unknown, right: unknown): boolean {
  return canonicalDefinitionJson(left) === canonicalDefinitionJson(right);
}

function validatePlan(input: unknown): ExecutionShardPlan {
  const plan = object(input, 'plan');
  if (plan.schemaVersion !== EXECUTION_SHARD_SCHEMA_VERSION) {
    fail('plan schemaVersion is unsupported');
  }
  const identities = object(plan.identities, 'plan.identities') as IdentityMap;
  if (Object.keys(identities).length === 0) fail('plan.identities must not be empty');
  if (!Array.isArray(plan.units) || plan.units.length === 0) fail('plan.units must not be empty');
  if (!Array.isArray(plan.shards) || plan.shards.length === 0) fail('plan.shards must not be empty');
  const contentSha256 = nonEmpty(plan.contentSha256, 'plan.contentSha256');
  if (!/^[a-f0-9]{64}$/.test(contentSha256)) {
    fail('plan.contentSha256 must be a SHA-256 digest');
  }
  const expectedHash = sha256(canonicalDefinitionJson({
    schemaVersion: plan.schemaVersion,
    identities: plan.identities,
    units: plan.units,
    shards: plan.shards,
  }));
  if (contentSha256 !== expectedHash) fail('plan contentSha256 does not match its content');

  const unitById = new Map<string, ExecutionShardUnit>();
  const plannedChecks = new Set<string>();
  for (const [index, unitInput] of plan.units.entries()) {
    const unit = object(unitInput, `plan.units[${index}]`);
    if (unit.ordinal !== index + 1) fail(`plan.units[${index}].ordinal is not contiguous`);
    const executionId = nonEmpty(unit.executionId, `plan.units[${index}].executionId`);
    const source = nonEmpty(unit.source, `plan.units[${index}].source`);
    if (unitById.has(executionId)) fail(`plan.units contains duplicate execution ${executionId}`);
    if (!Array.isArray(unit.checks) || unit.checks.length === 0) {
      fail(`plan.units[${index}].checks must not be empty`);
    }
    const checks = unit.checks.map((checkInput, checkIndex) => {
      const at = `plan.units[${index}].checks[${checkIndex}]`;
      const check = object(checkInput, at);
      const stableKey = nonEmpty(check.stableKey, `${at}.stableKey`);
      const points = nonNegativeInteger(check.points, `${at}.points`);
      if (plannedChecks.has(stableKey)) fail(`plan.units contains duplicate check ${stableKey}`);
      plannedChecks.add(stableKey);
      return { stableKey, points };
    });
    const validated = { ordinal: index + 1, executionId, source, checks };
    unitById.set(executionId, validated);
  }

  const shardIds = new Set<string>();
  const assignedUnits = new Set<string>();
  for (const [index, shardInput] of plan.shards.entries()) {
    const shard = object(shardInput, `plan.shards[${index}]`);
    const id = nonEmpty(shard.id, `plan.shards[${index}].id`);
    if (shardIds.has(id)) fail(`plan.shards contains duplicate shard ${id}`);
    if (!Array.isArray(shard.units) || shard.units.length === 0) {
      fail(`plan.shards[${index}].units must not be empty`);
    }
    let checkCount = 0;
    for (const unitInput of shard.units) {
      const executionId = nonEmpty(unitInput, `plan.shards[${index}].units`);
      const unit = unitById.get(executionId);
      if (!unit) fail(`plan shard ${id} has unknown execution ${executionId}`);
      if (assignedUnits.has(executionId)) {
        fail(`plan assigns execution ${executionId} more than once`);
      }
      assignedUnits.add(executionId);
      checkCount += unit.checks.length;
    }
    if (shard.checkCount !== checkCount) fail(`plan shard ${id} has an incorrect checkCount`);
    shardIds.add(id);
  }
  const missingUnits = [...unitById.keys()].filter(id => !assignedUnits.has(id));
  if (missingUnits.length) fail(`plan does not assign execution ${missingUnits.join(', ')}`);

  return plan as unknown as ExecutionShardPlan;
}

// Build deterministic work units at the recipe execution/source boundary.
// This function does not start workers or assign runtime resources.
export function planExecutionSourceShards(
  { execution, checks, identities }: ExecutionShardPlanningInput,
  { maxWorkers = 1 }: ExecutionShardPlanningOptions = {},
): ExecutionShardPlan {
  if (!Array.isArray(execution) || execution.length === 0) fail('execution must not be empty');
  if (!Array.isArray(checks) || checks.length === 0) fail('checks must not be empty');
  if (!Number.isSafeInteger(maxWorkers) || maxWorkers < 1) {
    fail('maxWorkers must be a positive integer');
  }
  const frozenIdentities = canonicalizeDefinition(object(identities, 'identities')) as IdentityMap;

  const executionById = new Map<string, { id: string; source: string; order: number }>();
  for (const [index, entryInput] of execution.entries()) {
    const entry = object(entryInput, `execution[${index}]`);
    const id = nonEmpty(entry.id, `execution[${index}].id`);
    const source = nonEmpty(entry.source, `execution[${index}].source`);
    if (executionById.has(id)) fail(`execution contains duplicate id ${id}`);
    executionById.set(id, { id, source, order: index });
  }

  const selected = checks.map((check, index) => checkDescriptor(check, `checks[${index}]`));
  const stableKeys = selected.map(check => check.stableKey);
  if (new Set(stableKeys).size !== stableKeys.length) fail('checks contains duplicate stable keys');
  const checksByExecution = new Map<string, PlannedCheck[]>();
  for (const check of selected) {
    if (!executionById.has(check.executionId)) {
      fail(`check ${check.stableKey} maps to unknown execution ${check.executionId}`);
    }
    const grouped = checksByExecution.get(check.executionId) ?? [];
    grouped.push({ stableKey: check.stableKey, points: check.points });
    checksByExecution.set(check.executionId, grouped);
  }

  const units: ExecutionShardUnit[] = [...executionById.values()]
    .filter(entry => checksByExecution.has(entry.id))
    .map((entry, index) => ({
      ordinal: index + 1,
      executionId: entry.id,
      source: entry.source,
      checks: checksByExecution.get(entry.id) ?? [],
    }));
  const workerCount = Math.min(maxWorkers, units.length);
  const shards: ExecutionShard[] = Array.from({ length: workerCount }, (_, index) => ({
    id: `grade-shard-${String(index + 1).padStart(3, '0')}`,
    units: [],
    checkCount: 0,
  }));
  for (const unit of units) {
    const first = shards[0];
    if (!first) fail('checks do not map to an execution unit');
    const shard = shards.reduce((least, candidate) =>
      candidate.checkCount < least.checkCount ? candidate : least, first);
    shard.units.push(unit.executionId);
    shard.checkCount += unit.checks.length;
  }

  const body = canonicalizeDefinition({
    schemaVersion: EXECUTION_SHARD_SCHEMA_VERSION,
    identities: frozenIdentities,
    units,
    shards,
  }) as unknown as Omit<ExecutionShardPlan, 'contentSha256'>;
  return { ...body, contentSha256: sha256(canonicalDefinitionJson(body)) };
}

function validateReportChecks(reportInput: unknown, expected: PlannedCheck[], at: string): void {
  const report = object(reportInput, at);
  const selection = object(report.selection, `${at}.selection`);
  const selectionChecks = selection.checks;
  const selectionKeys = exactKeys(selectionChecks, `${at}.selection.checks`);
  const evidence: Array<{ stableKey: unknown; points: unknown }> = [];
  const featureIds = new Set<number>();
  let evidenceTotal = 0;
  let evidenceMax = 0;
  if (!Array.isArray(report.features)) fail(`${at}.features must be an array`);
  for (const [featureIndex, featureInput] of report.features.entries()) {
    const featureAt = `${at}.features[${featureIndex}]`;
    const feature = object(featureInput, featureAt);
    const featureId = feature.id;
    if (!Number.isSafeInteger(featureId) || (featureId as number) < 1) {
      fail(`${featureAt}.id must be a positive integer`);
    }
    const id = featureId as number;
    if (featureIds.has(id)) fail(`${at}.features contains duplicate feature ${id}`);
    featureIds.add(id);
    if (!Array.isArray(feature.criteria) || feature.criteria.length === 0) {
      fail(`${featureAt}.criteria must not be empty`);
    }
    let featureTotal = 0;
    let featureMax = 0;
    for (const [criterionIndex, criterionInput] of feature.criteria.entries()) {
      const criterionAt = `${featureAt}.criteria[${criterionIndex}]`;
      const criterion = object(criterionInput, criterionAt);
      evidence.push({ stableKey: criterion.stableKey, points: criterion.points });
      const points = nonNegativeInteger(criterion.points, `${criterionAt}.points`);
      let passed: boolean;
      try {
        passed = evidencePassed(criterionEvidence(criterion));
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        fail(`${criterionAt} has invalid evidence: ${message}`);
      }
      featureMax += points;
      if (passed) featureTotal += points;
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
  const sorted = (keys: string[]): string[] => [...keys].sort();
  if (!same(sorted(selectionKeys), sorted(expectedKeys))) {
    fail(`${at}.selection.checks does not match the exact assigned checks`);
  }
  if (!same(sorted(evidenceKeys), sorted(expectedKeys))) {
    fail(`${at}.features criteria does not match the exact assigned checks`);
  }
  const points = new Map(expected.map(check => [check.stableKey, check.points]));
  if (!Array.isArray(selectionChecks)) fail(`${at}.selection.checks must be an array`);
  for (const itemInput of [...selectionChecks, ...evidence]) {
    const item = object(itemInput, `${at}.check`);
    if (item.points !== points.get(nonEmpty(item.stableKey, `${at}.check.stableKey`))) {
      fail(`${at} changes points for ${String(item.stableKey)}`);
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
export function mergeExecutionSourceShardResults(
  inputPlan: unknown,
  workerResults: unknown,
): MergedExecutionShardResult {
  const plan = validatePlan(inputPlan);
  if (!Array.isArray(workerResults)) fail('workerResults must be an array');
  const expectedShards = new Map(plan.shards.map(shard => [shard.id, shard]));
  const expectedUnits = new Map(plan.units.map(unit => [unit.executionId, unit]));
  const workers = new Map<string, UnknownRecord>();
  for (const [index, workerInput] of workerResults.entries()) {
    const worker = object(workerInput, `workerResults[${index}]`);
    const shardId = nonEmpty(worker.shardId, `workerResults[${index}].shardId`);
    if (!expectedShards.has(shardId)) fail(`worker result has unexpected shard ${shardId}`);
    if (workers.has(shardId)) fail(`worker result duplicates shard ${shardId}`);
    if (worker.planSha256 !== plan.contentSha256) {
      fail(`worker ${shardId} has a different plan identity`);
    }
    const identities = object(worker.identities, `worker ${shardId}.identities`);
    if (!same(identities, plan.identities)) fail(`worker ${shardId} has tampered identities`);
    if (!Array.isArray(worker.units)) fail(`worker ${shardId}.units must be an array`);
    workers.set(shardId, worker);
  }
  const missingShards = [...expectedShards.keys()].filter(id => !workers.has(id));
  if (missingShards.length) fail(`worker results are missing ${missingShards.join(', ')}`);

  const merged = new Map<string, ExecutionShardWorkerUnit>();
  for (const [shardId, expectedShard] of expectedShards) {
    const worker = workers.get(shardId);
    if (!worker || !Array.isArray(worker.units)) fail(`worker results are missing ${shardId}`);
    const assigned = new Set(expectedShard.units);
    const seen = new Set<string>();
    for (const [index, resultInput] of worker.units.entries()) {
      const result = object(resultInput, `worker ${shardId}.units[${index}]`);
      const executionId = nonEmpty(result.executionId,
        `worker ${shardId}.units[${index}].executionId`);
      if (!assigned.has(executionId)) {
        fail(`worker ${shardId} returned unexpected execution ${executionId}`);
      }
      if (seen.has(executionId) || merged.has(executionId)) {
        fail(`worker results duplicate execution ${executionId}`);
      }
      const expected = expectedUnits.get(executionId);
      if (!expected) fail(`worker ${shardId} returned unknown execution ${executionId}`);
      if (result.source !== expected.source) {
        fail(`worker ${shardId} changed source for ${executionId}`);
      }
      validateReportChecks(result.report, expected.checks,
        `worker ${shardId} execution ${executionId}.report`);
      const report = object(result.report, `worker ${shardId} execution ${executionId}.report`);
      seen.add(executionId);
      merged.set(executionId, canonicalizeDefinition({
        executionId,
        source: expected.source,
        report,
      }) as unknown as ExecutionShardWorkerUnit);
    }
    const missing = expectedShard.units.filter(id => !seen.has(id));
    if (missing.length) fail(`worker ${shardId} is missing execution ${missing.join(', ')}`);
  }

  const units = plan.units.map(unit => {
    const result = merged.get(unit.executionId);
    if (!result) fail(`worker results are missing execution ${unit.executionId}`);
    return result;
  });
  return canonicalizeDefinition({
    schemaVersion: EXECUTION_SHARD_SCHEMA_VERSION,
    planSha256: plan.contentSha256,
    identities: plan.identities,
    units,
  }) as unknown as MergedExecutionShardResult;
}
