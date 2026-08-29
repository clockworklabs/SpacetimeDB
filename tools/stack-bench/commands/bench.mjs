#!/usr/bin/env node
// Stack Bench: run the whole benchmark for one backend, unattended.
//
// For each level: build (or upgrade), grade, and if anything failed hand the
// agent a behavioural bug report and let it fix — up to --fix-rounds times —
// re-grading after each attempt. Records score, cost, time and fix rounds per
// level, then writes a summary.
//
// Usage:
//   node commands/bench.mjs --backend spacetime --levels 1-5 [--model claude-sonnet-5]
//                  [--fix-rounds 10] [--run-index 0] [--out <dir>]
//                  [--retain-backend] [--no-media]
//
// The benchmark runs its own SpacetimeDB host (STACK_BENCH_STDB_URI, default
// 127.0.0.1:3210, data in .spacetime-data) rather than a machine-wide one, so
// resource measurements describe the module under test and a durability restart
// cannot disturb anything else. It is started if absent and stopped at the end
// unless --retain-backend.

import { execFile, execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync, cpSync, rmSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';
import { loadTrack, resultsName, portsFor, workDirFor, assertNoPortCollisions,
  moduleName, dbName, suitesFor, DEFAULT_TRACK } from '../src/composition/tracks.js';
import { killTree } from '../src/runtime/platform.js';
import { formatRepairProgress } from '../src/evidence/scoring.js';
import { emptyArtifactIdentities, readArtifact, readArtifactPayload, writeArtifact, writeRunJson } from '../src/evidence/artifacts.js';
import { aggregateRunOutcome, classifyBundle, ladderMayAdvance, ladderMayContinue,
  mutationControlEligible, runExitCode } from '../src/evidence/outcomes.js';
import { summarizeSessions } from '../src/evidence/session-metrics.js';
import { hashDirectory, sha256 } from '../src/evidence/provenance.js';
import { createBackendLease, newRunId, publicBackendLease, readBackendLease,
  acquireResourceLocks, backendResourceLockKeys, releaseResourceLocks, resourceLockScope,
  updateBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { captureBackendDiagnostics } from '../src/runtime/backend-control.js';
import { releaseBackendLease } from '../src/runtime/backend-teardown.js';
import { resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { createAgentVisibleTaskRequest, createBoundRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import { criterionEvidence, evidencePassed } from '../src/evidence/check-evidence.js';
import { executeStackCapability } from '../src/stacks/stack-adapter-contract.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { agentRecipeIdentity, agentRequestArgv, agentSessionFailure,
  validateAgentResult } from '../src/agents/agent-adapter-contract.js';
import { AGENT_ADAPTER_REGISTRY, agentAdapterIdentity } from '../src/agents/agent-adapters.js';
import { runPreflight } from '../src/runtime/preflight.js';
import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.js';
import { SUPERVISOR_STATE_VERSION, writeRecoveryArtifact } from '../src/runtime/recovery.js';
import { resolveAgentCredential } from '../src/agents/agent-credentials.js';
import { sandboxProbeMode } from '../src/runtime/sandbox.js';
import { hashAppSource, restoreAppSource, seedAppSource, snapshotAppSource } from '../src/runtime/source-snapshot.js';
import { preserveLevelCheckpoint } from '../src/runtime/source-checkpoint.js';
import { compareRepairBaseline, createRepairGrant } from '../src/runtime/repair-grant.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.js';
import { repairEvidenceDecision } from '../src/evidence/repair-evidence.js';
import { mutationControlArgv, mutationControlTimeoutMs } from '../src/evidence/mutation-control.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import { compileProgressionInput, dependencyRuntimeDefinition, progressionLevels,
  validateFeatureCatalogInput, validateProgressionInput }
  from '../src/progression/progression-definition.js';
import { resolveProgressionRecipeAction, resolveProgressionRecipeLevelSelection }
  from '../src/progression/progression-recipe-selection.js';
import { createLiveProgressionExecution }
  from '../src/progression/live-progression.js';
import { validateCompiledCampaignPlan } from '../src/campaigns/campaign-compiler.js';
import { campaignAdmissionSmokeReuse, readCampaignAdmission }
  from '../src/campaigns/campaign-admission.js';
import { gradingRunTimeoutMs, selectedGradingSourceCount }
  from '../src/runtime/grading-timeout.js';
import { claudeRatesForModel } from '../src/evidence/claude-usage-cost.js';
import { PRICING_UNIT, validatePricingAuthority }
  from '../src/evidence/pricing-authority.js';

import { STACK_BENCH_ROOT as ROOT, stagedEntrypoint } from '../src/package-root.js';
const COMMAND_TIMEOUT_MS = 20 * 60_000;

export function addCostUsd(...values) {
  return Number(values.reduce((sum, value) => sum + (value ?? 0), 0).toFixed(6));
}

// Grading writes private evidence into the mounted app directory because the
// grader and app share one runtime. A repair session may receive only the
// behavioural BUG_REPORT.md produced from that evidence. Remove the raw
// bundle, scenario names, screenshots, and grader output before the coding
// model starts.
export function clearPrivateGradingEvidence(appDir) {
  rmSync(join(resolve(appDir), 'stack-bench'), { recursive: true, force: true });
}

export function parseArgs(argv) {
  const a = { model: null, agentAdapter: 'claude-code',
    fixRounds: 10, runIndex: 0, levels: '1', levelsProvided: false, media: true,
    maxStalledRepairs: 3,
    guidance: 'prescribed', track: DEFAULT_TRACK, packIds: [], checkKeys: [],
    featureIds: [], requestedSpecifications: [], expectedSpecifications: [],
    observedSpecifications: [],
    behavioralReview: false, mutationMaxRuntimeMinutes: 60 };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--backend': a.backend = argv[++i]; break;
      case '--track': a.track = argv[++i]; break;
      case '--levels': a.levels = argv[++i]; a.levelsProvided = true; break;
      case '--campaign-file': a.campaignFile = resolve(argv[++i]); break;
      case '--feature-catalog-sha256': a.featureCatalogSha256 = argv[++i]; break;
      case '--dependency-policy-sha256': a.dependencyPolicySha256 = argv[++i]; break;
      case '--campaign-sha256': a.campaignSha256 = argv[++i]; break;
      case '--campaign-attempt-id': a.campaignAttemptId = argv[++i]; break;
      case '--campaign-admission-id': a.campaignAdmissionId = argv[++i]; break;
      case '--progression-resume-from': a.progressionResumeFrom = resolve(argv[++i]); break;
      case '--recipe': a.recipe = argv[++i]; break;
      case '--pack': a.packIds.push(...argv[++i].split(',').filter(Boolean)); break;
      case '--check': a.checkKeys.push(...argv[++i].split(',').filter(Boolean)); break;
      case '--model': a.model = argv[++i]; break;
      case '--pricing-json': a.pricing = JSON.parse(argv[++i]); break;
      case '--fix-rounds': a.fixRounds = Number(argv[++i]); break;
      case '--max-stalled-repairs': a.maxStalledRepairs = Number(argv[++i]); break;
      case '--max-budget-usd': a.maxBudgetUsd = Number(argv[++i]); break;
      case '--run-index': a.runIndex = parseInt(argv[++i], 10); break;
      case '--out': a.out = argv[++i]; break;
      case '--app': a.app = argv[++i]; break;
      case '--url': a.url = argv[++i]; break;
      case '--agent-adapter': a.agentAdapter = argv[++i]; break;
      case '--no-media': a.media = false; break;
      case '--retain-backend': a.retainBackend = true; break;
      case '--behavioral-review': a.behavioralReview = true; break;
      case '--stack': a.guidance = argv[++i] === 'free' ? 'minimal' : 'prescribed'; break;
      case '--guidance': a.guidance = argv[++i]; break;
      case '--guidance-document-json': a.guidanceDocument = JSON.parse(argv[++i]); break;
      case '--condition-json': a.condition = JSON.parse(argv[++i]); break;
      case '--selection-json': a.selectionRequest = JSON.parse(argv[++i]); break;
      case '--task-mode': a.taskMode = argv[++i]; break;
      case '--feature-module': a.featureIds.push(...argv[++i].split(',').filter(Boolean)); break;
      case '--request-spec': a.requestedSpecifications.push(...argv[++i].split(',').filter(Boolean)); break;
      case '--expect-spec': a.expectedSpecifications.push(...argv[++i].split(',').filter(Boolean)); break;
      case '--observe-spec': a.observedSpecifications.push(...argv[++i].split(',').filter(Boolean)); break;
      case '--skip-probe': a.skipProbe = true; break;
      // Which reference documents to inline (spacetime only). The variable
      // under test in the cost work; passed straight through to agent.mjs.
      case '--skills': a.skills = argv[++i].split(',').filter(Boolean); break;
      case '--skills-json': a.skills = JSON.parse(argv[++i]); break;
      case '--api-key': a.apiKey = argv[++i]; break;
      case '--api-key-file': a.apiKeyFile = resolve(argv[++i]); break;
      case '--mutations': a.mutations = resolve(argv[++i]); break;
      case '--mutation-shard-index': a.mutationShardIndex = Number(argv[++i]); break;
      case '--mutation-shard-count': a.mutationShardCount = Number(argv[++i]); break;
      case '--mutation-resume-from': a.mutationResumeFrom = resolve(argv[++i]); break;
      case '--mutation-checkpoint-out': a.mutationCheckpointOut = resolve(argv[++i]); break;
      case '--mutation-baseline-bundle': a.mutationBaselineBundle = resolve(argv[++i]); break;
      case '--expected-mutation-calibration-json': {
        a.expectedMutationCalibration = JSON.parse(argv[++i]); break;
      }
      case '--mutation-max-runtime-minutes': a.mutationMaxRuntimeMinutes = Number(argv[++i]); break;
      case '--reference-mutation-only': a.referenceMutationOnly = true; break;
      // Reuse an existing lower-level build when the run plan requires an upgrade.
      case '--seed-from': a.seedFrom = argv[++i]; break;
      case '--parent-attempt-id': a.parentAttemptId = argv[++i]; break;
      case '--repair-from': a.repairFrom = resolve(argv[++i]); break;
      case '--repair-level': a.repairLevel = Number(argv[++i]); break;
      default: console.error(`Unknown argument: ${argv[i]}`); process.exit(2);
    }
  }
  if (!a.backend && !a.repairFrom) {
    console.error('Usage: node commands/bench.mjs --backend <b> --levels 1-3 [--fix-rounds 10] [--run-index N]');
    process.exit(2);
  }
  if ((a.mutationResumeFrom || a.mutationCheckpointOut || a.mutationBaselineBundle)
      && !a.mutations) {
    throw new Error('mutation control options require --mutations');
  }
  if (a.expectedMutationCalibration && !a.mutations) {
    throw new Error('--expected-mutation-calibration-json requires --mutations');
  }
  if (!Number.isFinite(a.mutationMaxRuntimeMinutes) || a.mutationMaxRuntimeMinutes < 1
      || a.mutationMaxRuntimeMinutes > 120) {
    throw new Error('--mutation-max-runtime-minutes must be from 1 through 120');
  }
  if (a.referenceMutationOnly && (!a.mutations || a.agentAdapter !== 'reference-fixture'
      || a.fixRounds !== 0 || !a.app || a.campaignFile)) {
    throw new Error('--reference-mutation-only requires a mutation-bound reference fixture run');
  }
  if (a.mutationBaselineBundle && !a.referenceMutationOnly) {
    throw new Error('--mutation-baseline-bundle is an internal reference mutation option');
  }
  if (a.repairFrom && (!Number.isSafeInteger(a.repairLevel) || a.repairLevel < 1)) {
    throw new Error('--repair-from requires --repair-level with a positive integer');
  }
  if (a.campaignFile && (!a.campaignSha256 || !a.campaignAttemptId)) {
    throw new Error('--campaign-file requires --campaign-sha256 and --campaign-attempt-id');
  }
  if (!a.campaignFile && (a.campaignSha256 || a.campaignAttemptId
    || a.campaignAdmissionId || a.featureCatalogSha256 || a.dependencyPolicySha256)) {
    throw new Error('campaign binding requires --campaign-file');
  }
  if (a.progressionResumeFrom && !a.campaignFile) {
    throw new Error('--progression-resume-from requires a compiled campaign');
  }
  if (a.campaignFile) {
    const unsupported = [
      [a.maxStalledRepairs !== 3, '--max-stalled-repairs'],
      [a.skipProbe === true, '--skip-probe'],
      [a.behavioralReview === true, '--behavioral-review'],
      [a.mutations !== undefined || a.mutationShardIndex !== undefined
        || a.mutationShardCount !== undefined, '--mutations'],
      [a.seedFrom !== undefined, '--seed-from'],
      [a.repairFrom !== undefined || a.repairLevel !== undefined, '--repair-from'],
      [a.app !== undefined, '--app'], [a.url !== undefined, '--url'],
      [a.retainBackend === true, '--retain-backend'],
      [a.apiKey !== undefined || a.apiKeyFile !== undefined, 'credential override'],
      [a.recipe !== undefined || a.packIds.length > 0 || a.checkKeys.length > 0
        || a.featureIds.length > 0 || a.requestedSpecifications.length > 0
        || a.expectedSpecifications.length > 0 || a.observedSpecifications.length > 0,
      'direct recipe selection'],
    ].filter(([changed]) => changed).map(([, name]) => name);
    if (unsupported.length) {
      throw new Error(`campaign progression input cannot override ${unsupported.join(', ')}`);
    }
    const artifact = readArtifact(a.campaignFile, { expectedKind: 'campaign_plan' });
    const plan = validateCompiledCampaignPlan(artifact.payload);
    if (plan.contentSha256 !== a.campaignSha256) {
      throw new Error('--campaign-sha256 does not match the compiled campaign plan');
    }
    const attempt = plan.attempts.find(item => item.id === a.campaignAttemptId);
    if (attempt) {
      a.condition ??= structuredClone(attempt.condition);
      a.skills ??= structuredClone(attempt.skills);
      a.selectionRequest ??= structuredClone(plan.definition.selection);
      a.guidanceDocument ??= structuredClone(
        attempt.condition?.guidance?.documents?.[attempt.stack]);
    }
    if (!attempt || attempt.stack !== a.backend || attempt.agentAdapter !== a.agentAdapter
      || attempt.model !== a.model
      || canonicalDefinitionJson(attempt.pricing)
        !== canonicalDefinitionJson(a.pricing)
      || plan.definition.track !== a.track || attempt.guidance !== a.guidance
      || canonicalDefinitionJson(attempt.condition) !== canonicalDefinitionJson(a.condition)
      || canonicalDefinitionJson(attempt.skills) !== canonicalDefinitionJson(a.skills)
      || canonicalDefinitionJson(plan.definition.selection)
        !== canonicalDefinitionJson(a.selectionRequest)
      || canonicalDefinitionJson(attempt.condition?.guidance?.documents?.[attempt.stack])
        !== canonicalDefinitionJson(a.guidanceDocument)
      || plan.definition.budgets.fixRounds !== a.fixRounds
      || plan.definition.budgets.maxCostUsdPerAttempt !== (a.maxBudgetUsd ?? null)
      || a.parentAttemptId !== attempt.id || a.media !== false) {
      throw new Error('--campaign-attempt-id does not match the requested campaign attempt');
    }
    a.experimentIdentity = {
      id: plan.id, version: plan.version, sha256: plan.contentSha256, state: plan.state,
    };
    if (a.campaignAdmissionId) {
      const admission = readCampaignAdmission(dirname(a.campaignFile),
        a.campaignAdmissionId, plan);
      const image = plan.definition.runtime.buildImage
        ?? process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE;
      a.campaignAdmission = {
        id: a.campaignAdmissionId,
        ...campaignAdmissionSmokeReuse(admission, {
          agentAdapter: a.agentAdapter,
          runIndex: a.runIndex,
          backend: a.backend,
          image,
        }),
      };
    }
    a.runMode = structuredClone(attempt.mode);
    if (plan.featureCatalog) {
      a.featureCatalog = validateFeatureCatalogInput(plan.featureCatalog);
      if (a.featureCatalog.identity.sha256 !== a.featureCatalogSha256
        || canonicalDefinitionJson(attempt.featureCatalog)
          !== canonicalDefinitionJson(a.featureCatalog.identity)) {
        throw new Error('--feature-catalog-sha256 does not match the compiled campaign plan');
      }
    } else if (a.featureCatalogSha256 !== undefined || attempt.featureCatalog !== undefined) {
      throw new Error('campaign attempt has an unexpected feature catalog');
    }
    if (attempt.mode.id === 'dependency') {
      if (plan.dependencyPolicy?.identity?.sha256 !== a.dependencyPolicySha256
        || canonicalDefinitionJson(attempt.dependencyPolicy)
          !== canonicalDefinitionJson(plan.dependencyPolicy.identity)) {
        throw new Error('--dependency-policy-sha256 does not match the compiled campaign plan');
      }
      a.dependencyPolicy = plan.dependencyPolicy;
      a.progression = compileProgressionInput(dependencyRuntimeDefinition(
        a.featureCatalog, a.dependencyPolicy));
      a.progressionOwner = { schemaVersion: 1,
        campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
        attempt: { id: attempt.id, track: plan.definition.track, stack: attempt.stack,
          agentAdapter: attempt.agentAdapter, model: attempt.model,
          conditionSha256: attempt.condition.sha256 } };
    }
  }
  if (a.progression) {
    if (a.levelsProvided) throw new Error('--levels cannot be combined with progression input');
    a.progression = validateProgressionInput(a.progression);
    a.levelList = progressionLevels(a.progression);
    a.levels = `${a.levelList[0]}-${a.levelList.at(-1)}`;
  } else {
    const [from, to] = a.levels.split('-').map(Number);
    a.levelList = Array.from({ length: (to ?? from) - from + 1 }, (_, i) => from + i);
  }
  if (a.recipe && a.levelList.length !== 1) {
    throw new Error('--recipe requires exactly one requested level');
  }
  if (!Number.isInteger(a.fixRounds) || a.fixRounds < 0 || a.fixRounds > 20) {
    throw new Error('--fix-rounds must be an integer from 0 through 20');
  }
  if (!Number.isInteger(a.maxStalledRepairs) || a.maxStalledRepairs < 0
    || a.maxStalledRepairs > 20) {
    throw new Error('--max-stalled-repairs must be an integer from 0 through 20');
  }
  if (a.maxBudgetUsd !== undefined && (!Number.isFinite(a.maxBudgetUsd) || a.maxBudgetUsd <= 0)) {
    throw new Error('--max-budget-usd must be a positive number');
  }
  if ((a.mutationShardIndex === undefined) !== (a.mutationShardCount === undefined)) {
    throw new Error('--mutation-shard-index and --mutation-shard-count must be supplied together');
  }
  return a;
}

export function repairProgressState(previous, bundle) {
  const outcome = classifyBundle(bundle);
  const score = bundle?.totals?.score ?? null;
  const fingerprint = canonicalDefinitionJson({
    kind: outcome.kind,
    phase: outcome.phase ?? null,
    appFailures: [...(outcome.appFailures ?? [])].sort(),
    inconclusive: [...(outcome.inconclusive ?? [])].sort(),
    harnessFailures: [...(outcome.harnessFailures ?? [])].sort(),
    contractFailures: (bundle?.suites?.lint?.results ?? [])
      .filter(result => result.status === 'FAIL')
      .map(result => ({ id: result.id, detail: result.detail ?? null })),
  });
  const stalledRounds = previous && score !== null && previous.score !== null
    && score <= previous.score && fingerprint === previous.fingerprint
    ? previous.stalledRounds + 1 : 0;
  return { score, fingerprint, stalledRounds };
}

export function repairHistoryEntry(round, before, after, result) {
  const failureKeys = bundle => {
    const outcome = classifyBundle(bundle);
    const contract = (bundle?.suites?.lint?.results ?? [])
      .filter(item => item.status === 'FAIL').map(item => `testing-interface/${item.id}`);
    return [...new Set([...(outcome.appFailures ?? []).filter(key => key !== 'contract-lint'),
      ...contract])].sort();
  };
  return {
    round,
    beforeScore: before?.totals?.score ?? null,
    beforeMax: before?.totals?.max ?? null,
    afterScore: after?.totals?.score ?? null,
    afterMax: after?.totals?.max ?? null,
    result,
    remainingFailures: failureKeys(after),
  };
}

export function levelGradeIsUsable(bundleOutcome, progressionAttempt = null) {
  if (progressionAttempt) return progressionAttempt.outcome === 'conclusive';
  return !['provider_failure', 'ungraded', 'harness_failure'].includes(bundleOutcome.kind);
}

export function dependencyRepairBudget(action, conclusiveAttempts) {
  if (!action || action.strikes?.scope !== 'feature'
    || !Number.isSafeInteger(action.strikes.maxRemaining)
    || action.strikes.maxRemaining < 0
    || !Number.isSafeInteger(conclusiveAttempts) || conclusiveAttempts < 0) {
    throw new Error('dependency repair budget requires one valid feature-strike action');
  }
  return Math.max(0, conclusiveAttempts + action.strikes.maxRemaining - 1);
}

export function dependencyStrikeRecords(state, level, includedNodeIds = []) {
  const included = new Set(includedNodeIds);
  return state.definition.nodes
    .filter(node => node.level === level
      || state.nodes[node.id].exhaustedAtLevel === level
      || included.has(node.id))
    .map(node => {
      const nodeState = state.nodes[node.id];
      return { nodeId: node.id,
        initialBudget: nodeState.strikes.initialBudget,
        granted: nodeState.strikes.granted,
        budget: nodeState.strikes.budget,
        used: nodeState.strikes.used,
        remaining: nodeState.strikes.budget - nodeState.strikes.used,
        exhaustionReason: nodeState.exhaustionReason };
    })
    .sort((left, right) => left.nodeId.localeCompare(right.nodeId));
}

function snapshotSource(appDir, to) {
  snapshotAppSource(appDir, to);
}

export function preserveFinalPackageEvidence({ appDir, outputDir }) {
  const failures = [];
  let source = null;
  let grading = null;

  try {
    const live = hashAppSource(appDir);
    const destination = join(outputDir, 'source');
    snapshotSource(appDir, destination);
    const saved = hashDirectory(destination);
    if (saved.sha256 !== live.sha256 || saved.files.length !== live.files.length) {
      throw new Error('preserved final source differs from the live application source');
    }
    source = { directory: 'source', sha256: saved.sha256, files: saved.files.length };
  } catch (error) {
    failures.push(`source: ${String(error.message).split(/\r?\n/)[0]}`);
  }

  try {
    const from = join(appDir, 'stack-bench');
    const destination = join(outputDir, 'grading');
    if (!existsSync(join(from, 'bundle.json'))) {
      throw new Error('final grader produced no bundle.json');
    }
    rmSync(destination, { recursive: true, force: true });
    cpSync(from, destination, {
      recursive: true,
      filter: path => !/[\\/]media([\\/]|$)/.test(path),
    });
    const bundle = readArtifactPayload(join(destination, 'bundle.json'), {
      expectedKind: 'grade_bundle',
    });
    if (!source || bundle.source?.sha256 !== source.sha256) {
      throw new Error('final grading bundle does not match the preserved application source');
    }
    grading = { directory: 'grading', artifact: 'grading/bundle.json',
      sourceSha256: bundle.source.sha256 };
  } catch (error) {
    failures.push(`grading: ${String(error.message).split(/\r?\n/)[0]}`);
  }

  if (failures.length) {
    throw new Error(`could not preserve mandatory result package evidence: ${failures.join('; ')}`);
  }
  return { source, grading };
}

export function sourceBoundFirstBuildOutcome(bundle, source) {
  if (source) return classifyBundle(bundle);
  const reason = 'the first-build source could not be preserved and verified';
  return { kind: 'harness_failure', phase: 'first-build-source', reason,
    appFailures: [], inconclusive: [], harnessFailures: [reason] };
}

// Restore in place: generated layouts are not prescribed, active watchers keep
// their directory handles, and dependency folders survive at any depth.
function restoreSource(from, appDir) {
  restoreAppSource(from, appDir);
}

// Check contamination after every coding session. File-tool permissions do not
// govern shell reads, so the transcript audit remains a separate hard gate.
function auditContamination(appDir) {
  const args = [join(ROOT, 'dist', 'commands', 'leak-audit.js'), '--app', appDir, '--json'];
  let firstFailure = null;
  for (let attempt = 1; attempt <= 2; attempt++) {
    try {
      const audit = sh('node', args, { stdio: 'pipe' });
      const escapes = JSON.parse(audit).flatMap(r => r.hits ?? []);
      const serious = escapes.filter(h => /GRADER|CONTRACT|BENCHMARK NOTES|PROMPTS/.test(h.kind));
      if (firstFailure) {
        console.error(`  warning: contamination audit passed on retry after: ${auditFailureSummary(firstFailure)}`);
      }
      if (!serious.length) return null;
      return { kind: 'contaminated',
        evidence: [...new Set(serious.map(h => `${h.kind}: ${h.path.split('/').slice(-2).join('/')}`))].slice(0, 8),
        verdict: 'SCORES NOT USABLE — the build read the harness that grades it.' };
    } catch (error) {
      firstFailure ??= error;
      if (attempt === 2) {
        // An audit that could not run is not a pass. Keep the process details so
        // the failure can be repaired without another paid reproduction.
        return { kind: 'harness_failure',
          evidence: [`audit did not run after retry: ${auditFailureSummary(error)}`],
          verdict: 'SCORES NOT USABLE — nothing verified this build stayed inside its directory.' };
      }
    }
  }
  return null;
}

export function auditFailureSummary(error) {
  const message = String(error?.message ?? error).split(/\r?\n/)[0];
  const stderr = String(error?.stderr ?? '').trim().split(/\r?\n/)[0];
  const details = [
    Number.isInteger(error?.status) ? `exit ${error.status}` : null,
    error?.signal ? `signal ${error.signal}` : null,
    stderr ? `stderr: ${stderr}` : null,
  ].filter(Boolean);
  return details.length ? `${message} (${details.join('; ')})` : message;
}

function containerIdentity(name) {
  try {
    const id = execFileSync('docker', ['inspect', '--format', '{{.Id}}', name],
      { encoding: 'utf8', stdio: 'pipe', timeout: 120_000 }).trim();
    if (!id) throw new Error('empty container id');
    return { name, id };
  } catch (error) {
    throw new Error(`cannot lease ${name}: ${String(error.message).split('\n')[0]}`);
  }
}

const sh = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, {
    encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: COMMAND_TIMEOUT_MS, ...opts,
  });

let activeAgentChild = null;
// Set once a run owns resources. The top-level rejection handler invokes this
// directly; relying only on process 'exit' made cleanup best-effort precisely
// when an awaited build rejected unexpectedly.
let emergencyTeardown = null;

function runAgent(args, adapter, mode, level, appDir) {
  const remainingBudget = args.maxBudgetUsd == null ? null
    : addCostUsd(args.maxBudgetUsd, -(args.spentBudgetUsd ?? 0));
  if (remainingBudget !== null && remainingBudget <= 0) {
    throw new Error(`attempt cost cap of $${args.maxBudgetUsd} was exhausted before ${mode} L${level}`);
  }
  if (remainingBudget !== null && adapter.costLimit === 'unsupported') {
    throw new Error(`agent adapter ${adapter.id} cannot enforce --max-budget-usd`);
  }
  const recipeTask = args.recipeTasks?.get(level)?.agentRequest
    ?? args.recipeTasks?.get(level)?.request ?? null;
  const request = { mode, level, app: appDir, backend: args.backend, track: args.track,
    runIndex: args.runIndex, model: args.model, guidance: args.guidance, skills: args.skills,
    recipe: agentRecipeIdentity(args.recipe, recipeTask),
    guidanceDocument: args.guidanceDocument,
    credentialAliases: args.condition?.guidance?.credentialAliases ?? {},
    recipeTask,
    maxBudgetUsd: remainingBudget, adapterCostLimit: adapter.costLimit };
  request.pricing = args.pricing;
  const argv = agentRequestArgv(adapter, request);
  if (args.apiKey && !adapter.apiKeyEnvironmentVariable) {
    throw new Error(`agent adapter ${adapter.id} does not accept an API key`);
  }
  const env = { ...process.env,
    ...(args.apiKey ? { [adapter.apiKeyEnvironmentVariable]: args.apiKey } : {}) };
  return new Promise((resolveRun, rejectRun) => {
    const child = execFile('node', argv, {
      encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: adapter.deadlineMs,
      env,
    },
      (error, stdout, stderr) => {
        if (activeAgentChild === child) activeAgentChild = null;
        if (error) {
          error.stdout = stdout;
          error.stderr = stderr;
          rejectRun(error);
          return;
        }
        try {
          const result = validateAgentResult(JSON.parse(stdout.trim().split('\n').pop()), request);
          args.spentBudgetUsd = addCostUsd(args.spentBudgetUsd, result.costUsd);
          resolveRun(result);
        }
        catch (parseError) {
          // Preserve bounded output tails when the agent result is malformed;
          // teardown may remove the container that produced them.
          const stdoutTail = stdout.trim().slice(-2000) || '<empty>';
          const stderrTail = stderr.trim().slice(-4000) || '<empty>';
          rejectRun(new Error(`agent returned invalid JSON: ${parseError.message}\n`
            + `agent stdout tail:\n${stdoutTail}\nagent stderr tail:\n${stderrTail}`));
        }
      });
    activeAgentChild = child;
  });
}

export function runSessionRecord(session, round = null) {
  return {
    ...(round === null ? {} : { round }),
    sessionId: session.sessionId ?? null,
    costUsd: session.costUsd,
    durationMs: session.durationMs,
    usage: session.usage ?? null,
    costReceipts: session.costReceipts ?? [],
    costComplete: session.costComplete === true,
    providerThrottle: session.setup?.providerThrottle ?? null,
    tokens: session.tokens ?? null,
    outputTokens: session.outputTokens ?? null,
    turns: session.turns ?? null,
    promptBytes: session.promptBytes ?? null,
    thinking: session.thinking ?? null,
    transcript: session.transcript ?? null,
    provenance: session.provenance ?? null,
    providerMetadata: session.providerMetadata ?? null,
  };
}

export function finalizeRunTotals(run, started, { now = Date.now(), costComplete = true } = {}) {
  const inherited = new Set(run.progressionResume?.inheritedLevels ?? []);
  const currentLevels = run.levels.filter(level => !inherited.has(level.level));
  const currentExecutionCostUsd = addCostUsd(currentLevels.reduce((n, level) => n
    + (level.buildCostUsd ?? level.resumeCostUsd ?? 0)
    + (level.fixCostUsd ?? 0), 0));
  const priorExecutionCostUsd = run.progressionResume?.priorTotals?.costUsd ?? null;
  const cumulativeCostUsd = run.progressionResume
    ? (typeof priorExecutionCostUsd === 'number'
      ? addCostUsd(priorExecutionCostUsd, currentExecutionCostUsd) : null)
    : currentExecutionCostUsd;
  run.totals = {
    score: run.levels.reduce((n, level) => n + (level.score ?? 0), 0),
    max: run.levels.reduce((n, level) => n + (level.max ?? 0), 0),
    costUsd: cumulativeCostUsd,
    costComplete: costComplete && (!run.progressionResume
      || (typeof priorExecutionCostUsd === 'number'
        && run.progressionResume.priorTotals?.costComplete !== false)),
    ...(run.progressionResume ? { priorExecutionCostUsd, currentExecutionCostUsd,
      cumulativeCostUsd } : {}),
    fixRounds: run.levels.reduce((n, level) => n + (level.fixRounds ?? 0), 0),
    sessions: run.levels.reduce((n, level) => n + (level.sessionTotals?.sessions ?? 0), 0),
    tokens: run.levels.reduce((n, level) => n + (level.sessionTotals?.tokens ?? 0), 0),
    outputTokens: run.levels.reduce((n, level) => n + (level.sessionTotals?.outputTokens ?? 0), 0),
    turns: run.levels.reduce((n, level) => n + (level.sessionTotals?.turns ?? 0), 0),
    modelDurationMs: run.levels.reduce((n, level) => n
      + (level.sessionTotals?.durationMs ?? 0), 0),
    durationSec: Math.round((now - started) / 1000),
    ungraded: run.levels.filter(level => !level.graded).map(level => level.level),
  };
  return run.totals;
}

export function formatLevelSummary(level) {
  const starting = level.firstBuild?.score != null
    ? `${level.firstBuild.score}/${level.firstBuild.max} unaided -> `
    : level.baseline?.score != null ? `${level.baseline.score}/${level.baseline.max} resumed -> ` : '';
  const score = level.graded ? `${starting}${level.score}/${level.max}` : 'NOT GRADED';
  const repairs = Number.isInteger(level.fixRounds) ? level.fixRounds : 0;
  const repairLabel = `${repairs} ${repairs === 1 ? 'repair' : 'repairs'}`;
  const totalCost = (level.buildCostUsd ?? level.resumeCostUsd ?? 0) + (level.fixCostUsd ?? 0);
  const durationSec = Number.isFinite(level.durationSec)
    ? level.durationSec : Math.round((level.durationMs ?? 0) / 1000);
  const status = level.error
    ? `stopped: ${level.error.replaceAll('-', ' ')}`
    : level.repair?.status?.replaceAll('-', ' ') ?? 'complete';
  return `L${level.level}: ${score} | ${repairLabel} | $${totalCost.toFixed(2)} total`
    + ` ($${(level.fixCostUsd ?? 0).toFixed(2)} repairs) | ${status} | ${durationSec}s`;
}

export function gradeArgv(args, appDir, url, label, level, track, parentAttemptId,
  { observation = 'scored', out = null, sourceSha256 = null } = {}) {
  const restartSpec = restartSpecFor(args, appDir, track);
  const expressPort = restartSpec.port ?? null;
  return [stagedEntrypoint('commands', 'run-suite.mjs'), '--app', appDir, '--url', url,
    '--backend', args.backend, '--label', label, '--level', String(level),
    '--track', args.track,
    ...(expressPort === null ? []
      : ['--reseed-probe', track.reseedProbeExpectation
        ? `http://localhost:${expressPort}${track.restartProbe}`
        : url]),
    ...(expressPort !== null && track.reseedProbeExpectation
      ? ['--reseed-probe-expectation-json', JSON.stringify(track.reseedProbeExpectation)] : []),
    '--run-index', String(args.runIndex),
    '--parent-attempt-id', parentAttemptId,
    '--observation', observation,
    ...(out ? ['--out', out] : []),
    ...(sourceSha256 ? ['--source-sha256', sourceSha256] : []),
    ...(args.recipe ? ['--recipe', args.recipe] : []),
    ...(args.recipeTasks?.get(level)
      ? ['--recipe-task-json', JSON.stringify(args.recipeTasks.get(level).request)] : []),
    ...(args.condition?.guidance?.credentialAliases
      ? ['--credential-aliases-json', JSON.stringify(
          args.condition.guidance.credentialAliases)] : []),
    ...(observation === 'scored' && args.recipeTasks && !args.progression
      ? ['--regression-checks-json', JSON.stringify([...args.recipeTasks.entries()]
        .filter(([priorLevel]) => priorLevel < level)
        .flatMap(([, task]) => task.selection.scoredChecks.map(check => check.stableKey)))] : []),
    ...(args.media && observation === 'scored' ? [] : ['--no-media']),
    ...(!executeStackCapability(STACK_ADAPTER_REGISTRY.get(args.backend),
      'run-policy', 'reset-enabled')
      ? ['--no-reset']
      : ['--restart-spec', JSON.stringify(restartSpec)])];
}

function grade(args, appDir, url, label, level, track, parentAttemptId,
  options = {}) {
  const { out = null } = options;
  const source = hashAppSource(appDir);
  const argv = gradeArgv(args, appDir, url, label, level, track, parentAttemptId, {
    ...options, sourceSha256: options.sourceSha256 ?? source.sha256,
  });
  const bundle = join(out ?? join(appDir, 'stack-bench'), 'bundle.json');
  rmSync(bundle, { force: true });
  const task = args.recipeTasks?.get(level);
  const currentChecks = options.observation === 'observed'
    ? task?.selection?.observedChecks
    : task?.selection?.scoredChecks ?? task?.selection?.checks;
  const regressionChecks = options.observation === 'observed' || args.progression
    ? []
    : [...(args.recipeTasks?.entries() ?? [])]
      .filter(([priorLevel]) => priorLevel < level)
      .flatMap(([, priorTask]) => priorTask.selection?.scoredChecks ?? priorTask.selection?.checks ?? []);
  const sourceCount = task
    ? selectedGradingSourceCount(currentChecks, regressionChecks)
    : suitesFor(track, level).length;
  try {
    sh('node', argv, { stdio: 'inherit', timeout: gradingRunTimeoutMs(sourceCount) });
  } catch { /* a current bundle may still explain a scored failure */ }
  return existsSync(bundle) ? readArtifactPayload(bundle, { expectedKind: 'grade_bundle' }) : null;
}

function restartSpecFor(args, appDir, track) {
  const port = portsFor(track, args.backend, args.runIndex).express ?? null;
  return { backend: args.backend, app: appDir, port: port == null ? null : Number(port),
    probe: track.restartProbe };
}

export function pristineMutationBaselinePath(args, exists = existsSync) {
  if (args.referenceMutationOnly) return args.mutationBaselineBundle ?? null;
  if (args.mutationBaselineBundle) return args.mutationBaselineBundle;
  const level = args.levelList?.at(-1);
  if (!Number.isSafeInteger(level) || level < 1 || !args.out) return null;
  const candidate = join(args.out, `first-build-l${level}-grading`, 'bundle.json');
  return exists(candidate) ? candidate : null;
}

function runMutationControl(args, appDir, url, track, imageId) {
  const output = join(args.out, 'mutation-control.json');
  if (!args.mutationResumeFrom || resolve(args.mutationResumeFrom) !== resolve(output)) {
    rmSync(output, { force: true });
  }
  args.mutationImageId = imageId ?? null;
  const manifest = JSON.parse(readFileSync(args.mutations, 'utf8'));
  const argv = mutationControlArgv(args, appDir, url, track);
  let processError = null;
  try { sh(process.execPath, argv, {
    stdio: 'inherit', timeout: mutationControlTimeoutMs(manifest,
      args.mutationMaxRuntimeMinutes),
  }); }
  catch (error) { processError = String(error.message).split('\n')[0]; }
  if (!existsSync(output)) {
    return { ok: false, artifact: output, processError,
      outcome: { kind: 'harness_failure', phase: 'mutation-control',
        reason: processError ?? 'mutation runner produced no artifact' } };
  }
  const artifact = readArtifactPayload(output, { expectedKind: 'mutation_control' });
  return { ok: artifact.ok === true && !processError, artifact: output,
    processError, summary: artifact.summary ?? null, outcome: artifact.outcome ?? null,
    results: artifact.results ?? [] };
}

function validateMutationInput(args) {
  if (!args.mutations) return;
  if (!args.app) throw new Error('--mutations requires an explicit pristine --app');
  const manifest = JSON.parse(readFileSync(args.mutations, 'utf8'));
  if (!/^[a-f0-9]{64}$/.test(manifest.fixtureSha256 ?? '')) {
    throw new Error('mutation manifest has no valid fixtureSha256');
  }
  const fixture = hashDirectory(args.app);
  if (fixture.sha256 !== manifest.fixtureSha256) {
    throw new Error(`mutation manifest targets fixture ${manifest.fixtureSha256}, not ${fixture.sha256}`);
  }
}

export function validateProgressionCampaignLevelScope(binding, progression, declared, level) {
  if (!declared) throw new Error(`study condition does not bind requested L${level}`);
  const derived = resolveProgressionRecipeLevelSelection(binding, progression, level);
  if (declared.recipe.contentSha256 !== derived.grader.request.recipe.contentSha256
    || declared.selection.sha256 !== derived.grader.selection.sha256
    || declared.task.sha256 !== derived.grader.task.sha256) {
    throw new Error(`dependency campaign graph-derived scope changed before L${level}`);
  }
  return derived;
}

async function main() {
  const args = parseArgs(process.argv);
  let repairGrant = null;
  if (args.repairFrom) {
    repairGrant = createRepairGrant(args.repairFrom,
      { level: args.repairLevel, rounds: args.fixRounds });
    const config = repairGrant.configuration;
    if (config.buildImage && process.env.STACK_BENCH_IMAGE
      && config.buildImage !== process.env.STACK_BENCH_IMAGE) {
      throw new Error('repair continuation build image differs from its parent run');
    }
    if (config.buildImage) process.env.STACK_BENCH_IMAGE = config.buildImage;
    Object.assign(args, {
      backend: config.backend,
      track: config.track,
      recipe: config.recipe,
      levels: String(config.level),
      levelList: [config.level],
      runIndex: config.runIndex,
      agentAdapter: config.agentAdapter,
      model: config.model,
      guidance: config.guidance,
      guidanceDocument: config.guidanceDocument,
      condition: config.condition,
      selectionRequest: config.selectionRequest,
      skills: config.skills,
      packIds: [...(config.selectionRequest.packs ?? [])],
      checkKeys: [...(config.selectionRequest.checks ?? [])],
      featureIds: [],
      requestedSpecifications: [],
      expectedSpecifications: [],
      observedSpecifications: [],
      seedFrom: repairGrant.sourcePath,
      url: config.url,
      parentAttemptId: repairGrant.parent.id,
      repairGrant,
    });
  }
  const stackAdapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  const agentAdapter = AGENT_ADAPTER_REGISTRY.get(args.agentAdapter);
  if (repairGrant) {
    const currentAgent = agentAdapterIdentity(agentAdapter);
    const parentAgent = repairGrant.parentArtifact.identities.agentAdapter;
    if (currentAgent.id !== parentAgent?.id || currentAgent.version !== parentAgent?.version
      || currentAgent.sha256 !== parentAgent?.sha256) {
      throw new Error('repair continuation agent adapter differs from its parent run');
    }
    if (stackAdapter.id !== repairGrant.parentArtifact.identities.stackAdapter?.id
      || stackAdapter.version !== repairGrant.parentArtifact.identities.stackAdapter?.version) {
      throw new Error('repair continuation stack adapter differs from its parent run');
    }
  }
  resolveAgentCredential(args, agentAdapter);
  args.model ??= agentAdapter.defaultModel;
  if (args.pricing !== undefined) {
    args.pricing = validatePricingAuthority(args.pricing, { at: '--pricing-json' });
  } else if (args.maxBudgetUsd != null && agentAdapter.costLimit === 'native') {
    const rates = claudeRatesForModel(args.model);
    if (!rates) throw new Error(`no default pricing is recorded for model ${args.model}`);
    args.pricing = validatePricingAuthority({ unit: PRICING_UNIT, rates },
      { at: 'default pricing' });
  } else {
    args.pricing = null;
  }
  if (args.retainBackend
    && !executeStackCapability(stackAdapter, 'run-policy', 'retain-host-supported')) {
    throw new Error(`stack adapter ${args.backend} does not support --retain-backend`);
  }
  const stackRuntime = executeStackCapability(stackAdapter, 'orchestrator', 'config', {
    root: ROOT, env: process.env, helpers: { exists: existsSync },
  });
  Object.assign(process.env, stackRuntime.environment);
  process.env.STACK_BENCH_NODE_BIN = process.platform === 'win32' ? 'node.exe' : process.execPath;
  const track = loadTrack(args.track);
  // Resolve the requested scope for every level before probing the sandbox,
  // acquiring a backend lease or paying for a build. A pack that exists at L2
  // but not L1 is not a late grading surprise; it is an invalid run request.
  args.recipeTasks = new Map();
  args.recipeBindings = new Map();
  args.selectionRequest ??= { packs: [...args.packIds], checks: [...args.checkKeys] };
  for (const level of args.levelList) {
    const declared = args.condition?.requested?.levels?.find(entry => entry.level === level) ?? null;
    const modularSelection = args.selectionRequest.levels?.find(entry => entry.level === level) ?? null;
    if (declared?.selection?.schemaVersion === 3) {
      const expected = args.featureCatalog
        ? { level, recipe: `${declared.recipe.id}@${declared.recipe.version}` }
        : { level, recipe: `${declared.recipe.id}@${declared.recipe.version}`,
          features: declared.selection.requested.features,
          checks: declared.selection.requested.checks };
      if (canonicalDefinitionJson(modularSelection) !== canonicalDefinitionJson(expected)) {
        throw new Error(`campaign selection changed before L${level}`);
      }
    } else if (modularSelection) {
      throw new Error(`campaign selection declares modular L${level} without a modular condition`);
    }
    const declaredRecipe = declared
      ? `${declared.recipe.id}@${declared.recipe.version}` : null;
    const binding = resolveRecipeRelease(track, level, declaredRecipe ?? args.recipe);
    if (!binding && (args.packIds.length || args.checkKeys.length)) {
      throw new Error(`L${level} has no recipe release, so --pack/--check cannot be resolved`);
    }
    if (binding) {
      args.recipeBindings.set(level, binding);
      if (args.featureCatalog) {
        validateProgressionCampaignLevelScope(binding, args.featureCatalog, declared, level);
      }
      const requested = declared?.selection?.requested;
      const resolved = args.featureCatalog
        ? resolveProgressionRecipeLevelSelection(binding, args.featureCatalog, level,
          { cumulative: Boolean(args.progression) })
        : createBoundRecipeTaskRequest(binding, requested?.features
          ? { featureIds: requested.features,
              requestedSpecifications: requested.specifications?.requested,
              expectedSpecifications: requested.specifications?.expected,
              observedSpecifications: requested.specifications?.observed,
              checkKeys: requested.checks }
          : args);
      const grader = args.featureCatalog ? resolved.grader : resolved;
      if (args.condition && !declared) {
        throw new Error(`study condition does not bind requested L${level}`);
      }
      if (declared && (declared.recipe.contentSha256 !== grader.request.recipe.contentSha256
        || declared.selection.sha256 !== grader.request.selection.sha256
        || JSON.stringify(declared.selection.taskPacks) !== JSON.stringify(grader.request.selection.taskPacks)
        || declared.task.sha256 !== grader.request.task.sha256)) {
        throw new Error(`study condition requested scope changed before L${level}`);
      }
      args.recipeTasks.set(level, args.featureCatalog ? {
        request: grader.request,
        selection: grader.selection,
        task: grader.task,
        agentRequest: resolved.agent.request,
      } : { ...resolved, agentRequest: createAgentVisibleTaskRequest(binding, resolved) });
    }
  }
  if (args.progression) {
    const state = progressionEngine.initialize(args.progression.definition);
    const declared = args.condition?.requested?.levels
      ?.find(entry => entry.level === state.level) ?? null;
    const binding = resolveRecipeRelease(track, state.level,
      declared ? `${declared.recipe.id}@${declared.recipe.version}` : null);
    resolveProgressionRecipeAction(binding, state);
    if (!args.progressionOwner) {
      throw new Error('live dependency progression requires an exact compiled campaign attempt');
    }
  }
  if (repairGrant) {
    const expectedSelection = repairGrant.level.selection?.sha256 ?? null;
    const resolvedSelection = args.recipeTasks.get(repairGrant.level.level)?.request.selection.sha256 ?? null;
    if (resolvedSelection !== expectedSelection) {
      throw new Error('repair continuation test selection differs from its parent run');
    }
  }
  if (!args.selectionRequest.levels && (JSON.stringify(args.selectionRequest.packs) !== JSON.stringify(args.packIds)
    || JSON.stringify(args.selectionRequest.checks) !== JSON.stringify(args.checkKeys))) {
    throw new Error('campaign pack/check selection changed before execution');
  }
  // Caller-owned mutation inputs are pure request data. Reject them before
  // checking credentials, Docker, ports, or any other ambient runner state so
  // an invalid experiment can never be masked by an unrelated preflight error.
  validateMutationInput(args);
  assertNoPortCollisions();
  // The deterministic adapter/stack is the model-free unit loop. Real runs
  // prove the exact requested scope, engine, image, credentials, storage and
  // ports before the sandbox probe or any paid coding session begins.
  const admittedSmoke = args.campaignAdmission?.reusable === true
    ? { id: args.campaignAdmission.id, createdAt: args.campaignAdmission.createdAt }
    : null;
  const preflight = args.backend === 'stub' ? null : runPreflight({
    backends: [args.backend], track: args.track, levels: args.levels,
    levelList: args.levelList, runIndex: args.runIndex, agentAdapter: args.agentAdapter,
    recipe: args.recipe,
    requestedScopes: args.condition?.requested ? [args.condition.requested] : null,
    featureCatalog: args.featureCatalog ?? null,
    mode: args.runMode ?? null,
    agentSkills: args.skills ?? null,
    packIds: args.packIds, checkKeys: args.checkKeys, smoke: admittedSmoke === null,
    admittedSmoke,
    supervisorState: process.env.STACK_BENCH_SUPERVISOR_STATE ?? null,
    image: process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE,
    resultsDir: resolve(args.out ?? process.env.STACK_BENCH_RESULTS_DIR ?? join(ROOT, 'results')),
  }, { env: args.apiKey && agentAdapter.apiKeyEnvironmentVariable
    ? { ...process.env, [agentAdapter.apiKeyEnvironmentVariable]: '<provided-by-argument>' }
    : process.env });
  if (preflight && !preflight.ok) {
    const failures = preflight.checks.filter(check => check.status === 'fail');
    console.error('\nPREFLIGHT FAILED — no model session was started.');
    for (const failure of failures) {
      console.error(`  ${failure.id}: ${failure.summary}`);
      if (failure.remediation) console.error(`    fix: ${failure.remediation}`);
    }
    process.exit(2);
  }
  if (preflight) console.log(`  preflight  ... ${preflight.summary.passed} checks passed`
    + `${preflight.summary.warnings ? `, ${preflight.summary.warnings} warning(s)` : ''}`);
  const beyondValidatedLevels = args.levelList.filter(level => level > track.validatedThrough);
  if (beyondValidatedLevels.length) {
    console.log(`  NOTICE: ${track.name} is validated through L${track.validatedThrough}; `
      + `this run also requests L${beyondValidatedLevels.join(', L')}. The result will record those exact levels.`);
  }

  // In a single-host topology, prove the file-tool sandbox before model spend.
  // The appliance instead relies on structural isolation: the coding container
  // has no controller, grader, scenarios, prior results, or Docker socket.
  // The stub backend is the offline test loop: no model, no cost, nothing to
  // protect. Spending a real CLI session probing it would make the one test
  // that is supposed to run for free stop being free.
  const probeMode = sandboxProbeMode({ appliance: process.env.STACK_BENCH_APPLIANCE === '1',
    explicitlySkipped: args.skipProbe, stackRequired: executeStackCapability(stackAdapter,
      'run-policy', 'sandbox-probe-required') && agentAdapter.sandboxProbe === 'direct-cli' });
  if (probeMode === 'container-isolation') {
    console.log('  sandbox    ... coding container is isolated from the controller and grading files');
  } else if (probeMode === 'direct-cli') {
    console.log('  sandbox    ... probing the deny rules');
    try {
      sh('node', [stagedEntrypoint('commands', 'probe-sandbox.mjs'), '--mode', 'acceptEdits', '--model', args.model],
        { stdio: 'inherit' });
    } catch {
      console.error('\nSANDBOX PROBE FAILED — refusing to start a run whose scores could not be trusted.');
      console.error('Run `node commands/probe-sandbox.mjs --mode acceptEdits` to see which path got through.');
      process.exit(2);
    }
  }
  let url = args.url ?? `http://localhost:${portsFor(track, args.backend, args.runIndex).vite}`;
  const runDir = resultsName(track, args.backend, args.runIndex);
  const runId = newRunId({ track: args.track, backend: args.backend, runIndex: args.runIndex });
  const artifactLabel = `${runDir}-${runId}`;
  // Default results never reuse a directory. The stable backend/run name is a
  // grouping directory only; every artifact beneath it belongs to one run id.
  args.out ??= join(process.env.STACK_BENCH_RESULTS_DIR ?? join(ROOT, 'results'), runDir, runId);
  mkdirSync(args.out, { recursive: true });
  if (existsSync(join(args.out, 'run.json'))) {
    throw new Error(`refusing to reuse result directory containing run.json: ${args.out}`);
  }
  if (preflight) writeArtifact(join(args.out, 'preflight.json'), {
    kind: 'preflight', id: `${runId}-preflight`,
    attempt: { id: `${runId}-preflight`, parentId: runId },
    identities: emptyArtifactIdentities({
      agentAdapter: agentAdapterIdentity(agentAdapter),
      stackAdapter: { id: stackAdapter.id, version: stackAdapter.version },
    }),
    payload: preflight,
  });

  // Validate caller-owned source before acquiring a backend slot so a bad
  // fixture cannot leave leased resources behind.
  const ownWorkDir = !args.app;
  const appDir = args.app ?? join(workDirFor(track, args.backend, args.runIndex, runId), 'app');
  if (args.repairGrant && url.startsWith('file:')) {
    url = pathToFileURL(join(appDir, 'index.html')).href;
  }

  // Bind destructive and lifecycle operations to exact resource identities and
  // an ownership token. Targets come only from the lease, never generated code.
  const runtimeRoot = resolve(process.env.STACK_BENCH_RUNTIME_DIR
    ?? join(tmpdir(), 'stack-bench-runtime'));
  const runtimeDir = join(runtimeRoot, runId);
  const leasePath = join(runtimeDir, 'backend-lease.json');
  const preparedLease = executeStackCapability(stackAdapter, 'lease', 'prepare', {
    track,
    runIndex: args.runIndex,
    runtimeDir,
    serverUri: stackRuntime.lease.serverUri,
    env: process.env,
    helpers: { containerIdentity, dbName, moduleName },
  });
  const initialLease = createBackendLease({
    runId,
    backend: args.backend,
    track: args.track,
    runIndex: args.runIndex,
    ...preparedLease.lease,
  });
  const lockScope = resourceLockScope();
  const lockKeys = backendResourceLockKeys(initialLease, preparedLease.lockKeys);
  let privateSupervisorStatePath = null;
  try {
    initialLease.resources.locks.push(...acquireResourceLocks({
      ...lockScope, keys: lockKeys, lease: initialLease,
    }));
    writeBackendLease(leasePath, initialLease);
    const supervisorState = process.env.STACK_BENCH_SUPERVISOR_STATE
      ?? (process.env.STACK_BENCH_SUPERVISOR_DIR
        ? join(resolve(process.env.STACK_BENCH_SUPERVISOR_DIR), `${runId}.json`) : null);
    if (supervisorState) {
      // Private handoff to an outer timeout supervisor. It contains the lease
      // token, so create it once with owner-only permissions and never place it
      // in the results tree.
      privateSupervisorStatePath = resolve(supervisorState);
      mkdirSync(dirname(privateSupervisorStatePath), { recursive: true, mode: 0o700 });
      writeFileSync(privateSupervisorStatePath, `${JSON.stringify({
        version: SUPERVISOR_STATE_VERSION, runId, backend: args.backend, runtimeDir, leasePath,
        ownershipToken: initialLease.ownershipToken, output: resolve(args.out),
      })}\n`, { flag: 'wx', mode: 0o600 });
    }
  } catch (error) {
    releaseResourceLocks(initialLease);
    rmSync(runtimeDir, { recursive: true, force: true });
    throw error;
  }
  process.env.STACK_BENCH_LEASE = leasePath;
  process.env.STACK_BENCH_LEASE_TOKEN = initialLease.ownershipToken;
  if (process.platform === 'win32') {
    // When Windows resolves `bash` through WSL, WSLENV must carry lease paths
    // and tokens into lifecycle scripts with path translation.
    const bridge = ['STACK_BENCH_LEASE/p', 'STACK_BENCH_LEASE_TOKEN',
      'STACK_BENCH_NODE_BIN', ...stackRuntime.windowsEnvironmentBridge];
    const existing = (process.env.WSLENV ?? '').split(':').filter(Boolean);
    process.env.WSLENV = [...new Set([...existing, ...bridge])].join(':');
  }

  let tornDown = false;
  let activeRun = null;
  const recoveryPath = join(args.out, 'recovery.json');
  const writeLeaseEvidence = (knownLease = null) => {
    const lease = knownLease ?? readBackendLease(leasePath,
      { token: initialLease.ownershipToken, backend: args.backend, runId });
    const out = join(args.out, 'backend-lease.json');
    const evidence = publicBackendLease(lease);
    const id = `${runId}-backend-lease`;
    writeArtifact(out, {
      kind: 'backend_lease_evidence', id,
      attempt: { id, parentId: runId },
      timestamps: { startedAt: evidence.createdAt, completedAt: new Date().toISOString() },
      identities: emptyArtifactIdentities({ stackAdapter: { id: args.backend } }),
      payload: evidence,
    });
    return evidence;
  };
  const teardown = ({ reason = null, retainBackend = args.retainBackend } = {}) => {
    if (tornDown) return;
    if (activeAgentChild?.pid) {
      killTree(activeAgentChild.pid);
      activeAgentChild = null;
    }
    // Preserve restart failures before removing the only filesystem that holds
    // their stderr. A 500 after restart is otherwise impossible to distinguish
    // from an application defect, a dead dependency, or host pressure.
    if (activeRun) {
      try {
        activeRun.backendDiagnostics = captureBackendDiagnostics(join(args.out, 'backend.log'));
      } catch (error) {
        activeRun.backendDiagnostics = { captured: false,
          reason: String(error.message).split(/\r?\n/)[0] };
      }
    }
    let released = false;
    let cleanupError = null;
    try {
      released = releaseBackendLease(leasePath, initialLease.ownershipToken,
        { retainBackend });
    } catch (error) { cleanupError = error; }
    let finalLease = initialLease;
    try {
      finalLease = readBackendLease(leasePath,
        { token: initialLease.ownershipToken, backend: args.backend, runId });
    } catch (error) { cleanupError ??= error; released = false; }
    const evidence = writeLeaseEvidence(finalLease);
    writeRecoveryArtifact(recoveryPath, finalLease, { cleanupSucceeded: released,
      retained: Boolean(retainBackend),
      reason: cleanupError?.message ?? reason ?? (released ? null : 'authenticated cleanup refused') });
    if (activeRun) {
      activeRun.backendLease = evidence;
      activeRun.outcome ??= aggregateRunOutcome(activeRun.levels);
      writeRunJson(join(args.out, 'run.json'), activeRun);
    }
    tornDown = released;
    if (released && !retainBackend) {
      rmSync(runtimeDir, { recursive: true, force: true });
      if (privateSupervisorStatePath) rmSync(privateSupervisorStatePath, { force: true });
    }
    if (cleanupError) throw cleanupError;
    if (!released) throw new Error(`backend teardown refused: listener no longer matches lease ${runId}`);
  };
  emergencyTeardown = teardown;

  try {
    executeStackCapability(stackAdapter, 'lifecycle', 'activate', {
      leasePath, leaseToken: initialLease.ownershipToken, lease: initialLease,
      ...stackRuntime.lifecycle,
    });
  } catch (error) {
    try { teardown({ reason: `backend activation failed: ${error.message}`, retainBackend: false }); }
    catch (cleanupError) {
      console.error(`  activation cleanup quarantined: ${String(cleanupError.message).split(/\r?\n/)[0]}`);
    }
    throw error;
  }

  // Grow one isolated app across levels, outside the harness and results tree.
  // Copy artifacts back at completion and remove only a work directory created
  // by this run; an explicit --app remains caller-owned.
  // Leave nothing running once the run is over, however it ends — but only stop
  // what this run brought up.
  // This run's work path is unique. There is no legitimate pre-existing build
  // container to delete; teardown removes one only after run-build records its
  // immutable id in the lease.
  const interrupt = (signal, exitCode) => {
    console.log(`interrupted by ${signal} — stopping exact owned resources`);
    try { teardown({ reason: `interrupted by ${signal}` }); }
    catch (error) { console.error(`  cleanup quarantined: ${String(error.message).split(/\r?\n/)[0]}`); }
    process.exit(exitCode);
  };
  process.on('SIGINT', () => interrupt('SIGINT', 130));
  process.on('SIGTERM', () => interrupt('SIGTERM', 143));
  process.on('exit', () => {
    if (!tornDown) {
      try { teardown(); } catch (error) {
        console.error(`  cleanup failed: ${String(error.message).split('\n')[0]}`);
      }
    }
  });

  // Seed the work dir from an existing app, so the first level upgrades it
  // rather than building from nothing. Source only; the upgrade session
  // installs its own dependencies exactly as a developer checking out the
  // earlier code would. The copy is layout-independent for neutral runs.
  if (args.seedFrom) {
    const from = resolve(args.seedFrom);
    if (!existsSync(from)) { console.error(`--seed-from path does not exist: ${from}`); process.exit(2); }
    seedAppSource(from, appDir);
    console.log(args.repairGrant
      ? `  restored L${args.levelList[0]} checkpoint from ${from} for a bounded repair continuation`
      : `  seeded from ${from} — level ${args.levelList[0]} will UPGRADE it, not rebuild`);
  }

  const started = Date.now();
  const run = { id: runId,
    ...(args.repairGrant ? { kind: 'repair_continuation',
      continuation: structuredClone(args.repairGrant.grant) } : {}),
    startedAt: new Date(started).toISOString(),
    parentAttemptId: args.parentAttemptId ?? null,
    identities: emptyArtifactIdentities({
      experiment: args.experimentIdentity ?? null,
      agentAdapter: agentAdapterIdentity(agentAdapter),
      stackAdapter: { id: stackAdapter.id, version: stackAdapter.version },
    }),
    mode: args.runMode ?? { id: args.progression ? 'dependency' : 'sequential',
      version: args.progression ? '2.1.0' : '1.0.0' },
    track: args.track, backend: args.backend, model: args.model,
    pricing: args.pricing,
    guidance: args.guidance, condition: args.condition ?? null,
    stack: args.guidance === 'minimal' ? 'free' : args.guidance,
    skills: args.skills ?? [],
    runtime: { buildImage: process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE, url },
    selectionRequest: args.selectionRequest,
    featureCatalog: args.featureCatalog?.identity ?? null,
    dependencyPolicy: args.dependencyPolicy?.identity ?? null,
    ...(args.progressionOwner ? { progressionOwner: args.progressionOwner } : {}),
    backendLease: publicBackendLease(readBackendLease(leasePath,
      { token: initialLease.ownershipToken, backend: args.backend, runId })),
    validation: { validatedThrough: track.validatedThrough, beyondValidatedLevels,
      ladder: { policy: args.progression ? args.progression.identity.policy : 'pass-before-next-level',
        requestedLevels: [...args.levelList],
        completedLevels: [], stoppedAfterLevel: null, blockedLevels: [] } }, levels: [] };
  activeRun = run;

  const progressionOwner = args.progression ? {
    ...args.progressionOwner,
    workspace: { appDirectory: 'source' },
  } : null;
  const progressionExecution = args.progression ? createLiveProgressionExecution({
      progression: args.progression,
      featureCatalogIdentity: args.featureCatalog?.identity,
      dependencyPolicyIdentity: args.dependencyPolicy?.identity,
      owner: progressionOwner,
      statePath: join(args.out, 'progression-state.json'),
      runId,
      outputDir: args.out,
      appDir,
      track: args.track,
      backend: args.backend,
      identities: run.identities,
      recipeBindings: args.recipeBindings,
      resumeFrom: args.progressionResumeFrom ?? null,
      getRunArtifact: () => {
        writeRunJson(join(args.out, 'run.json'), run);
        return readArtifact(join(args.out, 'run.json'));
      },
      onState: status => {
        run.progressionStatus = status;
        writeRunJson(join(args.out, 'run.json'), run);
      },
    }) : null;
  const progressionStart = progressionExecution?.initialize() ?? null;
  if (progressionStart?.resumed) {
    const prior = progressionStart.priorRun;
    const actionLevel = progressionStart.action.type === 'terminal'
      ? Number.MAX_SAFE_INTEGER : progressionStart.action.level;
    const inheritedLevels = (prior.payload.levels ?? [])
      .filter(level => level.level < actionLevel).map(level => level.level);
    run.levels = (prior.payload.levels ?? [])
      .filter(level => inheritedLevels.includes(level.level)).map(level => structuredClone(level));
    run.validation.ladder.completedLevels = [...inheritedLevels];
    run.progressionResume = {
      priorRunId: prior.id,
      priorRunSha256: sha256(canonicalDefinitionJson(prior)),
      stateSnapshotSha256: progressionStart.snapshotSha256,
      action: progressionStart.action.type === 'terminal'
        ? { type: 'terminal' }
        : { type: progressionStart.action.type, level: progressionStart.action.level },
      inheritedLevels,
      priorTotals: prior.payload.totals ?? null,
    };
    run.progressionStatus = progressionStart.status;
    writeRunJson(join(args.out, 'run.json'), run);
  }

  const bindProgressionAction = level => {
    if (!progressionExecution) return null;
    const selected = progressionExecution.bind(level);
    if (selected.action.type === 'terminal') return selected;
    args.recipeTasks.set(level, {
      request: selected.grader.request,
      selection: selected.grader.selection,
      task: selected.grader.task,
      agentRequest: selected.agent.request,
      progressionAction: selected.action,
    });
    return selected;
  };

  const recordProgressionGrade = input => progressionExecution?.record(input) ?? null;

  let runCostComplete = true;

  const runAgentForLevel = async (mode, level) => {
    try {
      const result = await runAgent(args, agentAdapter, mode, level, appDir);
      if (result.costComplete !== true) runCostComplete = false;
      return result;
    } catch (error) {
      const reason = String(error?.message ?? error).split(/\r?\n/)[0];
      run.outcome = { kind: 'harness_failure', phase: `agent-${mode}`,
        reason, appFailures: [], inconclusive: [], harnessFailures: [reason] };
      run.validation.ladder.stoppedAfterLevel = run.levels.at(-1)?.level ?? null;
      run.validation.ladder.blockedLevels = args.levelList.filter(candidate => candidate >= level);
      if (progressionExecution) {
        run.progressionStatus = progressionExecution.status();
        run.validation.ladder.completedLevels = [...new Set(progressionExecution.state.attempts
          .filter(attempt => attempt.outcome === 'conclusive')
          .map(attempt => attempt.level))];
      }
      finalizeRunTotals(run, started, { costComplete: false });
      run.completedAt = new Date().toISOString();
      writeRunJson(join(args.out, 'run.json'), run);
      throw error;
    }
  };

  // Stop before grading if a coding session read protected material or if the
  // audit itself failed. Keep the paid session and exact cost in run.json even
  // though no score may be used.
  const abortUnusableSession = (whichSession, audit, levelRecord, selected) => {
    const reason = audit.evidence.join('; ');
    const outcome = { kind: audit.kind === 'harness_failure' ? 'harness_failure' : 'ungraded',
      phase: 'contamination-audit', reason,
      appFailures: [], inconclusive: [],
      harnessFailures: audit.kind === 'harness_failure' ? [reason] : [] };
    run.contaminated = audit.kind === 'contaminated';
    run.contamination = { evidence: audit.evidence, verdict: audit.verdict,
      detectedAt: whichSession };
    run.levels.push({ ...levelRecord, error: reason, outcome });
    if (progressionExecution) {
      recordProgressionGrade({ selected, bundle: null, level: levelRecord.level,
        failure: outcome,
        repair: { status: 'ungraded', budgetRounds: 0, roundsUsed: 0,
          stopReason: audit.kind === 'harness_failure' ? 'audit-failure' : 'contaminated' } });
      run.progressionStatus = progressionExecution.status();
    }
    run.validation.ladder.stoppedAfterLevel = run.levels.at(-2)?.level ?? null;
    run.validation.ladder.blockedLevels = args.levelList
      .filter(candidate => candidate >= levelRecord.level);
    finalizeRunTotals(run, started, { costComplete: runCostComplete });
    run.outcome = outcome;
    run.completedAt = new Date().toISOString();
    if (run.contaminated) {
      console.log(`\n  !! CONTAMINATED at ${whichSession}:`);
      for (const evidence of audit.evidence) console.log(`     ${evidence}`);
      console.log('     Scores from this run must not be quoted.');
    } else {
      console.log(`\n  !! HARNESS FAILURE at ${whichSession}:`);
      for (const evidence of audit.evidence) console.log(`     ${evidence}`);
      console.log('     The audit did not establish a usable result.');
    }
    try { writeRunJson(join(args.out, 'run.json'), run); } catch { /* best effort */ }
    try { sh('node', [join(ROOT, 'dist', 'commands', 'archive-transcripts.js'), '--app', appDir, '--label', artifactLabel], { stdio: 'pipe' }); } catch { /* best effort */ }
    teardown();
    process.exit(4);
  };

  for (const level of args.levelList) {
    if (progressionExecution && level < progressionExecution.state.level) continue;
    const t0 = Date.now();
    const continuing = Boolean(args.repairGrant);
    console.log(`\n================ ${args.backend} — level ${level} ================`);

    let progressionSelection = bindProgressionAction(level);
    if (progressionSelection?.action.type === 'terminal') break;
    const conclusiveProgressionAttempts = () => progressionExecution
      ? progressionExecution.state.attempts.filter(attempt =>
        attempt.level === level && attempt.outcome === 'conclusive').length
      : 0;
    const repairBudgetFor = selected => selected
      ? dependencyRepairBudget(selected.action, conclusiveProgressionAttempts())
      : args.fixRounds;
    const levelStrikeNodeIds = new Set(progressionSelection?.action.strikes.nodes
      .map(node => node.nodeId) ?? []);
    let progressionRepairBudgetRounds = repairBudgetFor(progressionSelection);
    const trackProgressionBudget = selected => {
      if (!selected) return;
      selected.action.strikes.nodes.forEach(node => levelStrikeNodeIds.add(node.nodeId));
      progressionRepairBudgetRounds = Math.max(
        progressionRepairBudgetRounds, repairBudgetFor(selected));
    };
    const resumedRepair = progressionStart?.resumed === true
      && progressionStart.action.type === 'repair'
      && progressionStart.action.level === level;
    const priorRepairRounds = resumedRepair
      ? Math.max(0, conclusiveProgressionAttempts() - 1) : 0;
    if (resumedRepair) {
      sh('node', [join(ROOT, 'dist', 'commands', 'report-bugs.js'), '--app', appDir,
        '--history-json', '[]', '--archive', join(args.out, 'repair-reports',
          `bug-report-l${level}-resume.md`)], { stdio: 'pipe' });
      clearPrivateGradingEvidence(appDir);
    }

    const firstMode = resumedRepair ? 'fix'
      : continuing ? 'resume' : args.seedFrom ? 'upgrade' : 'build';
    const build = await runAgentForLevel(
      resumedRepair || level === args.levelList[0] ? firstMode : 'upgrade', level, appDir);
    const buildLeak = auditContamination(appDir);
    if (buildLeak) {
      const buildSession = runSessionRecord(build,
        resumedRepair ? priorRepairRounds + 1 : null);
      const sessionTotals = summarizeSessions([buildSession]);
      abortUnusableSession(`level ${level} ${firstMode}`, buildLeak, {
        level, graded: false, score: null, max: null, selection: null,
        ...(resumedRepair
          ? { fixCostUsd: build.costUsd, fixSessions: [buildSession], fixRounds: 1,
            priorRepairRounds, cumulativeFixRounds: priorRepairRounds + 1 }
          : continuing
          ? { resumeCostUsd: build.costUsd, resumeSession: buildSession }
          : { buildCostUsd: build.costUsd, buildSession }),
        sessionTotals, costUsd: build.costUsd, durationMs: Date.now() - t0,
      }, progressionSelection);
    }
    // Carry the agent's own record of the setup up to the run. Comparing two
    // scores is only meaningful if the reasoning budget, permission mode and
    // CLI version behind them were the same, and that is not knowable after the
    // fact unless it was written down at the time.
    run.setup ??= build.setup;
    if (continuing) {
      run.continuation.resumeSetup = {
        sessionId: build.sessionId ?? null,
        costUsd: build.costUsd,
        durationMs: build.durationMs,
        sourceVerified: false,
      };
    }
    // No session, no app. Grading an empty directory yields a real-looking zero
    // that is a harness failure, not a result for this backend.
    const buildFailure = agentSessionFailure(build);
    if (buildFailure) {
      console.log(`  ABORTED: ${buildFailure.reason}. Details will be kept in ${join(args.out, 'run.json')}`);
      const failedSession = runSessionRecord(build);
      if (progressionExecution) {
        recordProgressionGrade({ selected: progressionSelection, bundle: null, level,
          failure: buildFailure,
          repair: { status: 'ungraded', budgetRounds: 0, roundsUsed: 0,
            stopReason: 'agent-session-failure' } });
      }
      run.levels.push({ level, graded: false, score: null, max: null,
        selection: null, error: buildFailure.reason,
        outcome: buildFailure,
        ...(continuing
          ? { resumeSession: failedSession, resumeCostUsd: build.costUsd }
          : { buildSession: failedSession, buildCostUsd: build.costUsd }),
        sessionTotals: summarizeSessions([build]),
        costUsd: build.costUsd, durationMs: Date.now() - t0 });
      break;
    }
    if (continuing) {
      // The resume session may install dependencies and start arbitrary project
      // layouts, but it may not perform an unintended correction. Restoring edited source
      // is insufficient: a running server could still hold code compiled from
      // those edits. Reject any source mutation and grade only an unchanged
      // checkpoint runtime.
      const resumed = hashAppSource(appDir);
      if (resumed.sha256 !== args.repairGrant.checkpoint.payload.source.sha256
        || resumed.files.length !== args.repairGrant.checkpoint.payload.source.files) {
        throw new Error('resume setup changed the parent checkpoint source');
      }
      run.continuation.resumeSetup.sourceVerified = true;
    }
    if (args.referenceMutationOnly) {
      run.levels.push({ level, score: null, max: null, graded: false, contractPass: null,
        outcome: { kind: 'ungraded', phase: 'reference-mutation-only',
          reason: 'the parent qualification owns the full clean grade',
          appFailures: [], inconclusive: [], harnessFailures: [] },
        buildSession: runSessionRecord(build),
        buildCostUsd: build.costUsd, sessionTotals: summarizeSessions([build]),
        costUsd: build.costUsd, durationMs: Date.now() - t0 });
      break;
    }
    const firstBuildDirectory = continuing ? `baseline-l${level}` : `first-build-l${level}`;
    const firstBuildPath = join(args.out, firstBuildDirectory);
    let firstBuildSource = null;
    try {
      const liveSource = hashAppSource(appDir);
      snapshotSource(appDir, firstBuildPath);
      const preservedSource = hashDirectory(firstBuildPath);
      if (liveSource.sha256 !== preservedSource.sha256) {
        throw new Error('preserved first-build source differs from the live application source');
      }
      firstBuildSource = { sha256: liveSource.sha256, files: liveSource.files.length };
      console.log(`  kept the ${continuing ? 'continuation baseline' : 'unaided'} source at ${firstBuildPath}`);
    } catch (error) {
      console.log(`  !! could not bind the first-build source: ${String(error.message).split('\n')[0]}`);
    }
    let bundle = grade(args, appDir, url, `${args.backend}-l${level}`, level, track, runId);

    // What the model built BEFORE being handed the answers. Every backend can
    // reach the same total given enough fix rounds, so the post-fix score stops
    // discriminating — what it got right unaided is the comparison that survives.
    const firstBuild = {
      score: bundle?.totals?.score ?? null,
      max: bundle?.totals?.max ?? null,
      regression: bundle?.totals?.regression ?? null,
      contractPass: bundle?.totals?.contractPass ?? null,
      outcome: sourceBoundFirstBuildOutcome(bundle, firstBuildSource),
      source: firstBuildSource,
      missed: Object.values(bundle?.suites ?? {}).flatMap(s =>
        (s?.features ?? []).flatMap(f =>
          (f.criteria ?? []).filter(c => !evidencePassed(criterionEvidence(c)))
            .map(c => `${f.name}/${c.id}`))),
    };

    if (continuing) {
      const reproduction = compareRepairBaseline(args.repairGrant.level, {
        score: firstBuild.score,
        max: firstBuild.max,
        selectionSha256: bundle?.selection?.sha256 ?? null,
        sourceSha256: firstBuildSource?.sha256 ?? null,
        expectedSourceSha256: args.repairGrant.checkpoint.payload.source.sha256,
        outcome: firstBuild.outcome,
      });
      run.continuation.baseline = {
        score: firstBuild.score,
        max: firstBuild.max,
        selectionSha256: bundle?.selection?.sha256 ?? null,
        sourceSha256: firstBuildSource?.sha256 ?? null,
        outcome: firstBuild.outcome,
        ...reproduction,
      };
      if (!reproduction.reproduced) {
        const reason = `restored checkpoint did not reproduce its parent: ${reproduction.mismatches.join(', ')}`;
        console.log(`  CONTINUATION STOPPED: ${reason}`);
        const failure = { kind: 'harness_failure', phase: 'continuation-baseline', reason,
          appFailures: [], inconclusive: [], harnessFailures: [] };
        firstBuild.outcome = failure;
        bundle = { ...bundle, outcome: failure };
      }
    }

    const selectedObservedChecks = args.recipeTasks?.get(level)?.selection?.observedChecks ?? [];
    if (!continuing && !resumedRepair && selectedObservedChecks.length) {
      const observationOut = join(args.out, `first-build-l${level}-observed`);
      let observationBundle = null;
      let observationOutcome;
      if (!firstBuildSource) {
        observationOutcome = { kind: 'harness_failure', phase: 'first-build-source',
          reason: 'observed specifications require a source-bound first build' };
      } else if (!ladderMayContinue(firstBuild.outcome)) {
        observationOutcome = { kind: 'ungraded', phase: 'first-build-observation',
          reason: 'scored first-build grading did not establish a usable environment' };
      } else {
        observationBundle = grade(args, appDir, url, `${args.backend}-l${level}-observed`, level,
          track, runId, { observation: 'observed', out: observationOut,
            sourceSha256: firstBuildSource.sha256 });
        observationOutcome = classifyBundle(observationBundle);
      }
      firstBuild.observations = {
        sourceSha256: firstBuildSource?.sha256 ?? null,
        selectionSha256: args.recipeTasks.get(level).selection.sha256,
        selectedChecks: selectedObservedChecks.map(check => check.stableKey),
        reportedChecks: observationBundle?.selection?.reportedChecks ?? [],
        passedPoints: observationBundle?.totals?.score ?? null,
        observedPoints: observationBundle?.totals?.max ?? null,
        scoreContribution: false,
        repairVisible: false,
        artifact: observationBundle ? `first-build-l${level}-observed/bundle.json` : null,
        outcome: observationOutcome,
      };
    }

    // Preserve the first source and scored grading before repair overwrites the
    // app. Observed evidence remains in its own source-bound result directory.
    try {
      const gradingFrom = join(appDir, 'stack-bench');
      if (existsSync(gradingFrom)) {
        const gradingDirectory = continuing ? `baseline-l${level}-grading` : `first-build-l${level}-grading`;
        cpSync(gradingFrom, join(args.out, gradingDirectory), {
          recursive: true,
          filter: src => !/[\\/]media([\\/]|$)/.test(src),
        });
        console.log(`  kept the ${continuing ? 'continuation baseline' : 'unaided'} grading at ${join(args.out, gradingDirectory)}`);
      }
    } catch (e) {
      // Never worth losing a run over: the score is already recorded.
      console.log(`  !! could not keep the first build: ${String(e.message).split('\n')[0]}`);
    }

    let fixRounds = resumedRepair ? 1 : 0;
    let fixCost = resumedRepair ? build.costUsd : 0;
    const fixSessions = resumedRepair
      ? [runSessionRecord(build, priorRepairRounds + 1)] : [];
    const repairHistory = [];
    let regressed = false;
    let repairStopReason = null;
    let repairProgress = repairProgressState(null, bundle);
    const pauseForRepeatedFindings = () => {
      if (args.progression) return false;
      repairProgress = repairProgressState(repairProgress, bundle);
      if (args.maxStalledRepairs === 0
        || repairProgress.stalledRounds < args.maxStalledRepairs) return false;
      repairStopReason = 'repeated-findings';
      console.log(`    pausing after ${repairProgress.stalledRounds} repair rounds `
        + 'with the same failed checks and no score gain');
      return true;
    };
    const initialBundleOutcome = classifyBundle(bundle);
    const initialGradeUsable = ladderMayContinue(initialBundleOutcome);
    if (!initialGradeUsable) {
      repairStopReason = 'initial-grading-failed';
      console.log('  repairs skipped: the initial grade did not complete, so there are no reliable findings to fix');
    }

    let progressionNext = recordProgressionGrade({
      selected: progressionSelection,
      bundle,
      level,
      repair: {
        status: !initialGradeUsable ? 'ungraded'
          : resumedRepair ? (initialBundleOutcome.kind === 'passed' ? 'corrected' : 'incomplete')
          : initialBundleOutcome.kind === 'passed' ? 'not-needed' : 'incomplete',
        budgetRounds: progressionSelection
          ? progressionRepairBudgetRounds : args.fixRounds,
        roundsUsed: priorRepairRounds + (resumedRepair ? 1 : 0),
        ...(!args.progression ? { stallLimitRounds: args.maxStalledRepairs } : {}),
        stopReason: !initialGradeUsable ? 'initial-grading-failed'
          : initialBundleOutcome.kind === 'passed' ? 'not-needed' : null,
      },
    });
    const progressionMayRepair = () => !args.progression
      || (progressionNext?.type === 'repair' && progressionNext.level === level);
    const recordRepairProgression = ({ failure = null } = {}) => {
      progressionNext = recordProgressionGrade({
        selected: progressionSelection,
        bundle: failure ? null : bundle,
        level,
        failure,
        repair: {
          status: failure ? 'ungraded'
            : classifyBundle(bundle).kind === 'passed' ? 'corrected' : 'incomplete',
          budgetRounds: progressionSelection
            ? progressionRepairBudgetRounds : args.fixRounds,
          roundsUsed: priorRepairRounds + fixRounds,
          ...(!args.progression ? { stallLimitRounds: args.maxStalledRepairs } : {}),
          stopReason: null,
        },
      });
      return progressionMayRepair();
    };

    // Hand back findings and let the agent fix, until clean or out of rounds.
    while (ladderMayContinue(classifyBundle(bundle)) && progressionMayRepair()
      && (args.progression || fixRounds < args.fixRounds)) {
      let wroteReport = true;
      try {
        sh('node', [join(ROOT, 'dist', 'commands', 'report-bugs.js'), '--app', appDir,
          '--history-json', JSON.stringify(repairHistory),
          '--archive', join(args.out, 'repair-reports',
            `bug-report-l${level}-round${fixRounds + 1}.md`)], { stdio: 'pipe' });
      } catch (err) {
        if (err.status === 3) wroteReport = false;      // nothing failed
        else if (err.status === 4) {
          wroteReport = false;
          repairStopReason = 'no-actionable-findings';
        }
        else throw err;
      }
      if (!wroteReport) {
        repairStopReason = 'no-actionable-findings';
        break;
      }

      const before = bundle?.totals?.score ?? 0;
      const beforeMax = bundle?.totals?.max ?? 0;
      // Kept whole, not just its total: the regression check compares
      // per-criterion, because totals are scored out of a denominator that
      // moves between rounds.
      const beforeBundle = bundle;
      // A fix can break more than it mends. Keep the source that produced the
      // best score so far, and roll back to it if a round regresses.
      // Kept outside the results tree: a snapshot is a known-good copy of the
      // answer, and a coding session that can reach one will copy it instead of
      // building. It only has to survive this process.
      const snapshot = join(tmpdir(), `stack-bench-snapshot-${args.backend}-${args.track}-run${args.runIndex}-l${level}`);
      snapshotSource(appDir, snapshot);
      clearPrivateGradingEvidence(appDir);
      fixRounds += 1;
      if (args.progression) {
        progressionSelection = bindProgressionAction(level);
        trackProgressionBudget(progressionSelection);
      }
      const displayedRepairBudget = args.progression
        ? progressionRepairBudgetRounds
        : args.fixRounds;
      console.log(`--- fix round ${fixRounds}/${displayedRepairBudget} ---`);
      const fix = await runAgentForLevel('fix', level);
      fixCost += fix.costUsd;
      fixSessions.push(runSessionRecord(fix, priorRepairRounds + fixRounds));

      const fixFailure = agentSessionFailure(fix);
      if (fixFailure) {
        console.log(`    coding session failed: ${fixFailure.reason}; stopping repairs`);
        bundle = { outcome: fixFailure };
        repairHistory.push(repairHistoryEntry(fixRounds, beforeBundle, bundle,
          'agent session failed'));
        repairStopReason = 'agent-session-failure';
        recordRepairProgression({ failure: fixFailure });
        break;
      }

      // Check the round that just ran, before paying to grade it. A fix session
      // that read the scenario file is not going to be redeemed by another
      // round, and grading it only produces a number nobody may quote.
      const fixLeak = auditContamination(appDir);
      if (fixLeak) {
        const buildSession = runSessionRecord(build);
        const sessions = resumedRepair ? fixSessions : [buildSession, ...fixSessions];
        const sessionTotals = summarizeSessions(sessions);
        abortUnusableSession(`fix round ${fixRounds}`, fixLeak, {
          level, graded: false, score: null, max: null,
          selection: bundle?.selection ?? null,
          ...(resumedRepair
            ? { resumedRepair: firstBuild }
            : continuing
            ? { baseline: firstBuild, resumeCostUsd: build.costUsd, resumeSession: buildSession }
            : { firstBuild, buildCostUsd: build.costUsd, buildSession }),
          fixCostUsd: addCostUsd(fixCost), fixSessions, fixRounds,
          ...(resumedRepair ? { priorRepairRounds,
            cumulativeFixRounds: priorRepairRounds + fixRounds } : {}),
          repair: { status: 'ungraded', budgetRounds: displayedRepairBudget,
            roundsUsed: priorRepairRounds + fixRounds,
            stopReason: fixLeak.kind === 'harness_failure' ? 'audit-failure' : 'contaminated' },
          sessionTotals,
          costUsd: resumedRepair ? addCostUsd(fixCost) : addCostUsd(build.costUsd, fixCost),
          durationMs: Date.now() - t0,
        }, progressionSelection);
      }
      bundle = grade(args, appDir, url, `${args.backend}-l${level}-fix${fixRounds}`, level, track, runId);

      const after = bundle?.totals?.score ?? 0;
      const afterMax = bundle?.totals?.max ?? 0;
      // Compare the SAME criteria in both rounds, not the totals.
      //
      // Compare criteria that were conclusive in both rounds, but never let a
      // previous observation disappear: conclusive -> inconclusive is lost
      // evidence and rolls the source back instead of hiding a regression.
      // The declared denominator is fixed; typed evidence still matters here
      // because an unmeasured check is not interchangeable with a real failure.
      const decision = repairEvidenceDecision(beforeBundle, bundle);
      const shared = decision.shared;
      if (decision.action === 'keep-setup-repair') {
        console.log(afterMax > 0
          ? `    application setup is now gradeable (${after}/${afterMax}); keeping this repair`
          : '    application setup is still failing; keeping the attempted repair for the next round');
        repairHistory.push(repairHistoryEntry(fixRounds, beforeBundle, bundle,
          afterMax > 0
            ? 'kept because the app became gradeable'
            : 'kept to continue repairing application setup'));
        if (!recordRepairProgression()) break;
        if (pauseForRepeatedFindings()) break;
        continue;
      }
      if (decision.action === 'rollback-no-comparison') {
        console.log('    no criteria were conclusively scored in both rounds; rolling back this fix');
        restoreSource(snapshot, appDir);
        bundle = grade(args, appDir, url, `${args.backend}-l${level}-rollback${fixRounds}`, level, track, runId);
        repairHistory.push(repairHistoryEntry(fixRounds, beforeBundle, bundle,
          'rolled back because the result could not be compared'));
        if (!recordRepairProgression()) break;
        if (pauseForRepeatedFindings()) break;
        continue;
      }
      if (shared.points < Math.min(beforeMax, afterMax)) {
        console.log(`    comparing ${shared.points} point(s) across ${shared.count} criteria scored in both rounds`
          + ` (${before}/${beforeMax} -> ${after}/${afterMax} overall)`);
      }
      if (decision.action === 'rollback-regression') {
        if (shared.lostEvidence.length) {
          console.log(`    lost conclusive evidence for ${shared.lostEvidence.length} criterion/criteria; rolling back this fix`);
        } else if (shared.definitionChanges.length) {
          console.log('    rubric points changed between grades; rolling back this fix');
        } else {
          console.log(`    regressed (${shared.before} -> ${shared.after} on shared criteria); rolling back this fix`);
        }
        // Stop the servers BEFORE deleting what they are watching. Without
        // this, rolling back a regressed postgres run threw EBUSY on
        // app/server and took the whole finished run down with it.
        restoreSource(snapshot, appDir);
        bundle = grade(args, appDir, url, `${args.backend}-l${level}-rollback${fixRounds}`, level, track, runId);
        regressed = true;
        repairHistory.push(repairHistoryEntry(fixRounds, beforeBundle, bundle,
          'rolled back because earlier behavior regressed'));
        if (!recordRepairProgression()) break;
        if (pauseForRepeatedFindings()) break;
        continue;
      }
      if (shared.after === shared.before) {
        const remaining = displayedRepairBudget - fixRounds;
        console.log(`    ${formatRepairProgress(shared, { before, beforeMax, after, afterMax })}; `
          + (remaining > 0 ? `${remaining} correction round(s) remain` : 'correction budget exhausted'));
      }
      repairHistory.push(repairHistoryEntry(fixRounds, beforeBundle, bundle,
        shared.after === shared.before ? 'kept with no score gain' : 'kept'));
      if (!recordRepairProgression()) break;
      if (pauseForRepeatedFindings()) break;
    }

    // A grading run that crashed writes no bundle, and recording that as 0/0
    // makes a harness failure indistinguishable from an app that scored nothing
    // — in a ladder run it silently drops a level's result on the floor. Say so
    // instead, and leave the score null.
    const finalBundleOutcome = classifyBundle(bundle);
    const progressionAttempt = progressionExecution?.state.attempts.at(-1) ?? null;
    // Progression uses stricter evidence rules than a regular scored bundle.
    // Store one answer when a selected check is not measured: the raw bundle
    // remains available for diagnosis, but the level is not a usable grade.
    const graded = levelGradeIsUsable(finalBundleOutcome,
      args.progression ? progressionAttempt : null);
    const nodeStrikes = progressionExecution
      ? dependencyStrikeRecords(progressionExecution.state, level, levelStrikeNodeIds)
      : null;
    const repairBudgetRounds = progressionExecution
      ? Math.max(priorRepairRounds + fixRounds, progressionRepairBudgetRounds)
      : args.fixRounds;
    const repairBudgetExhausted = progressionExecution
      ? finalBundleOutcome.kind === 'app_failure'
        && !(progressionNext?.type === 'repair' && progressionNext.level === level)
      : fixRounds >= args.fixRounds;
    const repairStatus = !graded ? 'ungraded'
      : finalBundleOutcome.kind === 'passed' ? (fixRounds > 0 ? 'corrected' : 'not-needed')
        : repairBudgetExhausted ? 'budget-exhausted' : 'incomplete';
    const stopReason = repairStopReason ?? ({
      'not-needed': 'not-needed',
      corrected: 'passed',
      'budget-exhausted': 'budget-exhausted',
    }[repairStatus] ?? null);
    const repair = {
      status: repairStatus,
      budgetRounds: repairBudgetRounds,
      roundsUsed: priorRepairRounds + fixRounds,
      ...(!args.progression ? { stallLimitRounds: args.maxStalledRepairs } : {}),
      stopReason,
      ...(nodeStrikes ? {
        strikeScope: 'feature',
        nodeStrikes,
      } : {}),
    };
    if (continuing) {
      run.continuation.cumulativeRoundsAfter = run.continuation.cumulativeRoundsBefore + fixRounds;
    }
    let checkpoint = null;
    try {
      checkpoint = preserveLevelCheckpoint({
        appDir,
        outputDir: args.out,
        runId,
        identities: run.identities,
        track: args.track,
        backend: args.backend,
        level,
        repair,
        outcome: finalBundleOutcome,
        selectionSha256: bundle?.selection?.sha256 ?? null,
      });
      console.log(`  kept the L${level} source checkpoint at ${join(args.out, checkpoint.directory)}`);
    } catch (error) {
      console.log(`  !! could not keep the L${level} source checkpoint: ${String(error.message).split('\n')[0]}`);
    }
    if (!graded) {
      console.log(`  L${level}: GRADING DID NOT COMPLETE — no usable bundle. ` +
        `Score is unknown, not zero; re-grade this level before using the run.`);
    }
    const buildSession = runSessionRecord(build);
    const sessionTotals = summarizeSessions(resumedRepair ? fixSessions
      : [buildSession, ...fixSessions]);
    run.levels.push({
      level,
      graded,
      score: graded ? bundle.totals.score : null,
      max: graded ? bundle.totals.max : null,
      // Whether the guarantees earned at earlier levels still hold at this one —
      // the whole point of growing the app level by level. It reached the
      // console and the bundle but not run.json, so the thesis metric was
      // missing from the durable record.
      regression: bundle?.totals?.regression ?? null,
      selection: bundle?.selection ?? null,
      ...(resumedRepair
        ? { resumedRepair: firstBuild }
        : continuing
        ? { baseline: firstBuild, resumeCostUsd: build.costUsd, resumeSession: buildSession }
        : { firstBuild, buildCostUsd: build.costUsd, buildSession }),
      contractPass: bundle?.totals?.contractPass ?? null,
      code: bundle?.code ?? null,
      fixCostUsd: addCostUsd(fixCost),
      fixSessions,
      repairHistory,
      sessionTotals,
      tokens: sessionTotals.tokens,
      // Carried up so a run summary can explain a cost, not just report one.
      usage: sessionTotals.usage,
      turns: sessionTotals.turns,
      promptBytes: sessionTotals.promptBytes,
      tokensPerTurn: sessionTotals.turns
        ? Math.round(sessionTotals.tokens / sessionTotals.turns) : null,
      // Reasoning actually produced. The budget is deliberately unpinned so runs
      // measure what a customer gets; that is only defensible if a shift in the
      // CLI default is visible afterwards rather than silently absorbed into
      // every score. agent.mjs measured this from the session transcript and the
      // level record was dropping it, so the guarantee was not holding.
      thinking: sessionTotals.thinking,
      fixRounds,
      ...(resumedRepair ? { priorRepairRounds,
        cumulativeFixRounds: priorRepairRounds + fixRounds } : {}),
      repair,
      checkpoint,
      // Keep the summary flag derived from the typed status so the two cannot drift.
      stalled: repairStatus === 'budget-exhausted' || repairStopReason === 'repeated-findings',
      regressed,
      outcome: finalBundleOutcome,
      durationSec: Math.round((Date.now() - t0) / 1000),
    });
    if (!args.progression || progressionExecution.state.attempts
      .some(attempt => attempt.level === level && attempt.outcome === 'conclusive')) {
      run.validation.ladder.completedLevels.push(level);
    }
    writeRunJson(join(args.out, 'run.json'), run);
    const blockedLevels = args.levelList.filter(candidate => candidate > level);
    if (args.progression) {
      if (progressionExecution.state.phase === 'terminal') {
        if (blockedLevels.length) run.validation.ladder.stoppedAfterLevel = level;
        run.validation.ladder.blockedLevels = blockedLevels;
        writeRunJson(join(args.out, 'run.json'), run);
        break;
      }
      if (progressionExecution.state.level <= level) {
        if (progressionExecution.state.attempts.at(-1)?.outcome === 'inconclusive') {
          run.validation.ladder.stoppedAfterLevel = level;
          run.validation.ladder.blockedLevels = [level, ...blockedLevels];
          writeRunJson(join(args.out, 'run.json'), run);
          break;
        }
        throw new Error(`dependency progression did not leave L${level} after its strike budget`);
      }
      continue;
    }
    if (blockedLevels.length && !ladderMayAdvance(finalBundleOutcome)) {
      run.validation.ladder.stoppedAfterLevel = level;
      run.validation.ladder.blockedLevels = blockedLevels;
      writeRunJson(join(args.out, 'run.json'), run);
      console.log(`  ladder paused after L${level}: L${level} must pass before `
        + `${blockedLevels.map(candidate => `L${candidate}`).join(', ')} can start`);
      console.log('  inspect the failures, then explicitly grant more repair rounds or correct the benchmark');
      break;
    }
  }

  if (args.mutations) {
    console.log(`\n================ ${args.backend} mutation control ================`);
    const pristineOutcome = aggregateRunOutcome(run.levels);
    if (args.referenceMutationOnly || mutationControlEligible(pristineOutcome)) {
      args.parentAttemptId = runId;
      args.mutationBaselineBundle = pristineMutationBaselinePath(args);
      run.mutationControl = runMutationControl(args, appDir, url, track,
        run.setup?.isolation?.imageId ?? null);
    } else {
      console.log(`  skipped: pristine outcome is ${pristineOutcome.kind}`);
      run.mutationControl = { ok: false, skipped: true,
        outcome: { kind: pristineOutcome.kind, phase: 'mutation-control-prerequisite',
          reason: `pristine outcome is ${pristineOutcome.kind}` } };
    }
    writeRunJson(join(args.out, 'run.json'), run);
  }

  // Record a final transcript audit in addition to the per-session hard gates.
  // The same retry and diagnostic path is used at both gates.
  let finalAuditFailure = null;
  const finalAudit = auditContamination(appDir);
  if (!finalAudit) {
    run.contaminated = false;
    run.contamination = { evidence: 'no reads of the grader, contracts, prompts or notes',
      verdict: 'scores usable' };
  } else if (finalAudit.kind === 'contaminated') {
    run.contaminated = true;
    run.contamination = { evidence: finalAudit.evidence, verdict: finalAudit.verdict };
    console.log('\n  !! CONTAMINATED: this build read the harness that grades it:');
    for (const evidence of finalAudit.evidence) console.log(`     ${evidence}`);
    console.log('     Scores from this run must not be quoted.');
  } else {
    run.contaminated = false;
    run.contamination = { evidence: finalAudit.evidence, verdict: finalAudit.verdict };
    const reason = finalAudit.evidence.join('; ');
    finalAuditFailure = { kind: 'harness_failure', phase: 'contamination-audit', reason,
      appFailures: [], inconclusive: [], harnessFailures: [reason] };
    console.log('\n  !! AUDIT DID NOT COMPLETE. Scores from this run must not be quoted.');
  }

  // Keep the transcript evidence outside the provider CLI's prunable store.
  try {
    sh('node', [join(ROOT, 'dist', 'commands', 'archive-transcripts.js'), '--app', appDir, '--label', artifactLabel],
      { stdio: 'pipe' });
  } catch { console.log('  (transcript archiving failed — evidence is on a 30-day timer)'); }

  run.outcome = finalAuditFailure ?? (args.referenceMutationOnly && run.mutationControl?.ok
    ? { kind: 'passed', phase: 'mutation-control', reason: null,
      appFailures: [], inconclusive: [], harnessFailures: [] }
    : aggregateRunOutcome(run.levels));
  if (args.mutations && !run.mutationControl?.ok && !run.mutationControl?.skipped) {
    run.outcome = { kind: run.mutationControl?.outcome?.kind === 'incomplete'
      ? 'incomplete' : 'harness_failure', phase: 'mutation-control',
      reason: run.mutationControl?.outcome?.reason
        ?? run.mutationControl?.processError
        ?? 'one or more declared mutations were not cleanly caught',
      appFailures: [], inconclusive: [] };
  }

  if (run.levels.some(level => level.graded === true)) {
    try {
      preserveFinalPackageEvidence({ appDir, outputDir: args.out });
      console.log(`  source kept at ${join(args.out, 'source')}`);
      console.log(`  grading detail kept at ${join(args.out, 'grading')}`);
    } catch (error) {
      const reason = String(error.message).split(/\r?\n/)[0];
      run.outcome = { kind: 'harness_failure', phase: 'evidence-preservation', reason,
        appFailures: [], inconclusive: [], harnessFailures: [reason] };
      console.log(`  !! ${reason}`);
    }
  }

  finalizeRunTotals(run, started, { costComplete: runCostComplete });
  if (args.repairGrant) {
    run.continuation.cumulativeCostAfterUsd = addCostUsd(
      run.continuation.cumulativeCostBeforeUsd, run.totals.costUsd);
    run.continuation.cumulativeDurationAfterSec = run.continuation.cumulativeDurationBeforeSec
      + run.totals.durationSec;
  }
  run.completedAt = new Date().toISOString();
  writeRunJson(join(args.out, 'run.json'), run);

  // Produce a model-free friction report from the transcript when available.
  if (executeStackCapability(stackAdapter, 'run-policy', 'product-review-enabled')
    && run.setup?.session !== 'model-free-reference') {
    try {
      sh('node', [join(ROOT, 'dist', 'commands', 'stdb-report.js'), '--label', artifactLabel, '--track', args.track,
        '--level', String(args.levelList[args.levelList.length - 1]),
        '--score', `${run.totals.score}/${run.totals.max}`,
        '--cost', String(run.totals.costUsd),
        '--fix-rounds', String(run.totals.fixRounds),
        ...(run.contaminated ? ['--contaminated'] : [])], { stdio: 'inherit' });
    } catch (e) {
      console.log(`  (stdb friction report failed: ${String(e.message).split('\n')[0]})`);
    }
    // The deeper behavioural review is a separate model session. It is useful
    // product research, but running it implicitly would add unmetered provider
    // usage to a benchmark attempt and make campaign cost accounting false.
    // Keep the model-free friction report above automatic; make this analysis
    // explicit and never run it for an incomplete attempt.
    if (args.behavioralReview
      && !['provider_failure', 'harness_failure', 'ungraded'].includes(run.outcome?.kind)) {
      try {
        sh('node', [join(ROOT, 'dist', 'commands', 'stdb-review.js'), '--label', artifactLabel,
          '--source', join(args.out, 'source'),
          '--compare', executeStackCapability(stackAdapter, 'run-policy', 'product-review-comparisons')
            .map(b => resultsName(track, b, args.runIndex)).join(',')], { stdio: 'inherit' });
      } catch (e) {
        console.log(`  (stdb behavioural review failed: ${String(e.message).split('\n')[0]})`);
      }
    }
  }

  console.log(`\n================ ${args.backend} summary ================`);
  for (const l of run.levels) {
    console.log(`  ${formatLevelSummary(l)}`);
  }
  console.log(`  TOTAL ${run.totals.score}/${run.totals.max}  ` +
    `$${run.totals.costUsd}  ${run.totals.fixRounds} fix round(s)  ${run.totals.durationSec}s`);
  console.log(`  ${join(args.out, 'run.json')}`);

  teardown();

  // Leave nothing in temp. Best-effort: a directory some process still holds is
  // not worth failing a finished run over, and the next run makes its own
  // anyway. Say so rather than leaving it to be discovered. Only for a
  // directory THIS run created — an explicit --app is the caller's.
  if (ownWorkDir) {
    try {
      rmSync(dirname(appDir), { recursive: true, force: true });
    } catch {
      console.log(`  (work dir still held: ${dirname(appDir)} — the next sweep will take it)`);
    }
  }
  process.exitCode = runExitCode(run.outcome);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  main().catch(error => {
    console.error(error.stack ?? error.message);
    try { emergencyTeardown?.(); }
    catch (cleanupError) {
      console.error(`cleanup after failure also failed: ${String(cleanupError.message).split(/\r?\n/)[0]}`);
    }
    process.exitCode = 1;
  });
}
