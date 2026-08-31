import assert from 'node:assert/strict';
import test from 'node:test';

import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import type { CheckEvidence } from '../src/evidence/check-evidence.js';
import { mergeExecutionSourceShardResults, planExecutionSourceShards }
  from '../src/grading/execution-shards.js';
import type { ExecutionCheck, ExecutionShardPlan, ExecutionShardUnit,
  ExecutionSource } from '../src/grading/execution-shards.js';

const execution: ExecutionSource[] = [
  { id: 'accounts', source: 'scenarios/accounts.json' },
  { id: 'catalog', source: 'scenarios/catalog.json' },
  { id: 'orders', source: 'scenarios/orders.json' },
  { id: 'support', source: 'scenarios/support.json' },
];
const checks: ExecutionCheck[] = [
  { stableKey: 'pack.accounts.create', executionId: 'accounts', points: 1 },
  { stableKey: 'pack.accounts.sign-in', executionId: 'accounts', points: 1 },
  { stableKey: 'pack.catalog.list', executionId: 'catalog', points: 2 },
  { stableKey: 'pack.orders.buy', executionId: 'orders', points: 3 },
  { stableKey: 'pack.support.open', executionId: 'support', points: 1 },
];
const identities = {
  engine: { id: 'stack-bench', version: '1' },
  recipe: { id: 'ecommerce.example', version: '1.0.0', sha256: 'a'.repeat(64) },
  selection: { sha256: 'b'.repeat(64) },
  source: { sha256: 'c'.repeat(64) },
  stack: { id: 'mongodb', version: '1' },
};

const passedEvidence = () => createCheckEvidence({
  status: 'passed', code: 'completed', phase: 'assertion',
  startedAtMs: 1, completedAtMs: 2,
});
const failedEvidence = () => createCheckEvidence({
  status: 'failed', code: 'test_result', phase: 'assertion',
  startedAtMs: 1, completedAtMs: 2,
});

type ShardReport = {
  selection: { checks: Array<{ stableKey: string; points: number }> };
  features: Array<{ id: number; criteria: Array<{ stableKey: string; points: number;
    evidence: CheckEvidence }>; score: number; max: number }>;
  total: number;
  max: number;
};
type WorkerResult = { shardId: string; planSha256: string; identities: typeof identities;
  units: Array<{ executionId: string; source: string; report: ShardReport }> };

function first<Value>(values: readonly Value[]): Value {
  const value = values[0];
  assert(value);
  return value;
}

function reportFor(unit: ExecutionShardUnit): ShardReport {
  const total = unit.checks.reduce((sum, check) => sum + check.points, 0);
  return {
    selection: { checks: unit.checks.map(check => ({ ...check })) },
    features: [{ id: unit.ordinal, criteria: unit.checks.map(check => ({ ...check,
      evidence: passedEvidence() })), score: total, max: total }],
    total,
    max: total,
  };
}

function workerResults(plan: ExecutionShardPlan): WorkerResult[] {
  const units = new Map(plan.units.map(unit => [unit.executionId, unit]));
  return plan.shards.map(shard => ({
    shardId: shard.id,
    planSha256: plan.contentSha256,
    identities: structuredClone(identities),
    units: shard.units.map(executionId => {
      const unit = units.get(executionId);
      if (!unit) throw new Error(`unknown execution unit ${executionId}`);
      return { executionId, source: unit.source, report: reportFor(unit) };
    }),
  }));
}

test('execution-source planning is deterministic and balances selected check counts', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  assert.equal(plan.shards.length, 2);
  assert.deepEqual(plan.shards.map(shard => shard.checkCount), [3, 2]);
  assert.deepEqual(plan.shards.map(shard => shard.units),
    [['accounts', 'support'], ['catalog', 'orders']]);
  assert.deepEqual(plan,
    planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 }));
});

test('merging accepts reordered workers and units but restores execution order', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const workers = workerResults(plan).reverse();
  for (const worker of workers) worker.units.reverse();
  const merged = mergeExecutionSourceShardResults(plan, workers);
  assert.deepEqual(merged.units.map(unit => unit.executionId),
    ['accounts', 'catalog', 'orders', 'support']);
  assert.equal(merged.planSha256, plan.contentSha256);
});

test('merging rejects duplicate worker and check results', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const workers = workerResults(plan);
  assert.throws(() => mergeExecutionSourceShardResults(plan, [first(workers), first(workers)]),
    /duplicates shard/);

  const duplicatedCheck = structuredClone(workers);
  const duplicateCheckUnit = first(first(duplicatedCheck).units);
  duplicateCheckUnit.report.selection.checks.push(
    structuredClone(first(duplicateCheckUnit.report.selection.checks)));
  assert.throws(() => mergeExecutionSourceShardResults(plan, duplicatedCheck),
    /contains duplicate checks/);
});

test('merging rejects missing and unexpected worker results', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const workers = workerResults(plan);
  assert.throws(() => mergeExecutionSourceShardResults(plan, workers.slice(1)), /are missing/);

  const unexpectedShard = structuredClone(workers);
  first(unexpectedShard).shardId = 'grade-shard-999';
  assert.throws(() => mergeExecutionSourceShardResults(plan, unexpectedShard),
    /unexpected shard grade-shard-999/);

  const unexpected = structuredClone(workers);
  first(first(unexpected).units).executionId = 'not-assigned';
  assert.throws(() => mergeExecutionSourceShardResults(plan, unexpected),
    /unexpected execution not-assigned/);
});

test('merging rejects missing and unexpected check coverage', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const missing = workerResults(plan);
  first(first(first(missing).units).report.features).criteria.pop();
  assert.throws(() => mergeExecutionSourceShardResults(plan, missing), /criterion evidence/);

  const unexpected = workerResults(plan);
  const unexpectedUnit = first(first(unexpected).units);
  unexpectedUnit.report.selection.checks.push({
    stableKey: 'pack.accounts.unexpected', points: 1,
  });
  const unexpectedFeature = first(unexpectedUnit.report.features);
  unexpectedFeature.criteria.push({
    stableKey: 'pack.accounts.unexpected', points: 1, evidence: passedEvidence(),
  });
  unexpectedFeature.score += 1;
  unexpectedFeature.max += 1;
  unexpectedUnit.report.total += 1;
  unexpectedUnit.report.max += 1;
  assert.throws(() => mergeExecutionSourceShardResults(plan, unexpected),
    /exact assigned checks/);
});

test('merging rejects identity-tampered workers and plans', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const workers = workerResults(plan);
  first(workers).identities.stack.id = 'postgres';
  assert.throws(() => mergeExecutionSourceShardResults(plan, workers), /tampered identities/);

  const changedPlan = { ...plan, identities: { ...plan.identities,
    source: { sha256: 'd'.repeat(64) } } };
  assert.throws(() => mergeExecutionSourceShardResults(changedPlan, workerResults(plan)),
    /contentSha256 does not match/);
});

test('merging derives totals from criterion evidence and rejects aggregate tampering', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const changedTotal = workerResults(plan);
  first(first(changedTotal).units).report.total -= 1;
  assert.throws(() => mergeExecutionSourceShardResults(plan, changedTotal),
    /criterion evidence/);

  const changedMax = workerResults(plan);
  first(first(changedMax).units).report.max += 1;
  assert.throws(() => mergeExecutionSourceShardResults(plan, changedMax),
    /criterion evidence/);

  const changedFeature = workerResults(plan);
  first(first(first(changedFeature).units).report.features).score -= 1;
  assert.throws(() => mergeExecutionSourceShardResults(plan, changedFeature),
    /criterion evidence/);
});

test('merging accepts an earned total below max when criterion evidence fails', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const workers = workerResults(plan);
  const report = first(first(workers).units).report;
  const failed = report.features.at(0)?.criteria[1];
  assert(failed);
  failed.evidence = failedEvidence();
  first(report.features).score -= failed.points;
  report.total -= failed.points;
  const merged = mergeExecutionSourceShardResults(plan, workers);
  const mergedUnit = first(merged.units);
  assert.equal(mergedUnit.report.total, 1);
  assert.equal(mergedUnit.report.max, 2);
});

test('merging rejects duplicate features and empty feature shells', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const empty = workerResults(plan);
  first(first(empty).units).report.features.push({ id: 99, score: 0, max: 0, criteria: [] });
  assert.throws(() => mergeExecutionSourceShardResults(plan, empty), /criteria must not be empty/);

  const duplicate = workerResults(plan);
  const duplicateUnit = first(first(duplicate).units);
  duplicateUnit.report.features.push({
    id: first(duplicateUnit.report.features).id,
    score: 0,
    max: 0,
    criteria: [],
  });
  assert.throws(() => mergeExecutionSourceShardResults(plan, duplicate), /duplicate feature/);
});

test('planning permits separate execution identities to use the same source', () => {
  const shared = execution.map(entry => ({ ...entry, source: 'scenarios/shared.json' }));
  const plan = planExecutionSourceShards({ execution: shared, checks, identities }, { maxWorkers: 2 });
  assert.equal(plan.units.length, execution.length);
  assert.equal(plan.units.every(unit => unit.source === 'scenarios/shared.json'), true);
});

test('planning rejects duplicate and unmapped checks', () => {
  assert.throws(() => planExecutionSourceShards({ execution,
    checks: [...checks, structuredClone(first(checks))], identities }), /duplicate stable keys/);
  assert.throws(() => planExecutionSourceShards({ execution,
    checks: [{ stableKey: 'pack.unknown.check', executionId: 'unknown', points: 1 }], identities }),
  /unknown execution/);
});
