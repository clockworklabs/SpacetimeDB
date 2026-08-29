import assert from 'node:assert/strict';
import test from 'node:test';

import { createCheckEvidence } from '../dist/src/evidence/check-evidence.js';
import { mergeExecutionSourceShardResults, planExecutionSourceShards }
  from '../dist/src/grading/execution-shards.js';

const execution = [
  { id: 'accounts', source: 'scenarios/accounts.json' },
  { id: 'catalog', source: 'scenarios/catalog.json' },
  { id: 'orders', source: 'scenarios/orders.json' },
  { id: 'support', source: 'scenarios/support.json' },
];
const checks = [
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

function reportFor(unit) {
  const total = unit.checks.reduce((sum, check) => sum + check.points, 0);
  return {
    selection: { checks: unit.checks.map(check => ({ ...check })) },
    features: [{ id: unit.ordinal, criteria: unit.checks.map(check => ({ ...check,
      evidence: passedEvidence() })), score: total, max: total }],
    total,
    max: total,
  };
}

function workerResults(plan) {
  const units = new Map(plan.units.map(unit => [unit.executionId, unit]));
  return plan.shards.map(shard => ({
    shardId: shard.id,
    planSha256: plan.contentSha256,
    identities: structuredClone(plan.identities),
    units: shard.units.map(executionId => {
      const unit = units.get(executionId);
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
  assert.throws(() => mergeExecutionSourceShardResults(plan, [workers[0], workers[0]]),
    /duplicates shard/);

  const duplicatedCheck = structuredClone(workers);
  duplicatedCheck[0].units[0].report.selection.checks.push(
    structuredClone(duplicatedCheck[0].units[0].report.selection.checks[0]));
  assert.throws(() => mergeExecutionSourceShardResults(plan, duplicatedCheck),
    /contains duplicate checks/);
});

test('merging rejects missing and unexpected worker results', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const workers = workerResults(plan);
  assert.throws(() => mergeExecutionSourceShardResults(plan, workers.slice(1)), /are missing/);

  const unexpectedShard = structuredClone(workers);
  unexpectedShard[0].shardId = 'grade-shard-999';
  assert.throws(() => mergeExecutionSourceShardResults(plan, unexpectedShard),
    /unexpected shard grade-shard-999/);

  const unexpected = structuredClone(workers);
  unexpected[0].units[0].executionId = 'not-assigned';
  assert.throws(() => mergeExecutionSourceShardResults(plan, unexpected),
    /unexpected execution not-assigned/);
});

test('merging rejects missing and unexpected check coverage', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const missing = workerResults(plan);
  missing[0].units[0].report.features[0].criteria.pop();
  assert.throws(() => mergeExecutionSourceShardResults(plan, missing), /criterion evidence/);

  const unexpected = workerResults(plan);
  unexpected[0].units[0].report.selection.checks.push({
    stableKey: 'pack.accounts.unexpected', points: 1,
  });
  unexpected[0].units[0].report.features[0].criteria.push({
    stableKey: 'pack.accounts.unexpected', points: 1, evidence: passedEvidence(),
  });
  unexpected[0].units[0].report.features[0].score += 1;
  unexpected[0].units[0].report.features[0].max += 1;
  unexpected[0].units[0].report.total += 1;
  unexpected[0].units[0].report.max += 1;
  assert.throws(() => mergeExecutionSourceShardResults(plan, unexpected),
    /exact assigned checks/);
});

test('merging rejects identity-tampered workers and plans', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const workers = workerResults(plan);
  workers[0].identities.stack.id = 'postgres';
  assert.throws(() => mergeExecutionSourceShardResults(plan, workers), /tampered identities/);

  const changedPlan = structuredClone(plan);
  changedPlan.identities.source.sha256 = 'd'.repeat(64);
  assert.throws(() => mergeExecutionSourceShardResults(changedPlan, workerResults(plan)),
    /contentSha256 does not match/);
});

test('merging derives totals from criterion evidence and rejects aggregate tampering', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const changedTotal = workerResults(plan);
  changedTotal[0].units[0].report.total -= 1;
  assert.throws(() => mergeExecutionSourceShardResults(plan, changedTotal),
    /criterion evidence/);

  const changedMax = workerResults(plan);
  changedMax[0].units[0].report.max += 1;
  assert.throws(() => mergeExecutionSourceShardResults(plan, changedMax),
    /criterion evidence/);

  const changedFeature = workerResults(plan);
  changedFeature[0].units[0].report.features[0].score -= 1;
  assert.throws(() => mergeExecutionSourceShardResults(plan, changedFeature),
    /criterion evidence/);
});

test('merging accepts an earned total below max when criterion evidence fails', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const workers = workerResults(plan);
  const report = workers[0].units[0].report;
  const failed = report.features[0].criteria[1];
  failed.evidence = failedEvidence();
  report.features[0].score -= failed.points;
  report.total -= failed.points;
  const merged = mergeExecutionSourceShardResults(plan, workers);
  assert.equal(merged.units[0].report.total, 1);
  assert.equal(merged.units[0].report.max, 2);
});

test('merging rejects duplicate features and empty feature shells', () => {
  const plan = planExecutionSourceShards({ execution, checks, identities }, { maxWorkers: 2 });
  const empty = workerResults(plan);
  empty[0].units[0].report.features.push({ id: 99, score: 0, max: 0, criteria: [] });
  assert.throws(() => mergeExecutionSourceShardResults(plan, empty), /criteria must not be empty/);

  const duplicate = workerResults(plan);
  duplicate[0].units[0].report.features.push({
    id: duplicate[0].units[0].report.features[0].id,
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
    checks: [...checks, structuredClone(checks[0])], identities }), /duplicate stable keys/);
  assert.throws(() => planExecutionSourceShards({ execution,
    checks: [{ stableKey: 'pack.unknown.check', executionId: 'unknown', points: 1 }], identities }),
  /unknown execution/);
});
