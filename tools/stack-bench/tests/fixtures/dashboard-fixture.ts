import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { ARTIFACT_FILE, emptyArtifactIdentities, writeArtifact } from '../../src/evidence/artifacts.js';
import { claimNextAttempt, createCampaignState, finishCampaignExecution }
  from '../../src/campaigns/campaign-scheduler.js';
import type { CampaignState } from '../../src/campaigns/campaign-scheduler.js';
import { compileCampaignFile } from '../../src/campaigns/campaign-compiler.js';
import type { CampaignAttemptPlan, CompiledCampaignPlan }
  from '../../src/campaigns/campaign-compiler.js';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../../src/progression/progression-definition.js';
import { createCheckEvidence } from '../../src/evidence/check-evidence.js';
import { hashDirectory } from '../../src/evidence/provenance.js';
import { progressionEngine } from '../../src/progression/progression-engine.js';
import { writeProgressionState } from '../../src/progression/progression-state.js';
import type { ProgressionState } from '../../src/progression/progression-state.js';
import { STACK_BENCH_ROOT } from '../../src/package-root.js';

// The results trees the dashboard tests and the screenshot script read: a
// sequential week at appliance scale, and one dependency campaign whose
// progression state carries builds, repairs and a failure.

export const EXAMPLE_CAMPAIGN = join(STACK_BENCH_ROOT, 'appliance', 'campaign.example.json');
export const DEPENDENCY_CAMPAIGN = join(STACK_BENCH_ROOT, 'appliance',
  'campaign.ecommerce-progression-reference.json');

export function writeCampaign(root: string, plan: CompiledCampaignPlan, state: CampaignState): void {
  mkdirSync(root, { recursive: true });
  writeArtifact(join(root, 'plan.json'), { kind: 'campaign_plan', id: `${plan.id}-plan`,
    identities: emptyArtifactIdentities(), payload: plan });
  writeArtifact(join(root, 'state.json'), { kind: 'campaign_state', id: `${plan.id}-state`,
    identities: emptyArtifactIdentities(), payload: state });
}

// A results tree the size of a real appliance week: thirty campaigns, nine
// attempts each, with the artifacts the views actually read.
export const FIXTURE_CAMPAIGNS = 30;

export function writeRunEvidence(output: string, plan: CompiledCampaignPlan,
  attempt: CampaignAttemptPlan, deficit: number): void {
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter)!;
  const stack = plan.stacks.find(item => item.id === attempt.stack)!;
  const selection = structuredClone(attempt.condition.requested.levels
    .find(item => item.level === 1)!.selection) as { sha256: string;
      scoredPoints: number; scoredChecks?: Array<{ stableKey: string; points: number }> };
  const max = selection.scoredPoints;
  const score = max;
  const outcome = { kind: 'passed', inconclusive: [], harnessFailures: [] };
  const firstOutcome = { kind: 'app_failure', phase: 'grading', reason: null,
    appFailures: [], inconclusive: [], harnessFailures: [] };
  const source = join(output, 'source');
  mkdirSync(source, { recursive: true });
  writeFileSync(join(source, 'app.js'), 'export const ready = true;\n');
  const checks = selection.scoredChecks ?? [];
  const checkKeys = checks.map(check => check.stableKey);
  const evidence = createCheckEvidence({ status: 'passed', code: 'completed',
    phase: 'assertion', startedAtMs: 1, completedAtMs: 2 });
  writeArtifact(join(output, 'grading', 'bundle.json'), {
    kind: 'grade_bundle', id: `${attempt.id}-grade-l1`, payload: {
      observation: 'scored', source: { sha256: hashDirectory(source).sha256 },
      suites: { fixture: { features: [{ id: 'fixture', name: 'Fixture',
        setupEvidence: createCheckEvidence({ status: 'passed', code: 'completed',
          phase: 'setup', startedAtMs: 1, completedAtMs: 2 }),
        criteria: checks.map(check => ({ id: check.stableKey, stableKey: check.stableKey,
          desc: check.stableKey, points: check.points, evidence })) }] } },
      totals: { score, max },
      selection: { sha256: selection.sha256, checks, attemptedChecks: checkKeys,
        reportedChecks: checkKeys, notRun: [] },
    },
  });
  writeArtifact(join(output, 'run.json'), { kind: 'benchmark_run', id: `${attempt.id}-run`,
    attempt: { id: `${attempt.id}-run`, parentId: attempt.id },
    identities: emptyArtifactIdentities({ engine: plan.identities.engine,
      experiment: { id: plan.id, version: plan.version, sha256: plan.contentSha256,
        state: plan.state },
      agentAdapter: agent.identity, stackAdapter: stack }),
    payload: {
      mode: attempt.mode, track: plan.definition.track, backend: attempt.stack,
      model: attempt.model, pricing: attempt.pricing, guidance: attempt.guidance,
      condition: attempt.condition, selectionRequest: plan.definition.selection,
      featureCatalog: attempt.featureCatalog ?? null,
      dependencyPolicy: attempt.dependencyPolicy ?? null, progressionOwner: null,
      skills: attempt.skills, runtime: { buildImage: plan.definition.runtime.buildImage },
      totals: { score, max, costUsd: 1.25, costComplete: true, durationSec: 900 },
      levels: [{ level: 1, graded: true, selection, score, max, fixRounds: 3,
        firstBuild: { score: score - deficit, max, outcome: firstOutcome },
        repair: { status: 'corrected', budgetRounds: 3, roundsUsed: 3, stopReason: null },
        durationSec: 900, buildCostUsd: 1.25, outcome }],
      outcome: { kind: 'passed' },
    } });
  writeFileSync(join(output, 'process.stdout.log'),
    `=== ${attempt.stack}-l1-first (${attempt.stack}) ===\n  TOTAL ... ${score - 1}/${max}\n`
    + `--- repair round 1/3 ---\n=== ${attempt.stack}-l1-fix1 (${attempt.stack}) ===\n`
    + `  TOTAL ... ${score}/${max}\n`);
}

export function writeFixtureResults(resultsRoot: string, { running = false }: {
  running?: boolean;
} = {}): void {
  const plan = compileCampaignFile(EXAMPLE_CAMPAIGN);
  const now = '2026-08-18T12:00:00.000Z';
  for (let index = 0; index < FIXTURE_CAMPAIGNS; index += 1) {
    const directory = join(resultsRoot, 'campaigns', `fixture-run-${index}`);
    let state = createCampaignState(plan, { now });
    const claims = [];
    for (;;) {
      const claimed = claimNextAttempt(state, { now, admissionId: `admission-${index}` });
      if (!claimed.claim) break;
      claims.push(claimed.claim);
      state = running && claims.length === 1 ? claimed.state
        : finishCampaignExecution(claimed.state, claimed.claim.executionId,
          { exitCode: 0, run: { outcome: { kind: 'passed' } } }, { now });
    }
    writeCampaign(directory, plan, state);
    claims.forEach((claim, ordinal) => {
      const output = join(directory, claim.output);
      mkdirSync(output, { recursive: true });
      writeRunEvidence(output, plan, claim.attempt, (ordinal % 3) * 4 + 1);
    });
  }
}

export function dependencyProgressionEvidence(plan: CompiledCampaignPlan,
  attempt: CampaignAttemptPlan): ProgressionState {
  assert.ok(plan.featureCatalog && plan.dependencyPolicy);
  const input = compileProgressionInput(dependencyRuntimeDefinition(
    plan.featureCatalog, plan.dependencyPolicy));
  let state = progressionEngine.initialize(input.definition);
  const missed = new Set<string>();
  for (let step = 1; step <= 24; step += 1) {
    if (progressionEngine.nextAction(state).type === 'terminal') break;
    const selection = progressionEngine.gradingSelection(state);
    // One feature per grade misses a check the first time it is graded, so the
    // replay carries repairs as well as builds.
    const target = selection.nodeIds.find(nodeId => !missed.has(nodeId));
    state = progressionEngine.recordResult(state, {
      attemptId: `${attempt.id}-${step}`,
      outcome: 'conclusive',
      nodes: selection.nodeIds.map(nodeId => ({ id: nodeId,
        checks: selection.checks.filter(check => check.nodeId === nodeId).map((check, index) => ({
          id: check.id,
          outcome: nodeId === target && index === 0 ? 'fail' as const : 'pass' as const })) })),
    });
    if (target) missed.add(target);
  }
  return state;
}


// One dependency campaign with a progression state per stack, which is what the
// graph and the replay read.
export function writeDependencyResults(resultsRoot: string, key: string): void {
  const plan = compileCampaignFile(DEPENDENCY_CAMPAIGN);
  assert.ok(plan.featureCatalog && plan.dependencyPolicy);
  const now = '2026-08-25T12:00:00.000Z';
  const directory = join(resultsRoot, 'campaigns', key);
  let state = createCampaignState(plan, { now });
  const claims = [];
  for (;;) {
    const claimed = claimNextAttempt(state, { now, admissionId: 'admission-1' });
    if (!claimed.claim) break;
    claims.push(claimed.claim);
    state = claimed.state;
  }
  writeCampaign(directory, plan, state);
  const progression = compileProgressionInput(dependencyRuntimeDefinition(
    plan.featureCatalog, plan.dependencyPolicy));
  for (const claim of claims) {
    const output = join(directory, claim.output);
    mkdirSync(join(output, 'source'), { recursive: true });
    writeProgressionState(join(output, ARTIFACT_FILE.progressionState), {
      progression,
      featureCatalogIdentity: plan.featureCatalog.identity,
      dependencyPolicyIdentity: plan.dependencyPolicy.identity,
      owner: { schemaVersion: 1,
        campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
        attempt: { id: claim.attempt.id, track: plan.definition.track, stack: claim.attempt.stack,
          agentAdapter: claim.attempt.agentAdapter, model: claim.attempt.model,
          conditionSha256: claim.attempt.condition.sha256 },
        workspace: { appDirectory: 'source' } },
      state: dependencyProgressionEvidence(plan, claim.attempt) });
  }
}
