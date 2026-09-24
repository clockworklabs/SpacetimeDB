import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { pendingRunSnapshot, auditFailureSummary, gradeArgv, parseAgentProcessResult }
  from '../commands/bench.js';
import { finalizeRunTotals }
  from '../src/evidence/benchmark-run.js';
import { formatLevelSummary } from '../src/evidence/evidence-presentation.js';
import type { BenchmarkRunRecord, RunLevelRecord, RunSessionRecord, RunTotalsInput } from '../src/evidence/benchmark-run.js';
import { parseBenchArguments } from '../commands/bench-arguments.js';
import { pristineMutationBaselinePath } from '../src/evidence/mutation-control.js';

test('new direct runs default to production framing with an explicit opt-out', () => {
  const argv = ['node', 'bench', '--backend', 'postgres'];
  assert.equal(parseBenchArguments(argv).productionQuality, true);
  assert.equal(parseBenchArguments([...argv, '--production-quality']).productionQuality, true);
  assert.equal(parseBenchArguments([...argv, '--no-production-quality']).productionQuality, false);
  assert.throws(() => parseBenchArguments([...argv, '--production-quality', '--no-production-quality']), /only one/);
});
import { clearPrivateGradingEvidence, privateGradingDirectory, levelGradeIsUsable, repairEvidenceDecision,
  repairHistoryEntry, repairProgressState, repairRegressionDecision,
  restorePrivateGradingEvidence }
  from '../src/evidence/repair-evidence.js';
import { finalPackageEvidenceRequired, preserveFinalPackageEvidence, sourceBoundFirstBuildOutcome }
  from '../src/runtime/source-checkpoint.js';
import { materializationAppFailure, materializeAcceptedSource, restoreRepairSource }
  from '../src/runtime/source-materialization.js';
import { dependencyLevelRepairRecords, dependencyRepairBudget, dependencyRepairRecords }
  from '../src/progression/dependency-mode.js';
import { loadTrack, RUN_INDEX_CAP } from '../src/composition/tracks.js';
import { writeArtifact } from '../src/evidence/artifacts.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import type { GradeBundlePayload } from '../src/evidence/benchmark-run.js';
import { compiledEntrypoint } from '../src/package-root.js';
import { agentSessionFailure } from '../src/agents/agent-result-contract.js';

test('billable agent runs require the Docker appliance', () => {
  const env = { ...process.env };
  delete env.STACK_BENCH_APPLIANCE;
  const result = spawnSync(process.execPath,
    [compiledEntrypoint('commands', 'bench.js'), '--backend', 'stub'],
    { encoding: 'utf8', env });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /agent adapter claude-code requires the Docker appliance/);
});

test('a nonzero agent process preserves its valid provider-failure result', () => {
  const request = { app: '/app', mode: 'build' as const, level: 1, backend: 'spacetime',
    track: 'ecommerce', runIndex: 0, model: 'provider-model', guidance: 'neutral',
    adapterCostLimit: 'non-billable' as const };
  const raw = {
    appDir: '/app', mode: 'build', level: 1, ok: false, sessionId: 'session-1',
    costUsd: 0, tokens: 10, outputTokens: 2, turns: 1, promptBytes: 20, durationMs: 100,
    setup: {}, usage: { input: 8, output: 2, cacheWrite: 0, cacheRead: 0 }, costReceipts: [],
    providerMetadata: { failureCode: 'provider-usage-receipt-missing' },
  };
  const result = parseAgentProcessResult(`${JSON.stringify(raw)}\n`, '', new Error('exit code 3'),
    request);
  assert.equal(result.ok, false);
  assert.equal(agentSessionFailure(result)?.kind, 'provider_failure');
  assert.throws(() => parseAgentProcessResult(
    `${JSON.stringify({ ...raw, ok: true })}\n`, '', new Error('exit code 3'), request),
  /failed after reporting success/);
});

test('accepted source is materialized through the application lifecycle before grading', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-materialized-source-'));
  try {
    const source = join(root, 'source');
    const app = join(root, 'app');
    mkdirSync(join(app, 'node_modules'), { recursive: true });
    mkdirSync(source, { recursive: true });
    writeFileSync(join(source, 'index.js'), 'export const value = "accepted";\n');
    writeFileSync(join(source, 'start.sh'), '#!/bin/sh\n');
    writeFileSync(join(app, 'index.js'), 'export const value = "coding runtime";\n');
    writeFileSync(join(app, '.server.pid'), '100\n');
    writeFileSync(join(app, 'node_modules', 'state'), 'stale\n');
    const modes: string[] = [];
    const application = { backend: 'postgres', app, port: 6573, probe: '' };
    await materializeAcceptedSource(source, app, application,
      async (_spec, mode = 'restart') => {
        modes.push(mode);
        if (mode === 'stop' && modes.length === 1) {
          assert.match(readFileSync(join(app, 'index.js'), 'utf8'), /coding runtime/);
        }
        if (mode === 'start') {
          assert.match(readFileSync(join(app, 'index.js'), 'utf8'), /accepted/);
          assert.equal(existsSync(join(app, 'node_modules')), false);
          writeFileSync(join(app, '.server.pid'), '200\n');
        }
      });
    assert.deepEqual(modes, ['stop', 'start']);
    assert.equal(readFileSync(join(app, '.server.pid'), 'utf8'), '200\n');
    assert.equal(hashAppSource(app).sha256, hashAppSource(source).sha256);
    await assert.rejects(materializeAcceptedSource(source, app, application, async (_spec, mode) => {
      if (mode === 'start') writeFileSync(join(app, 'index.js'), 'changed during start\n');
    }), /differs from its accepted snapshot/);
    assert.match(readFileSync(join(app, 'index.js'), 'utf8'), /accepted/);
    rmSync(join(source, 'start.sh'));
    const missingContractModes: string[] = [];
    let missingContract: unknown = null;
    try {
      await materializeAcceptedSource(source, app, application, async (_spec, mode) => {
        missingContractModes.push(mode ?? 'restart');
      });
    } catch (error) {
      missingContract = error;
    }
    assert.deepEqual(missingContractModes, ['stop']);
    assert.equal(materializationAppFailure(missingContract)?.kind, 'app_failure');

    writeFileSync(join(source, 'start.sh'), '#!/bin/sh\n');
    let cleanupStops = 0;
    await assert.rejects(materializeAcceptedSource(source, app, application, async (_spec, mode) => {
      if (mode === 'start') writeFileSync(join(app, 'index.js'), 'changed during start\n');
      if (mode === 'stop' && ++cleanupStops === 2) throw new Error('stop failed');
    }), /could not stop and restore/);
    assert.match(readFileSync(join(app, 'index.js'), 'utf8'), /accepted/);

    const failedStartModes: string[] = [];
    let failedStart: unknown = null;
    try {
      await materializeAcceptedSource(source, app, application, async (_spec, mode) => {
        failedStartModes.push(mode ?? 'restart');
        if (mode === 'start') {
          mkdirSync(join(app, 'dist'));
          throw Object.assign(new Error(
            'npm install failed for DATABASE_URL=mongodb://user:secret@database:27017/app'),
          { code: 'generated_app_not_restartable' });
        }
      });
    } catch (error) {
      failedStart = error;
    }
    assert.deepEqual(failedStartModes, ['stop', 'start', 'stop']);
    assert.equal(existsSync(join(app, 'node_modules')), false);
    assert.equal(existsSync(join(app, 'dist')), false);
    const failedStartOutcome = materializationAppFailure(failedStart);
    assert.equal(failedStartOutcome?.kind, 'app_failure');
    assert.match(failedStartOutcome?.reason ?? '', /npm install failed/);
    assert.doesNotMatch(failedStartOutcome?.reason ?? '', /secret/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('repair rollback restores schema before startup and stops on reset failure for every stack', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-schema-'));
  try {
    const source = join(root, 'accepted');
    mkdirSync(source);
    writeFileSync(join(source, 'schema.txt'), 'accepted');
    writeFileSync(join(source, 'start.sh'), '#!/bin/sh\n');
    for (const backend of ['spacetime', 'postgres', 'mongodb']) {
      const app = join(root, backend);
      mkdirSync(app);
      writeFileSync(join(app, 'schema.txt'), 'rejected');
      const application = { backend, app, port: 6573, probe: '' };
      let databaseSchema = 'rejected';
      let stopped = false;
      const events: string[] = [];
      const lifecycle: Parameters<typeof restoreRepairSource>[3] = async (_spec, mode) => {
        events.push(mode ?? 'restart');
        if (mode === 'stop') stopped = true;
        if (mode === 'start') {
          assert.equal(stopped, true);
          assert.equal(readFileSync(join(app, 'schema.txt'), 'utf8'), 'accepted');
          if (databaseSchema !== 'accepted') throw new Error('schema migration required');
          stopped = false;
        }
      };
      await assert.rejects(materializeAcceptedSource(source, app, application, lifecycle),
        /schema migration required/);
      events.length = 0;
      await restoreRepairSource(source, app, application, lifecycle, request => {
        assert.deepEqual(request, { backend, app });
        assert.equal(stopped, true);
        assert.equal(readFileSync(join(app, 'schema.txt'), 'utf8'), 'accepted');
        events.push('reset');
        databaseSchema = 'accepted';
      });
      assert.deepEqual(events, ['stop', 'reset', 'start']);
      assert.equal(hashAppSource(app).sha256, hashAppSource(source).sha256);
      events.length = 0;
      await assert.rejects(restoreRepairSource(source, app, application, lifecycle, () => {
        events.push('reset');
        throw new Error('database reset failed');
      }), /database reset failed/);
      assert.deepEqual(events, ['stop', 'reset', 'stop']);
      assert.equal(stopped, true);
      assert.equal(hashAppSource(app).sha256, hashAppSource(source).sha256);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});
test('final package preservation verifies both source and grading before success', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-final-package-'));
  try {
    const app = join(root, 'app');
    const output = join(root, 'output');
    mkdirSync(app, { recursive: true });
    mkdirSync(privateGradingDirectory(app), { recursive: true });
    mkdirSync(output, { recursive: true });
    writeFileSync(join(app, 'index.js'), 'export const ready = true;\n');
    const source = hashAppSource(app);
    writeArtifact(join(privateGradingDirectory(app), 'bundle.json'), {
      kind: 'grade_bundle', id: 'final-grade', payload: {
        observation: 'scored', source: { sha256: source.sha256 },
        suites: {}, totals: { score: 1, max: 1 },
        selection: { sha256: 'a'.repeat(64) },
      },
    });

    const evidence = preserveFinalPackageEvidence({ appDir: app, outputDir: output });
    assert.equal(evidence.source.sha256, source.sha256);
    assert.equal(existsSync(join(output, 'grading', 'bundle.json')), true);

    rmSync(join(privateGradingDirectory(app), 'bundle.json'));
    assert.throws(() => preserveFinalPackageEvidence({ appDir: app, outputDir: output }),
      /mandatory result package evidence.*final grader produced no bundle/);
    assert.equal(existsSync(join(output, 'source', 'index.js')), true,
      'source evidence remains available when grading preservation fails');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('hostile app-local grade output cannot replace accepted private evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-private-grade-'));
  try {
    const app = join(root, 'app');
    const output = join(root, 'result');
    mkdirSync(app);
    writeFileSync(join(app, 'index.js'), 'export {};');
    const source = hashAppSource(app);
    const grading = privateGradingDirectory(app);
    mkdirSync(grading);
    writeArtifact(join(grading, 'bundle.json'), {
      kind: 'grade_bundle', id: 'trusted-grade', payload: {
        source: { sha256: source.sha256 }, totals: { score: 0, max: 1 },
      },
    });
    const attacker = spawnSync(process.execPath, ['-e', `
      const fs = require('node:fs');
      fs.mkdirSync('stack-bench', { recursive: true });
      fs.writeFileSync('stack-bench/bundle.json', JSON.stringify({ totals: { score: 1, max: 1 } }));
    `], { cwd: app, encoding: 'utf8' });
    assert.equal(attacker.status, 0, attacker.stderr);
    preserveFinalPackageEvidence({ appDir: app, outputDir: output });
    assert.deepEqual(readFileSync(join(output, 'grading', 'bundle.json')),
      readFileSync(join(grading, 'bundle.json')));
    const argv = gradeArgv({ backend: 'postgres', track: 'ecommerce', runIndex: 0,
      media: false }, app, 'http://localhost:3000', 'isolation', 1,
    loadTrack('ecommerce'), 'test');
    assert.equal(argv[argv.indexOf('--out') + 1], grading);
    assert.throws(() => privateGradingDirectory(app, join(app, 'stack-bench')), /outside/);
    const alias = join(root, 'alias');
    symlinkSync(app, alias, 'junction');
    assert.throws(() => privateGradingDirectory(app, join(alias, 'new-grades')), /outside/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('an interrupted run keeps its root failure even after an earlier grade', () => {
  const levels = [{ graded: true }, { graded: false }];
  assert.equal(finalPackageEvidenceRequired({ kind: 'provider_failure' }, levels), false);
  assert.equal(finalPackageEvidenceRequired({ kind: 'harness_failure' }, levels), false);
  assert.equal(finalPackageEvidenceRequired({ kind: 'app_failure' }, levels), true);
});

test('a missing first-build source is a harness failure', () => {
  const passed = { outcome: { kind: 'passed' }, totals: { score: 1, max: 1 }, suites: {} };
  assert.equal(sourceBoundFirstBuildOutcome(passed, { sha256: 'a'.repeat(64) }).kind, 'passed');
  assert.deepEqual(sourceBoundFirstBuildOutcome(passed, null), {
    kind: 'harness_failure',
    phase: 'first-build-source',
    reason: 'the first-build source could not be preserved and verified',
    appFailures: [],
    inconclusive: [],
    harnessFailures: ['the first-build source could not be preserved and verified'],
  });
});

test('direct runs default to ten repairs while an explicit budget still wins', () => {
  assert.equal(parseBenchArguments(['node', 'bench', '--backend', 'postgres']).repairs, 10);
  assert.equal(parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--repairs', '4']).repairs, 4);
});

test('bench arguments reject partial and out-of-range run indexes', () => {
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--run-index', '1junk']), /--run-index must be an integer/);
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--run-index', String(RUN_INDEX_CAP + 1)]), /--run-index must be an integer from 0 through/);
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--pricing-json', '{}']), /--pricing-json/);
  const sharded = parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--mutation-shard-index', '1', '--mutation-shard-count', '3']);
  assert.equal(sharded.mutationShardIndex, 1);
  assert.equal(sharded.mutationShardCount, 3);
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--mutation-shard-index', '1']), /must be supplied together/);
});

test('progression level usability follows its stricter evidence result', () => {
  assert.equal(levelGradeIsUsable({ kind: 'app_failure' }), true);
  assert.equal(levelGradeIsUsable({ kind: 'app_failure', inconclusive: ['feature/check'] }), false);
  assert.equal(levelGradeIsUsable(
    { kind: 'app_failure', inconclusive: ['unrelated/check'] }, { outcome: 'conclusive' }), true);
  assert.equal(levelGradeIsUsable({ kind: 'inconclusive' }), false);
  assert.equal(levelGradeIsUsable({ kind: 'provider_failure' }), false);
  assert.equal(levelGradeIsUsable({ kind: 'app_failure' }, { outcome: 'inconclusive' }), false);
  assert.equal(levelGradeIsUsable({ kind: 'app_failure' }, { outcome: 'conclusive' }), true);
});

test('dependency repair accounting uses repairs, not grading observations', () => {
  const action = { type: 'repair', repair: { remaining: 3 } };
  assert.equal(dependencyRepairBudget(action, 0), 3);
  assert.equal(dependencyRepairBudget(action, 1), 4);
  assert.throws(() => dependencyRepairBudget({ type: 'build', repair: { remaining: null } }, 0),
    /valid repair action/);

  const state = {
    definition: { nodes: [
      { id: 'accounts', level: 1 },
      { id: 'catalog', level: 1 },
      { id: 'recovery', level: 2 },
    ] },
    nodes: {
      accounts: { repairs: { used: 1 },
        exhaustedAtLevel: null, exhaustionReason: null },
      catalog: { repairs: { used: 5 },
        exhaustedAtLevel: 1, exhaustionReason: 'feature-repairs-exhausted' },
      recovery: { repairs: { used: 1 },
        exhaustedAtLevel: null, exhaustionReason: null },
    },
    attempts: [
      { level: 1, repair: { depth: 1, nodeIds: ['accounts', 'recovery'] } },
      { level: 2, repair: { depth: 2, nodeIds: ['recovery'] } },
      { level: 2, repair: null },
    ],
  };
  assert.deepEqual(dependencyRepairRecords(state, 1, ['recovery']), [
    { nodeId: 'accounts', used: 1, exhaustionReason: null },
    { nodeId: 'catalog', used: 5, exhaustionReason: 'feature-repairs-exhausted' },
    { nodeId: 'recovery', used: 1, exhaustionReason: null },
  ]);
  // The level record covers the level's own nodes plus the nodes a repair at
  // that depth touched, and nothing else.
  assert.deepEqual(dependencyLevelRepairRecords(state, 1), [
    { nodeId: 'accounts', used: 1, exhaustionReason: null },
    { nodeId: 'catalog', used: 5, exhaustionReason: 'feature-repairs-exhausted' },
    { nodeId: 'recovery', used: 1, exhaustionReason: null },
  ]);
  assert.deepEqual(dependencyLevelRepairRecords(state, 2), [
    { nodeId: 'recovery', used: 1, exhaustionReason: null },
  ]);
});

test('resumed dependency costs separate prior, current, and cumulative execution usage', () => {
  const run: RunTotalsInput = { levels: [
    { level: 1, graded: true, score: 1, max: 1, buildCostUsd: 4,
      sessionTotals: { sessions: 1, tokens: 10, outputTokens: 2, turns: 1, durationMs: 100 } },
    { level: 2, graded: true, score: 1, max: 1, repairCostUsd: 2,
      sessionTotals: { sessions: 1, tokens: 5, outputTokens: 1, turns: 1, durationMs: 50 } },
  ], progressionResume: { inheritedLevels: [1],
    priorTotals: { costUsd: 4, costComplete: true } } };
  const totals = finalizeRunTotals(run, 1_000, { now: 3_000 });
  assert.equal(totals.priorExecutionCostUsd, 4);
  assert.equal(totals.currentExecutionCostUsd, 2);
  assert.equal(totals.cumulativeCostUsd, 6);
  assert.equal(totals.costUsd, 6);
  assert.equal(totals.costComplete, true);
});

test('audit failures retain the exit code and stderr needed for diagnosis', () => {
  const error = Object.assign(new Error('Command failed: leak audit'), {
    status: 7,
    stderr: "node:fs:441\nError: EACCES: permission denied, open '/run/transcript.jsonl'",
  });
  assert.equal(auditFailureSummary(error),
    "Command failed: leak audit (exit 7; stderr: Error: EACCES: permission denied, open '/run/transcript.jsonl')");
});

test('repair preparation removes raw grading evidence but keeps the app and bug report', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-isolation-'));
  try {
    mkdirSync(privateGradingDirectory(root), { recursive: true });
    writeFileSync(join(privateGradingDirectory(root), 'bundle.json'), '{"private":true}\n');
    writeFileSync(join(root, 'BUG_REPORT.md'), '# Behaviour only\n');
    writeFileSync(join(root, 'app.js'), 'export {};\n');
    clearPrivateGradingEvidence(root);
    assert.equal(existsSync(privateGradingDirectory(root)), false);
    assert.equal(existsSync(join(root, 'BUG_REPORT.md')), true);
    assert.equal(existsSync(join(root, 'app.js')), true);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('repair progress pauses only on repeated findings without a score gain', () => {
  const bundle = (score: number, failures: string[]): GradeBundlePayload => ({
    totals: { score, max: 10, contractPass: true },
    suites: {}, outcome: { kind: 'app_failure', phase: 'grading',
      appFailures: failures, inconclusive: [], harnessFailures: [] } });
  let state = repairProgressState(null, bundle(5, ['a']));
  state = repairProgressState(state, bundle(5, ['a']));
  assert.equal(state.stalledRounds, 1);
  state = repairProgressState(state, bundle(5, ['b']));
  assert.equal(state.stalledRounds, 0);
  state = repairProgressState(state, bundle(6, ['b']));
  assert.equal(state.stalledRounds, 0);
  state = repairProgressState(state, bundle(6, ['b']));
  assert.equal(state.stalledRounds, 1);
});

test('repair history reports score movement and exact remaining findings', () => {
  const before = { totals: { score: 4, max: 6 }, suites: {}, outcome: {
    kind: 'app_failure', phase: 'grading', appFailures: ['suite/a'] } };
  const after = { totals: { score: 5, max: 6 }, suites: { lint: { results: [
    { id: 'review-average', status: 'FAIL' },
  ] } }, outcome: { kind: 'app_failure', phase: 'grading',
    appFailures: ['contract-lint', 'suite/b'] } };
  assert.deepEqual(repairHistoryEntry(2, before, after, 'kept'), {
    round: 2, beforeScore: 4, beforeMax: 6, afterScore: 5, afterMax: 6,
    result: 'kept', remainingFailures: ['suite/b', 'testing-interface/review-average'],
  });
});

test('mutation-only execution is restricted to model-free reference runs', () => {
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--reference-mutation-only']), /requires a mutation-bound reference fixture/);
  const args = parseBenchArguments(['node', 'bench', '--backend', 'postgres', '--repairs', '0',
    '--agent-adapter', 'reference-fixture', '--app', 'fixture', '--mutations', 'mutations.json',
    '--reference-mutation-only', '--mutation-baseline-bundle', 'baseline.json']);
  assert.equal(args.referenceMutationOnly, true);
  assert(args.mutationBaselineBundle);
  assert.match(args.mutationBaselineBundle, /baseline\.json$/);
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--mutation-baseline-bundle', 'baseline.json']), /require --mutations/);
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--mutations', 'mutations.json', '--mutation-baseline-bundle', 'baseline.json']),
  /internal reference mutation option/);
});

test('mutation control reuses the existing clean grade when it is present', () => {
  const args = { out: 'results/run', levelList: [1, 3] };
  assert.equal(pristineMutationBaselinePath(args, () => true),
    join('results/run', 'first-build-l3-grading', 'bundle.json'));
  assert.equal(pristineMutationBaselinePath(args, () => false), null);
  assert.equal(pristineMutationBaselinePath({ ...args, referenceMutationOnly: true }), null);
});

test('the first repair that makes an unstartable app gradeable is never rolled back', () => {
  const after = { suites: {}, totals: { score: 35, max: 58 } };
  for (const phase of ['application-restart', 'application-readiness']) {
    const before = { outcome: { kind: 'app_failure', phase },
      suites: {}, totals: { score: 0, max: 58 } };
    assert.equal(repairEvidenceDecision(before, after).action, 'keep-setup-repair');
  }
  assert.equal(repairEvidenceDecision({ suites: {} }, after).action, 'rollback-no-comparison');
});

test('repair comparison respects declared scope without hiding missing or regressed evidence', () => {
  const criterion = (id: string, status: 'passed' | 'failed', points = 1) => ({
    id, stableKey: `check.${id}`, points,
    evidence: createCheckEvidence({ status, code: 'test_result', phase: 'assertion',
      startedAtMs: 1, completedAtMs: 2 }),
  });
  const bundle = (criteria: ReturnType<typeof criterion>[]) => ({
    suites: { features: { features: [{ id: 'work', criteria }] } },
  });
  const before = bundle([criterion('retained', 'passed'), criterion('repair', 'failed'),
    criterion('outside', 'passed')]);
  const after = { ...bundle([criterion('retained', 'passed'), criterion('repair', 'passed')]),
    selection: { checks: [{ stableKey: 'check.retained' }, { stableKey: 'check.repair' }] } };
  for (const decide of [repairEvidenceDecision, repairRegressionDecision]) {
    assert.equal(decide(before, after).action, 'keep');
    assert.equal(decide(before, { ...after,
      ...bundle([criterion('repair', 'passed')]) }).action, 'rollback-regression');
    assert.equal(decide(before, { ...after,
      ...bundle([criterion('retained', 'failed'), criterion('repair', 'passed')])
    }).action, 'rollback-regression');
    assert.equal(decide(before, { ...after, selection: undefined }).action, 'rollback-regression');
  }
  const traded = repairEvidenceDecision(bundle([criterion('a', 'passed', 2), criterion('b', 'failed', 3)]),
    bundle([criterion('a', 'failed', 2), criterion('b', 'passed', 3)]));
  assert.equal(traded.action, 'rollback-regression');
  assert.deepEqual(traded.shared.regressions, ['check.a']);
  const earlier = bundle([criterion('pass', 'passed'), criterion('fail', 'failed')]);
  assert.equal(repairRegressionDecision(earlier, bundle([criterion('pass', 'passed')])).action, 'keep');
  assert.equal(repairRegressionDecision(earlier, bundle([criterion('pass', 'failed')])).action,
    'rollback-regression');
});

test('repair rollback restores the accepted grading evidence without another grade', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-grade-rollback-'));
  try {
    const app = join(root, 'app');
    const snapshot = join(root, 'snapshot');
    mkdirSync(app, { recursive: true });
    mkdirSync(privateGradingDirectory(app), { recursive: true });
    mkdirSync(snapshot, { recursive: true });
    writeFileSync(join(privateGradingDirectory(app), 'bundle.json'), 'new grade\n');
    writeFileSync(join(snapshot, 'bundle.json'), 'accepted grade\n');

    restorePrivateGradingEvidence(app, snapshot);

    assert.equal(readFileSync(join(privateGradingDirectory(app), 'bundle.json'), 'utf8'),
      'accepted grade\n');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reference qualification does not recover inconclusive suites, while paid grading can', () => {
  const track = loadTrack('ecommerce');
  for (const backend of ['spacetime', 'postgres', 'mongodb', 'convex']) {
    for (const agentAdapter of ['reference-fixture', 'codex']) {
      const args = { backend, agentAdapter, track: 'ecommerce', runIndex: 0, media: false };
      const argv = gradeArgv(args, '/app', 'http://localhost:6573', 'qualification', 1,
        track, 'attempt', { sourceSha256: 'a'.repeat(64) });
      assert.equal(argv.includes('--retry-inconclusive'), agentAdapter === 'codex',
        `${backend}/${agentAdapter}: qualification must preserve the first inconclusive result`);
    }
  }
});

test('grading binds scored evidence to the selected application source', () => {
  const track = loadTrack('ecommerce');
  const sourceSha256 = 'a'.repeat(64);
  const argv = gradeArgv({ backend: 'spacetime', track: 'ecommerce', runIndex: 0,
    media: false }, '/app', 'http://localhost:6481', 'spacetime-l1', 1, track, 'attempt',
  { sourceSha256 });
  const index = argv.indexOf('--source-sha256');
  assert(index > 0);
  assert.equal(argv[index + 1], sourceSha256);
});

test('grading records a materialization defect through the complete grading path', () => {
  const track = loadTrack('ecommerce');
  const applicationFailure = { kind: 'app_failure', phase: 'application-restart',
    reason: 'application startup changed the accepted source',
    appFailures: ['application-restart'], inconclusive: [], harnessFailures: [] } as const;
  const argv = gradeArgv({ backend: 'postgres', track: 'ecommerce', runIndex: 0,
    media: false }, '/app', 'http://localhost:6573', 'postgres-l1', 1, track, 'attempt',
  { applicationFailure });
  const index = argv.indexOf('--application-failure-json');
  assert(index > 0);
  assert.deepEqual(JSON.parse(argv[index + 1] ?? ''), applicationFailure);
});

test('later-level grading receives prior selected checks as regression scope', () => {
  const track = loadTrack('ecommerce');
  const args = { backend: 'postgres', track: 'ecommerce', runIndex: 0, media: false,
    recipeTasks: new Map([
      [1, { request: { schemaVersion: 3 }, selection: { scoredChecks: [
        { stableKey: 'prior/a' }, { stableKey: 'prior/b' },
      ] } }],
      [2, { request: { schemaVersion: 3 }, selection: { scoredChecks: [
        { stableKey: 'current/c' },
      ] } }],
    ]) };
  const argv = gradeArgv(args, '/app', 'http://localhost:6573', 'postgres-l2', 2,
    track, 'attempt');
  const index = argv.indexOf('--regression-checks-json');
  assert(index > 0);
  const regressionChecks = argv[index + 1];
  assert(regressionChecks);
  assert.deepEqual(JSON.parse(regressionChecks), ['prior/a', 'prior/b']);
  const dependency = { ...args, progression: { identity: { policy: 'dependency-graph' } } };
  assert.equal(gradeArgv(dependency, '/app', 'http://localhost:6573', 'postgres-l2', 2,
    track, 'attempt').includes('--regression-checks-json'), false);
});

test('level summary names early stopping without claiming budget exhaustion', () => {
  const summary = formatLevelSummary({ level: 2, graded: true, score: 65, max: 70,
    repair: { status: 'incomplete', stopReason: 'repeated-findings', used: 2, limit: 5 } });
  assert(summary.includes('stopped: repeated findings'));
  assert(!summary.includes('budget exhausted'));
  const ungraded = formatLevelSummary({ level: 1, graded: false,
    error: 'coding-session-failed', buildCostUsd: 1.25, durationMs: 4_400 });
  assert(ungraded.includes('NOT GRADED'));
  assert(ungraded.includes('stopped: coding session failed'));
  assert(!ungraded.includes('budget exhausted'));
});


test('pending paid sessions survive a stop without changing accepted level ownership', () => {
  const paid = (costUsd: number) => ({ costUsd, costComplete: true, costReceipts: [], sessionId: null, durationMs: 0, usage: null, providerThrottle: null, resources: null, tokens: null, outputTokens: null, turns: null, promptBytes: null, thinking: null, transcript: null, provenance: null, providerMetadata: null }) as RunSessionRecord;
  const baseline = { level: 2, graded: true, score: 70, max: 70, selection: null,
    outcome: { kind: 'passed' }, buildCostUsd: 2, buildSessions: [paid(2)] } as RunLevelRecord;
  const run = { id: 'pending', startedAt: new Date().toISOString(), levels: [baseline] } as BenchmarkRunRecord;
  const pending = { level: 3, graded: false, score: null, max: null, selection: null,
    outcome: { kind: 'ungraded' }, buildCostUsd: 3, buildSessions: [paid(3)],
    repairCostUsd: 5, repairSessions: [paid(5)], repairs: 1 } as RunLevelRecord;
  const snapshot = pendingRunSnapshot(run, pending, false);
  assert.equal(snapshot.totals!.costUsd, 10);
  assert.equal(snapshot.totals!.costComplete, false);
  assert.equal(snapshot.levels[1]!.repairSessions![0]!.costUsd, 5);
  assert.equal(run.levels.length, 1);
  assert.equal(run.totals, undefined);
  const sameDepth = pendingRunSnapshot(run, { ...pending, level: 2 }, true);
  assert.equal(sameDepth.levels.length, 1);
  assert.equal(sameDepth.levels[0]!.buildSessions!.length, 2);
  assert.equal(sameDepth.totals!.costUsd, 10);
});
