import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { auditFailureSummary, gradeArgv, parseAgentProcessResult }
  from '../commands/bench.js';
import { finalizeRunTotals }
  from '../src/evidence/benchmark-run.js';
import { formatLevelSummary } from '../src/evidence/evidence-presentation.js';
import type { RunTotalsInput } from '../src/evidence/benchmark-run.js';
import { parseBenchArguments } from '../commands/bench-arguments.js';
import { pristineMutationBaselinePath } from '../src/evidence/mutation-control.js';
import { clearPrivateGradingEvidence, levelGradeIsUsable, repairEvidenceDecision,
  repairHistoryEntry, repairProgressState, restorePrivateGradingEvidence }
  from '../src/evidence/repair-evidence.js';
import { finalPackageEvidenceRequired, preserveFinalPackageEvidence, sourceBoundFirstBuildOutcome }
  from '../src/runtime/source-checkpoint.js';
import { materializationAppFailure, materializeAcceptedSource }
  from '../src/runtime/source-materialization.js';
import { dependencyRepairBudget, dependencyStrikeRecords }
  from '../src/progression/dependency-mode.js';
import { loadTrack } from '../src/composition/tracks.js';
import { writeArtifact } from '../src/evidence/artifacts.js';
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
          assert.equal(readFileSync(join(app, 'node_modules', 'state'), 'utf8'), 'stale\n');
        }
      });
    assert.deepEqual(modes, ['stop', 'start']);
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
    assert.equal(existsSync(join(app, 'node_modules')), true);
    assert.equal(existsSync(join(app, 'dist')), false);
    const failedStartOutcome = materializationAppFailure(failedStart);
    assert.equal(failedStartOutcome?.kind, 'app_failure');
    assert.match(failedStartOutcome?.reason ?? '', /npm install failed/);
    assert.doesNotMatch(failedStartOutcome?.reason ?? '', /secret/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('final package preservation verifies both source and grading before success', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-final-package-'));
  try {
    const app = join(root, 'app');
    const output = join(root, 'output');
    mkdirSync(join(app, 'stack-bench'), { recursive: true });
    mkdirSync(output, { recursive: true });
    writeFileSync(join(app, 'index.js'), 'export const ready = true;\n');
    const source = hashAppSource(app);
    writeArtifact(join(app, 'stack-bench', 'bundle.json'), {
      kind: 'grade_bundle', id: 'final-grade', payload: {
        observation: 'scored', source: { sha256: source.sha256 },
        suites: {}, totals: { score: 1, max: 1 },
        selection: { sha256: 'a'.repeat(64) },
      },
    });

    const evidence = preserveFinalPackageEvidence({ appDir: app, outputDir: output });
    assert.equal(evidence.source.sha256, source.sha256);
    assert.equal(existsSync(join(output, 'grading', 'bundle.json')), true);

    rmSync(join(app, 'stack-bench', 'bundle.json'));
    assert.throws(() => preserveFinalPackageEvidence({ appDir: app, outputDir: output }),
      /mandatory result package evidence.*final grader produced no bundle/);
    assert.equal(existsSync(join(output, 'source', 'index.js')), true,
      'source evidence remains available when grading preservation fails');
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

test('direct runs default to ten repair rounds while an explicit budget still wins', () => {
  assert.equal(parseBenchArguments(['node', 'bench', '--backend', 'postgres']).fixRounds, 10);
  assert.equal(parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--fix-rounds', '4']).fixRounds, 4);
});

test('bench arguments reject partial and out-of-range run indexes', () => {
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--run-index', '1junk']), /--run-index must be an integer/);
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--run-index', '21']), /--run-index must be an integer from 0 through 20/);
});

test('bench arguments validate pricing at the CLI boundary', () => {
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--pricing-json', '{}']), /--pricing-json/);
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

test('dependency repair accounting uses repair rounds, not grading observations', () => {
  const action = { strikes: { scope: 'feature', maxRemaining: 3 } };
  assert.equal(dependencyRepairBudget(action, 0, true), 2);
  assert.equal(dependencyRepairBudget(action, 0), 3);
  assert.equal(dependencyRepairBudget(action, 1), 4);
  assert.throws(() => dependencyRepairBudget({ strikes: { scope: 'level', maxRemaining: 3 } }, 0),
    /valid strike action/);

  const state = {
    definition: { nodes: [
      { id: 'accounts', level: 1 },
      { id: 'catalog', level: 1 },
      { id: 'recovery', level: 2 },
    ] },
    nodes: {
      accounts: { strikes: { initialBudget: 3, granted: 0, budget: 3, used: 1 },
        exhaustedAtLevel: null, exhaustionReason: null },
      catalog: { strikes: { initialBudget: 3, granted: 2, budget: 5, used: 5 },
        exhaustedAtLevel: 1, exhaustionReason: 'strikes-exhausted' },
      recovery: { strikes: { initialBudget: 2, granted: 0, budget: 2, used: 1 },
        exhaustedAtLevel: null, exhaustionReason: null },
    },
  };
  assert.deepEqual(dependencyStrikeRecords(state, 1, ['recovery']), [
    { nodeId: 'accounts', initialBudget: 3, granted: 0, budget: 3, used: 1,
      remaining: 2, exhaustionReason: null },
    { nodeId: 'catalog', initialBudget: 3, granted: 2, budget: 5, used: 5,
      remaining: 0, exhaustionReason: 'strikes-exhausted' },
    { nodeId: 'recovery', initialBudget: 2, granted: 0, budget: 2, used: 1,
      remaining: 1, exhaustionReason: null },
  ]);
});

test('resumed dependency costs separate prior, current, and cumulative execution usage', () => {
  const run: RunTotalsInput = { levels: [
    { level: 1, graded: true, score: 1, max: 1, buildCostUsd: 4,
      sessionTotals: { sessions: 1, tokens: 10, outputTokens: 2, turns: 1, durationMs: 100 } },
    { level: 2, graded: true, score: 1, max: 1, fixCostUsd: 2,
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

test('ungraded level summaries contain useful failure values', () => {
  assert.equal(formatLevelSummary({ level: 1, graded: false,
    error: 'coding-session-failed', buildCostUsd: 1.25, durationMs: 4_400 }),
  'L1: NOT GRADED | 0 repairs | $1.25 total ($0.00 repairs) | '
    + 'stopped: coding session failed | 4s');
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
    mkdirSync(join(root, 'stack-bench'), { recursive: true });
    writeFileSync(join(root, 'stack-bench', 'bundle.json'), '{"private":true}\n');
    writeFileSync(join(root, 'BUG_REPORT.md'), '# Behaviour only\n');
    writeFileSync(join(root, 'app.js'), 'export {};\n');
    clearPrivateGradingEvidence(root);
    assert.equal(existsSync(join(root, 'stack-bench')), false);
    assert.equal(existsSync(join(root, 'BUG_REPORT.md')), true);
    assert.equal(existsSync(join(root, 'app.js')), true);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('dependency campaign progression rejects an incomplete or unbound plan reference', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-plan-'));
  try {
    const path = join(root, 'plan.json');
    writeArtifact(path, { kind: 'campaign_plan', id: 'plan', payload: {} });
    assert.throws(() => parseBenchArguments(['node', 'bench',
      '--campaign-file', path, '--campaign-attempt-id', 'attempt']),
    /compiled campaign/);
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

test('mutation shard coordinates are paired', () => {
  const args = parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--mutation-shard-index', '1', '--mutation-shard-count', '3']);
  assert.equal(args.mutationShardIndex, 1);
  assert.equal(args.mutationShardCount, 3);
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--mutation-shard-index', '1']), /must be supplied together/);
});

test('mutation-only execution is restricted to model-free reference runs', () => {
  assert.throws(() => parseBenchArguments(['node', 'bench', '--backend', 'postgres',
    '--reference-mutation-only']), /requires a mutation-bound reference fixture/);
  const args = parseBenchArguments(['node', 'bench', '--backend', 'postgres', '--fix-rounds', '0',
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

test('repair rollback restores the accepted grading evidence without another grade', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-grade-rollback-'));
  try {
    const app = join(root, 'app');
    const snapshot = join(root, 'snapshot');
    mkdirSync(join(app, 'stack-bench'), { recursive: true });
    mkdirSync(snapshot, { recursive: true });
    writeFileSync(join(app, 'stack-bench', 'bundle.json'), 'new grade\n');
    writeFileSync(join(snapshot, 'bundle.json'), 'accepted grade\n');

    restorePrivateGradingEvidence(app, snapshot);

    assert.equal(readFileSync(join(app, 'stack-bench', 'bundle.json'), 'utf8'),
      'accepted grade\n');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('grading restarts the complete application on its public port', () => {
  const track = loadTrack('ecommerce');
  const argv = gradeArgv({ backend: 'postgres', track: 'ecommerce', runIndex: 0,
    media: false }, '/app', 'http://localhost:6573', 'postgres-l1', 1, track, 'attempt');
  const index = argv.indexOf('--restart-spec');
  assert(index > 0);
  assert.deepEqual(JSON.parse(argv[index + 1] ?? ''), {
    backend: 'postgres', app: '/app', port: 6573, probe: '',
  });
  assert.equal(argv.includes('--reseed-probe'), false);
  assert.equal(argv.includes('--reseed-probe-expectation-json'), false);
});

test('Spacetime grading probes the application instead of a missing API port', () => {
  const track = loadTrack('ecommerce');
  const argv = gradeArgv({ backend: 'spacetime', track: 'ecommerce', runIndex: 0,
    media: false }, '/app', 'http://localhost:6473', 'spacetime-l1', 1, track, 'attempt');
  assert.equal(argv.includes('--reseed-probe'), false);
  assert.equal(argv.includes('--reseed-probe-expectation-json'), false);
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
});

test('dependency grading uses its exact action scope without a second regression selection', () => {
  const track = loadTrack('ecommerce');
  const args = { backend: 'postgres', track: 'ecommerce', runIndex: 0, media: false,
    progression: { identity: { policy: 'dependency-graph' } },
    recipeTasks: new Map([
      [1, { request: { schemaVersion: 3 }, selection: { scoredChecks: [
        { stableKey: 'prior/a' },
      ] } }],
      [2, { request: { schemaVersion: 3 }, selection: { scoredChecks: [
        { stableKey: 'prior/a' }, { stableKey: 'current/b' },
      ] } }],
    ]) };
  const argv = gradeArgv(args, '/app', 'http://localhost:6573', 'postgres-l2', 2,
    track, 'attempt');
  assert.equal(argv.includes('--regression-checks-json'), false);
});
