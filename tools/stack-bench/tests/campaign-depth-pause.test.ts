import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { acquireCampaignLock, releaseCampaignLock, writeCampaignRecord } from '../src/campaigns/campaign-lock.js';
import { campaignDepthPauseStatus, continueCampaignDepth, depthPauseDurationMs,
  readDepthPause, validateDepthPauseEvidence, waitAtDepthBoundary } from '../src/campaigns/campaign-depth-pause.js';
import { claimNextAttempt, initializeCampaignDirectory, writeCampaignState,
  campaignTimeBudget, type CampaignClaim } from '../src/campaigns/campaign-scheduler.js';
import { ARTIFACT_FILE } from '../src/evidence/artifacts.js';
import { finalizeRunTotals, type RunTotalsInput } from '../src/evidence/benchmark-run.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { dependencyRuntimeDefinition } from '../src/progression/progression-definition.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import { runBounded } from '../src/runtime/bounded-process.js';

test('planned hold preserves the L3 action, repair history, cohort and cost accounting', async () => {
  const root = mkdtempSync(join(tmpdir(), 'depth-pause-'));
  let lock: ReturnType<typeof acquireCampaignLock> | undefined;
  const abort = new AbortController();
  const waits: Promise<number>[] = [];
  try {
    const manifest = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
      'appliance/campaign.ecommerce-progression-reference.json'), 'utf8'));
    manifest.levels = [1, 2, 3];
    manifest.selection.levels = manifest.selection.levels.slice(0, 3);
    manifest.repair = { selection: 'feature', budget: { perFeature: 5 } };
    manifest.parallelism = 3;
    const path = join(root, 'manifest.json');
    writeFileSync(path, JSON.stringify(manifest));
    const uninterrupted = compileCampaignFile(path);
    manifest.mode.pauseAfterDepth = 2;
    writeFileSync(path, JSON.stringify(manifest));
    const plan = compileCampaignFile(path);
    assert.deepEqual(plan.bindings, uninterrupted.bindings);
    assert.deepEqual(plan.featureCatalog, uninterrupted.featureCatalog);
    assert.deepEqual(plan.dependencyPolicy, uninterrupted.dependencyPolicy);
    assert.deepEqual(plan.conditions, uninterrupted.conditions);
    assert.deepEqual(plan.attempts.map(a => a.skills), uninterrupted.attempts.map(a => a.skills));
    assert.notEqual(plan.contentSha256, uninterrupted.contentSha256, 'pause is disclosed in protocol identity');
    manifest.parallelism = 2;
    writeFileSync(path, JSON.stringify(manifest));
    assert.throws(() => compileCampaignFile(path), /whole cohort/);
    manifest.parallelism = 3;
    manifest.mode.pauseAfterDepth = 3;
    writeFileSync(path, JSON.stringify(manifest));
    assert.throws(() => compileCampaignFile(path), /later depth/);

    let progression = progressionEngine.initialize(dependencyRuntimeDefinition(
      plan.featureCatalog!, plan.dependencyPolicy!));
    let sequence = 0;
    while (progression.phase === 'active' && progression.level <= 2) {
      const selection = progressionEngine.gradingSelection(progression);
      progression = progressionEngine.recordResult(progression, {
        attemptId: `action-${++sequence}`, outcome: 'conclusive',
        ...(progressionEngine.nextAction(progression).type === 'repair' ? { completedRepair: true } : {}),
        nodes: selection.nodeIds.map(id => ({ id, checks: selection.checks
          .filter(c => c.nodeId === id).map(c => ({ id: c.id,
            outcome: sequence === 1 ? 'fail' : 'pass' })) })),
      });
      assert(sequence < 30);
    }
    assert.equal(progression.level, 3);
    assert(progression.attempts.some(a => a.repair), 'prefix includes repairs');
    const next = structuredClone(progressionEngine.nextAction(progression));
    const before = structuredClone(progression);
    const results = join(root, 'results');
    const initialized = initializeCampaignDirectory(plan, results);
    lock = acquireCampaignLock(results, plan);
    let state = initialized.state;
    const claims: CampaignClaim[] = [];
    for (let i = 0; i < 3; i++) {
      const claimed = claimNextAttempt(state, { runIndex: i, admissionId: `admission-${i}` });
      assert(claimed.claim);
      state = claimed.state;
      claims.push(claimed.claim);
    }
    writeCampaignState(initialized.paths.state, plan, state);
    assert.throws(() => continueCampaignDepth(results), /every attempt/);
    for (const claim of claims) {
      const output = join(results, claim.output);
      const app = join(output, 'app');
      mkdirSync(app, { recursive: true });
      writeFileSync(join(app, 'index.js'), 'export const accepted = true;');
      writeFileSync(join(output, ARTIFACT_FILE.progressionState), JSON.stringify(progression));
      waits.push(waitAtDepthBoundary(output, app, { directory: results,
        campaignSha256: plan.contentSha256, ownershipMarkerSha256: lock.record.ownershipMarkerSha256,
        attemptId: claim.attempt.id, executionId: claim.executionId, depth: 2 }, { signal: abort.signal }));
    }
    assert.equal(campaignDepthPauseStatus(results).attempts.filter(a => a.paused).length, 3);
    await delay(25);
    const release = continueCampaignDepth(results);
    assert.deepEqual(continueCampaignDepth(results), release, 'release is idempotent');
    const durations = await Promise.all(waits);
    assert(durations.every(ms => ms > 0));
    const proof = { campaignSha256: plan.contentSha256, attemptId: claims[0]!.attempt.id,
      depth: 2, durationMs: durations[0]! };
    validateDepthPauseEvidence(join(results, claims[0]!.output), proof);
    assert.throws(() => validateDepthPauseEvidence(join(results, claims[0]!.output),
      { ...proof, durationMs: proof.durationMs + 1 }), /pause accounting/);
    assert.deepEqual(progression, before);
    assert.deepEqual(progressionEngine.nextAction(progression), next);
    const run: RunTotalsInput = { levels: [{ level: 1, buildCostUsd: 2, repairCostUsd: 3,
      repairs: 2 }, { level: 2, buildCostUsd: 4, repairCostUsd: 1, repairs: 1 }] };
    const baseline = finalizeRunTotals(run, 0, { now: 10_000 });
    run.pausedDurationMs = 5_000;
    const held = finalizeRunTotals(run, 0, { now: 15_000 });
    assert.equal(held.costUsd, baseline.costUsd);
    assert.equal(held.repairs, baseline.repairs);
    assert.equal(held.activeDurationSec, baseline.durationSec);
    assert.equal(held.durationSec, 15);
    const attempt = state.attempts[0]!;
    attempt.executions[0]!.pausedMs = 5_000;
    const started = Date.parse(attempt.executions[0]!.startedAt);
    assert.equal(campaignTimeBudget(plan, attempt, started + 15_000).consumedMs, 10_000);
  } finally {
    abort.abort();
    await Promise.allSettled(waits);
    if (lock) releaseCampaignLock(lock);
    rmSync(root, { recursive: true, force: true });
  }
});

test('hold rejects source changes, stale authority and cancellation without advancing', async () => {
  for (const failure of ['source', 'authority', 'cancel']) {
    const root = mkdtempSync(join(tmpdir(), 'depth-pause-invalid-'));
    const abort = new AbortController();
    try {
      const app = join(root, 'app');
      mkdirSync(app);
      writeFileSync(join(app, 'index.js'), 'accepted');
      writeFileSync(join(root, ARTIFACT_FILE.progressionState), '{}');
      const context = { directory: root, campaignSha256: 'a'.repeat(64),
        ownershipMarkerSha256: 'b'.repeat(64), attemptId: 'attempt', executionId: 'execution', depth: 2 };
      const pending = waitAtDepthBoundary(root, app, context, { signal: abort.signal });
      const rejected = assert.rejects(pending, failure === 'source' ? /source or progression/
        : failure === 'authority' ? /ownershipMarkerSha256/ : /abort/i);
      if (failure === 'cancel') abort.abort();
      else {
        if (failure === 'source') writeFileSync(join(app, 'index.js'), 'edited');
        writeCampaignRecord(join(root, 'depth-release.json'), { campaignSha256: context.campaignSha256,
          ownershipMarkerSha256: (failure === 'authority' ? 'c' : 'b').repeat(64), depth: 2,
          releasedAt: Date.now() });
      }
      await rejected;
      assert.equal(readDepthPause(root, context)?.resumedAt, null);
      assert(depthPauseDurationMs(root, context) >= 0);
    } finally { abort.abort(); rmSync(root, { recursive: true, force: true }); }
  }
});

test('a planned process hold preserves remaining working time and still permits cancellation', async () => {
  const start = Date.now();
  const paused = await runBounded(process.execPath, ['-e', 'setInterval(()=>{},1000)'], {
    stdio: 'ignore', timeoutMs: 250, terminate: pid => process.kill(pid, 'SIGKILL'),
    pauseInterval: () => ({ startedAt: start + 100, resumedAt: Date.now() >= start + 550 ? start + 550 : null }),
  });
  assert(paused.timedOut);
  assert.equal(paused.error, null);
  assert(Date.now() - start >= 690);
  const late = await runBounded(process.execPath, ['-e', 'setInterval(()=>{},1000)'], {
    stdio: 'ignore', timeoutMs: 100, terminate: pid => process.kill(pid, 'SIGKILL'),
    pauseInterval: () => ({ startedAt: Date.now(), resumedAt: null }),
  });
  assert.match(late.error!.message, /invalid planned process pause/);
  const abort = new AbortController();
  const heldAt = Date.now();
  const timer = setTimeout(() => abort.abort(), 350);
  try {
    const cancelled = await runBounded(process.execPath, ['-e', 'setInterval(()=>{},1000)'], {
      stdio: 'ignore', timeoutMs: 250, signal: abort.signal,
      terminate: pid => process.kill(pid, 'SIGKILL'),
      pauseInterval: () => ({ startedAt: heldAt + 100, resumedAt: null }),
    });
    assert(cancelled.cancelled);
    assert.equal(cancelled.timedOut, false);
  } finally { clearTimeout(timer); }
});
