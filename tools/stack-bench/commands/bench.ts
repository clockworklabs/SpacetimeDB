#!/usr/bin/env node

import { execFile, execFileSync } from 'node:child_process';
import type { ChildProcess, ExecFileSyncOptionsWithStringEncoding } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync, cpSync, rmSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';
import { loadTrack, resultsName, portsFor, workDirFor, assertNoPortCollisions,
  moduleName, dbName, suitesFor } from '../src/composition/tracks.js';
import { parseBenchArguments } from './bench-arguments.js';
import type { BenchArguments } from './bench-arguments.js';
import { killTree } from '../src/runtime/platform.js';
import { formatRepairProgress } from '../src/evidence/scoring.js';
import { ARTIFACT_FILE, emptyArtifactIdentities, readArtifact, readArtifactPayload,
  writeArtifact, writeRunJson } from '../src/evidence/artifacts.js';
import { aggregateRunOutcome, classifyBundle, ladderMayAdvance, ladderMayContinue,
  mutationControlEligible, runExitCode, runOutcomeKind } from '../src/evidence/outcomes.js';
import { summarizeSessions } from '../src/evidence/session-metrics.js';
import { hashDirectory, sha256 } from '../src/evidence/provenance.js';
import { createBackendLease, newRunId, publicBackendLease, readBackendLease,
  acquireResourceLocks, backendResourceLockKeys, releaseResourceLocks, resourceLockScope,
  writeBackendLease } from '../src/runtime/backend-lease.js';
import { captureApplicationDiagnostics } from '../src/runtime/backend-control.js';
import type { RuntimeControlSpec } from '../src/runtime/backend-control.js';
import { releaseBackendLease } from '../src/runtime/backend-teardown.js';
import { resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { createAgentVisibleTaskRequest, createBoundRecipeTaskRequest }
  from '../src/composition/recipe-selection.js';
import { criterionEvidence, evidencePassed } from '../src/evidence/check-evidence.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { agentRecipeIdentity, agentRequestArgv } from '../src/agents/agent-adapter-contract.js';
import { agentSessionFailure, validateAgentResult }
  from '../src/agents/agent-result-contract.js';
import { AGENT_ADAPTER_REGISTRY, agentAdapterIdentity } from '../src/agents/agent-adapters.js';
import { archiveTranscripts } from '../src/agents/transcript-archive.js';
import { runPreflight } from '../src/runtime/preflight.js';
import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.js';
import { SUPERVISOR_STATE_VERSION, writeRecoveryArtifact } from '../src/runtime/recovery.js';
import { applyAgentCredential } from '../src/agents/agent-credentials.js';
import { hashAppSource, resetAppToSource, seedAppSource, snapshotAppSource } from '../src/runtime/source-snapshot.js';
import { finalPackageEvidenceRequired, preserveFinalPackageEvidence, preserveLevelCheckpoint,
  sourceBoundFirstBuildOutcome } from '../src/runtime/source-checkpoint.js';
import { materializationAppFailure, materializeAcceptedSource }
  from '../src/runtime/source-materialization.js';
import { compareRepairBaseline, createRepairGrant } from '../src/runtime/repair-grant.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.js';
import { contractInterfaceNames } from '../src/composition/agent-visible-contract.js';
import { clearPrivateGradingEvidence, levelGradeIsUsable, repairEvidenceDecision,
  repairHistoryEntry, repairProgressState, restorePrivateGradingEvidence }
  from '../src/evidence/repair-evidence.js';
import { mutationControlArgv, mutationControlTimeoutMs, pristineMutationBaselinePath }
  from '../src/evidence/mutation-control.js';
import type { MutationControlArgs } from '../src/evidence/mutation-control.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import { dependencyRepairBudget, dependencyStrikeRecords }
  from '../src/progression/dependency-mode.js';
import { DEPENDENCY_MODE_VERSION } from '../src/progression/dependency-definition.js';
import { resolveProgressionRecipeAction, resolveProgressionRecipeLevelSelection,
  validateProgressionCampaignLevelScope }
  from '../src/progression/progression-recipe-selection.js';
import { createLiveProgressionExecution }
  from '../src/progression/live-progression.js';
import type { CampaignSelection } from '../src/campaigns/campaign-compiler.js';
import { gradingRunTimeoutMs, selectedGradingSourceCount }
  from '../src/runtime/grading-timeout.js';
import { claudeRatesForModel } from '../src/evidence/claude-usage-cost.js';
import { PRICING_UNIT, validatePricingAuthority }
  from '../src/evidence/pricing-authority.js';
import type { BoundRecipeTaskRequestResult } from '../src/composition/recipe-selection.js';
import type { RecipeBinding } from '../src/composition/recipe-release.js';
import type { RepairGrantResolution, RepairOutcome } from '../src/runtime/repair-grant.js';
import type { AgentAdapter, AgentMode, AgentRequest }
  from '../src/agents/agent-adapter-contract.js';
import type { ValidatedAgentResult } from '../src/agents/agent-result-contract.js';
import type { Track } from '../src/composition/tracks.js';
import type { RunOutcome } from '../src/evidence/outcomes.js';
import type { GradeBundlePayload, BenchmarkRunRecord, RunLevelRecord,
  RunContinuation, RunTotals } from '../src/evidence/benchmark-run.js';
import { addCostUsd, finalizeRunTotals, runSessionRecord }
  from '../src/evidence/benchmark-run.js';
import { formatLevelSummary } from '../src/evidence/evidence-presentation.js';
import type { ProgressionAction } from '../src/progression/progression-engine.js';
import type { ProgressionState } from '../src/progression/progression-state.js';
import type { ProgressionRecipeAction, ProgressionRecipeSelections }
  from '../src/progression/progression-recipe-selection.js';
import type { BackendLease } from '../src/runtime/backend-lease.js';

import { STACK_BENCH_ROOT as ROOT, compiledEntrypoint } from '../src/package-root.js';
import { stackBenchResultsRoot } from '../src/runtime/operational-paths.js';
import { runningContainerIdentity } from '../src/runtime/container-identity.js';
const COMMAND_TIMEOUT_MS = 20 * 60_000;

type UnknownRecord = Record<string, unknown>;
type ContaminationAudit = { kind: 'contaminated' | 'harness_failure'; evidence: string[];
  verdict: string };
type LeakAuditEntry = { hits: Array<{ kind: string; path: string }> };
type CommandFailure = Error & { stdout?: string | Buffer; stderr?: string | Buffer;
  status?: number | null; signal?: NodeJS.Signals | null };
type GradeOptions = { observation?: 'scored' | 'observed'; out?: string | null;
  sourceSha256?: string | null; applicationFailure?: RunOutcome | null };
type MutationControlResult = UnknownRecord & { ok: boolean; artifact?: string;
  skipped?: boolean; processError?: string | null; outcome: RunOutcome | null };
type RecipeTask = (BoundRecipeTaskRequestResult | ProgressionRecipeSelections['grader']) & { agentRequest?: UnknownRecord;
  progressionAction?: ProgressionAction };
type BenchArgs = BenchArguments & {
  recipeTasks: Map<number, RecipeTask>;
  recipeBindings: Map<number, RecipeBinding>;
  repairGrant?: RepairGrantResolution;
  mutationImageId?: string;
  spentBudgetUsd?: number;
};
type ProgressionWorkRecipeAction = ProgressionRecipeSelections & {
  action: Exclude<ProgressionAction, { type: 'terminal' }>;
};
type FirstBuildRecord = {
  score: number | null;
  max: number | null;
  regression: NonNullable<GradeBundlePayload['totals']>['regression'] | null;
  contractPass: boolean | null;
  outcome: RunOutcome;
  source: { sha256: string; files: number } | null;
  missed: string[];
  observations?: UnknownRecord;
};
type RepairStatus = 'not-needed' | 'corrected' | 'budget-exhausted' | 'incomplete' | 'ungraded';
type ProgressionFailure = { kind?: string; reason?: string };

const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const errorMessage = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

function commandFailure(error: unknown): CommandFailure {
  if (error instanceof Error) return error;
  throw error;
}

function parseLeakAudit(value: string): LeakAuditEntry[] {
  const parsed: unknown = JSON.parse(value);
  if (!Array.isArray(parsed)) throw new Error('contamination audit output must be an array');
  return parsed.map((entry, index) => {
    if (!object(entry) || !Array.isArray(entry.hits)) {
      throw new Error(`contamination audit output[${index}] is invalid`);
    }
    const hits = entry.hits.map((hit, hitIndex) => {
      if (!object(hit) || typeof hit.kind !== 'string' || typeof hit.path !== 'string') {
        throw new Error(`contamination audit output[${index}].hits[${hitIndex}] is invalid`);
      }
      return { kind: hit.kind, path: hit.path };
    });
    return { hits };
  });
}

function stringArray(value: unknown, at: string): string[] {
  if (!Array.isArray(value) || value.some(entry => typeof entry !== 'string')) {
    throw new Error(`${at} must be an array of strings`);
  }
  return [...value];
}

function campaignSelection(value: unknown, at: string): CampaignSelection {
  if (!object(value)) throw new Error(`${at} must be an object`);
  const optionalStrings = (field: 'packs' | 'checks'): string[] | undefined => {
    const entry = value[field];
    if (entry === undefined) return undefined;
    return stringArray(entry, `${at}.${field}`);
  };
  let levels: CampaignSelection['levels'];
  if (value.levels !== undefined) {
    if (!Array.isArray(value.levels)) throw new Error(`${at}.levels must be an array`);
    levels = value.levels.map((entry, index) => {
      if (!object(entry)) throw new Error(`${at}.levels[${index}] is invalid`);
      const level = entry.level;
      const recipe = entry.recipe;
      if (typeof level !== 'number' || !Number.isSafeInteger(level) || typeof recipe !== 'string') {
        throw new Error(`${at}.levels[${index}] is invalid`);
      }
      return { level, recipe,
        ...(entry.features === undefined ? {} : { features: stringArray(entry.features,
          `${at}.levels[${index}].features`) }),
        ...(entry.checks === undefined ? {} : { checks: stringArray(entry.checks,
          `${at}.levels[${index}].checks`) }) };
    });
  }
  return { ...(optionalStrings('packs') === undefined ? {} : { packs: optionalStrings('packs') }),
    ...(optionalStrings('checks') === undefined ? {} : { checks: optionalStrings('checks') }),
    ...(levels === undefined ? {} : { levels }) };
}

function isProgressionWorkRecipeAction(value: ProgressionRecipeAction):
  value is ProgressionWorkRecipeAction {
  return value.action.type !== 'terminal';
}

function repairReportArgs(value: ProgressionRecipeAction | null): string[] {
  if (!value || !isProgressionWorkRecipeAction(value) || value.action.type !== 'repair') return [];
  if (!object(value.action.prompt) || !Array.isArray(value.action.prompt.nodeIds)
    || !object(value.action.grading) || !Array.isArray(value.action.grading.checks)) {
    throw new Error('dependency repair action has invalid prompt or grading selections');
  }
  const promptNodeIds = new Set(value.action.prompt.nodeIds.map(nodeId => {
    if (typeof nodeId !== 'string' || !nodeId) {
      throw new Error('dependency repair action has an invalid prompt node');
    }
    return nodeId;
  }));
  const checks = value.action.grading.checks.flatMap(check => {
    if (!object(check) || typeof check.id !== 'string' || !check.id
      || typeof check.nodeId !== 'string' || !check.nodeId) {
      throw new Error('dependency repair action has an invalid grading check');
    }
    return promptNodeIds.has(check.nodeId) ? [check.id] : [];
  });
  if (checks.length === 0) throw new Error('dependency repair action selects no repair checks');
  const interfaces = contractInterfaceNames(value.agent.task.contractText);
  return ['--checks-json', JSON.stringify(checks),
    '--controls-json', JSON.stringify(interfaces)];
}

function requireProgressionState(state: ProgressionState | null): ProgressionState {
  if (!state) throw new Error('live dependency progression has no active state');
  return state;
}

function requireContinuation(run: BenchmarkRunRecord): RunContinuation {
  if (!run.continuation) throw new Error('repair continuation has no continuation record');
  return run.continuation;
}

function requireRunTotals(run: BenchmarkRunRecord): RunTotals {
  if (!run.totals) throw new Error('benchmark run totals are not available');
  return run.totals;
}

function repairOutcome(outcome: RunOutcome): RepairOutcome {
  return { kind: outcome.kind, appFailures: [...(outcome.appFailures ?? [])],
    inconclusive: [...(outcome.inconclusive ?? [])],
    harnessFailures: [...(outcome.harnessFailures ?? [])] };
}

function progressionFailure(outcome: RunOutcome): ProgressionFailure {
  return { kind: outcome.kind, ...(outcome.reason === null || outcome.reason === undefined
    ? {} : { reason: outcome.reason }) };
}

function mutationControlArgs(args: BenchArgs): MutationControlArgs {
  if (!args.out || !args.mutations || !args.backend || !args.parentAttemptId) {
    throw new Error('mutation control has incomplete run identity');
  }
  return { levelList: args.levelList, out: args.out, recipe: args.recipe,
    recipeTasks: args.recipeTasks, mutations: args.mutations, backend: args.backend,
    track: args.track, runIndex: args.runIndex, parentAttemptId: args.parentAttemptId,
    mutationShardIndex: args.mutationShardIndex, mutationShardCount: args.mutationShardCount,
    mutationResumeFrom: args.mutationResumeFrom, mutationCheckpointOut: args.mutationCheckpointOut,
    mutationBaselineBundle: args.mutationBaselineBundle,
    expectedMutationCalibration: args.expectedMutationCalibration,
    mutationMaxRuntimeMinutes: args.mutationMaxRuntimeMinutes,
    mutationImageId: args.mutationImageId };
}

function mutationOutcome(value: unknown): RunOutcome | null {
  if (value === null || value === undefined) return null;
  if (!object(value)) {
    throw new Error('mutation control artifact outcome is invalid');
  }
  return { kind: runOutcomeKind(value.kind),
    ...(typeof value.phase === 'string' ? { phase: value.phase } : {}),
    ...(typeof value.reason === 'string' ? { reason: value.reason } : {}) };
}

function recipeRequestIdentity(value: unknown): { recipeSha256: string; selectionSha256: string;
  taskPacks: unknown; taskSha256: string } {
  if (!object(value) || !object(value.recipe) || !object(value.selection) || !object(value.task)
    || typeof value.recipe.contentSha256 !== 'string' || typeof value.selection.sha256 !== 'string'
    || typeof value.task.sha256 !== 'string') {
    throw new Error('recipe task request has no complete identity');
  }
  return { recipeSha256: value.recipe.contentSha256, selectionSha256: value.selection.sha256,
    taskPacks: value.selection.taskPacks, taskSha256: value.task.sha256 };
}

function snapshotSource(appDir: string, to: string): void {
  snapshotAppSource(appDir, to);
}

// Check contamination after every coding session. File-tool permissions do not
// govern shell reads, so the transcript audit remains a separate hard gate.
function auditContamination(appDir: string): ContaminationAudit | null {
  const args = [join(ROOT, 'dist', 'commands', 'leak-audit.js'), '--app', appDir, '--json'];
  let firstFailure: unknown = null;
  for (let attempt = 1; attempt <= 2; attempt++) {
    try {
      const audit = sh('node', args, { stdio: 'pipe' });
      const escapes = parseLeakAudit(audit).flatMap(entry => entry.hits);
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

export function auditFailureSummary(error: unknown): string {
  const failure = object(error) ? error : {};
  const message = errorMessage(error).split(/\r?\n/)[0] ?? '';
  const stderrLines = String(failure.stderr ?? '').trim().split(/\r?\n/).filter(Boolean);
  const stderr = stderrLines.find(line => /(?:error|eacces|permission denied|failed)/i.test(line))
    ?? stderrLines[0];
  const details = [
    Number.isInteger(failure.status) ? `exit ${String(failure.status)}` : null,
    failure.signal ? `signal ${String(failure.signal)}` : null,
    stderr ? `stderr: ${stderr}` : null,
  ].filter(Boolean);
  return details.length ? `${message} (${details.join('; ')})` : message;
}

const sh = (cmd: string, args: readonly string[],
  opts: Omit<ExecFileSyncOptionsWithStringEncoding, 'encoding'> = {}): string =>
  execFileSync(cmd, [...args], {
    encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: COMMAND_TIMEOUT_MS, ...opts,
  });

let activeAgentChild: ChildProcess | null = null;
// Set once a run owns resources. The top-level rejection handler invokes this
// directly; relying only on process 'exit' made cleanup best-effort precisely
// when an awaited build rejected unexpectedly.
let emergencyTeardown: (() => void) | null = null;

export function parseAgentProcessResult(stdout: string, stderr: string, processError: unknown,
  request: AgentRequest): ValidatedAgentResult {
  const resultLine = stdout.trim().split('\n').pop();
  let result: ValidatedAgentResult;
  try {
    if (!resultLine) throw new Error('agent returned no result line');
    result = validateAgentResult(JSON.parse(resultLine), request);
  } catch (resultError) {
    const stdoutTail = stdout.trim().slice(-2000) || '<empty>';
    const stderrTail = stderr.trim().slice(-4000) || '<empty>';
    const processDetail = processError ? `agent process failed: ${errorMessage(processError)}\n` : '';
    throw new Error(`${processDetail}agent returned an invalid result: ${errorMessage(resultError)}\n`
      + `agent stdout tail:\n${stdoutTail}\nagent stderr tail:\n${stderrTail}`);
  }
  if (processError && result.ok) {
    throw new Error(`agent process failed after reporting success: ${errorMessage(processError)}`);
  }
  return result;
}

function runAgent(
  args: BenchArgs,
  adapter: AgentAdapter,
  mode: AgentMode,
  level: number,
  appDir: string,
): Promise<ValidatedAgentResult> {
  if (!args.backend || !args.model) throw new Error('agent run requires backend and model');
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
  const request: AgentRequest = { mode, level, app: appDir, backend: args.backend, track: args.track,
    runIndex: args.runIndex, model: args.model, guidance: args.guidance, skills: args.skills,
    ...(adapter.usesStackSkills
      ? { skillIdentity: args.condition?.guidance.skills[args.backend] } : {}),
    recipe: agentRecipeIdentity(args.recipe, recipeTask),
    guidanceDocument: args.guidanceDocument,
    credentialAliases: args.condition?.guidance?.credentialAliases ?? {},
    recipeTask, pricing: args.pricing,
    maxBudgetUsd: remainingBudget, adapterCostLimit: adapter.costLimit };
  const argv = agentRequestArgv(adapter, request);
  if (args.apiKey && !adapter.apiKeyEnvironmentVariable) {
    throw new Error(`agent adapter ${adapter.id} does not accept an API key`);
  }
  const env = { ...process.env };
  if (args.apiKey) {
    const credentialName = adapter.apiKeyEnvironmentVariable;
    if (!credentialName) throw new Error(`agent adapter ${adapter.id} does not accept an API key`);
    env[credentialName] = args.apiKey;
  }
  return new Promise((resolveRun, rejectRun) => {
    const child = execFile('node', argv, {
      encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: adapter.deadlineMs,
      env,
    },
      (error, stdout, stderr) => {
        if (activeAgentChild === child) activeAgentChild = null;
        try {
          const result = parseAgentProcessResult(stdout, stderr, error, request);
          args.spentBudgetUsd = addCostUsd(args.spentBudgetUsd, result.costUsd);
          resolveRun(result);
        }
        catch (parseError) {
          rejectRun(parseError);
        }
      });
    activeAgentChild = child;
  });
}

interface GradeCheck {
  stableKey: string;
  executionId?: string;
  source?: string;
}

interface GradeRecipeTask {
  request: UnknownRecord;
  selection: { checks: readonly GradeCheck[] }
    | { scoredChecks: readonly GradeCheck[]; observedChecks?: readonly GradeCheck[] };
}

function checksForGrade(task: GradeRecipeTask | undefined, observation: GradeOptions['observation']):
  readonly GradeCheck[] {
  if (!task) return [];
  if ('scoredChecks' in task.selection) {
    return observation === 'observed'
      ? task.selection.observedChecks ?? [] : task.selection.scoredChecks;
  }
  return task.selection.checks;
}

type GradeArguments = Pick<BenchArgs, 'backend' | 'track' | 'runIndex' | 'media' | 'recipe'> & {
  recipeTasks?: ReadonlyMap<number, GradeRecipeTask>;
  progression?: { identity: { policy?: string } };
  condition?: { guidance?: { credentialAliases?: Record<string, unknown> } };
};

export function gradeArgv(
  args: GradeArguments,
  appDir: string,
  url: string,
  label: string,
  level: number,
  track: Track,
  parentAttemptId: string,
  { observation = 'scored', out = null, sourceSha256 = null,
    applicationFailure = null }: GradeOptions = {},
): string[] {
  if (!args.backend) throw new Error('grading requires a backend');
  const restartSpec = restartSpecFor(args, appDir, track);
  const task = args.recipeTasks?.get(level);
  return [compiledEntrypoint('commands', 'run-suite.js'), '--app', appDir, '--url', url,
    '--backend', args.backend, '--label', label, '--level', String(level),
    '--track', args.track,
    '--run-index', String(args.runIndex),
    '--parent-attempt-id', parentAttemptId,
    '--observation', observation,
    ...(out ? ['--out', out] : []),
    ...(sourceSha256 ? ['--source-sha256', sourceSha256] : []),
    ...(args.recipe ? ['--recipe', args.recipe] : []),
    ...(task ? ['--recipe-task-json', JSON.stringify(task.request)] : []),
    ...(args.condition?.guidance?.credentialAliases
      ? ['--credential-aliases-json', JSON.stringify(
          args.condition.guidance.credentialAliases)] : []),
    ...(applicationFailure
      ? ['--application-failure-json', JSON.stringify(applicationFailure)] : []),
    ...(observation === 'scored' && args.recipeTasks && !args.progression
      ? ['--regression-checks-json', JSON.stringify([...args.recipeTasks.entries()]
        .filter(([priorLevel]) => priorLevel < level)
        .flatMap(([, priorTask]) => checksForGrade(priorTask, 'scored')
          .map(check => check.stableKey)))] : []),
    ...(args.media && observation === 'scored' ? [] : ['--no-media']),
    ...(!STACK_ADAPTER_REGISTRY.get(args.backend).runPolicy.resetEnabled
      ? ['--no-reset']
      : ['--restart-spec', JSON.stringify(restartSpec)])];
}

function grade(
  args: BenchArgs,
  appDir: string,
  url: string,
  label: string,
  level: number,
  track: Track,
  parentAttemptId: string,
  options: GradeOptions = {},
): GradeBundlePayload | null {
  const { out = null } = options;
  const source = hashAppSource(appDir);
  const argv = gradeArgv(args, appDir, url, label, level, track, parentAttemptId, {
    ...options, sourceSha256: options.sourceSha256 ?? source.sha256,
  });
  const bundle = join(out ?? join(appDir, 'stack-bench'), ARTIFACT_FILE.gradeBundle);
  rmSync(bundle, { force: true });
  const task = args.recipeTasks?.get(level);
  const currentChecks = checksForGrade(task, options.observation);
  const regressionChecks = options.observation === 'observed' || args.progression
    ? []
    : [...(args.recipeTasks?.entries() ?? [])]
      .filter(([priorLevel]) => priorLevel < level)
      .flatMap(([, priorTask]) => checksForGrade(priorTask, 'scored'));
  const sourceCount = task
    ? selectedGradingSourceCount(currentChecks, regressionChecks)
    : suitesFor(track, level).length;
  try {
    sh('node', argv, { stdio: 'inherit', timeout: gradingRunTimeoutMs(sourceCount) });
  } catch { /* a current bundle may still explain a scored failure */ }
  return existsSync(bundle)
    ? readArtifactPayload<GradeBundlePayload>(bundle, { expectedKind: 'grade_bundle' }) : null;
}

function restartSpecFor(args: Pick<GradeArguments, 'backend' | 'runIndex'>,
  appDir: string, track: Track): RuntimeControlSpec {
  if (!args.backend) throw new Error('restart specification requires a backend');
  const port = portsFor(track, args.backend, args.runIndex).vite ?? null;
  if (port == null) throw new Error(`stack ${args.backend} has no application port`);
  return { backend: args.backend, app: appDir, port: Number(port), probe: '' };
}

function runMutationControl(
  args: BenchArgs,
  appDir: string,
  url: string,
  track: Track,
  imageId: string | null,
): MutationControlResult {
  if (!args.out || !args.mutations) throw new Error('mutation control requires output and manifest paths');
  const output = join(args.out, ARTIFACT_FILE.mutationControl);
  if (!args.mutationResumeFrom || resolve(args.mutationResumeFrom) !== resolve(output)) {
    rmSync(output, { force: true });
  }
  if (imageId) args.mutationImageId = imageId;
  else delete args.mutationImageId;
  const argv = mutationControlArgv(mutationControlArgs(args), appDir, url, track);
  let processError = null;
  try { sh(process.execPath, argv, {
    stdio: 'inherit', timeout: mutationControlTimeoutMs(args.mutationMaxRuntimeMinutes),
  }); }
  catch (error) { processError = errorMessage(error).split('\n')[0] ?? null; }
  if (!existsSync(output)) {
    return { ok: false, artifact: output, processError,
      outcome: { kind: 'harness_failure', phase: 'mutation-control',
        reason: processError ?? 'mutation runner produced no artifact' } };
  }
  const artifact = readArtifactPayload(output, { expectedKind: 'mutation_control' });
  return { ok: artifact.ok === true && !processError, artifact: output,
    processError, summary: artifact.summary ?? null, outcome: mutationOutcome(artifact.outcome),
    results: artifact.results ?? [] };
}

function validateMutationInput(args: BenchArgs): void {
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

async function main() {
  const args: BenchArgs = {
    ...parseBenchArguments(process.argv),
    recipeTasks: new Map(),
    recipeBindings: new Map(),
  };
  let repairGrant = null;
  if (args.repairFrom) {
    const repairLevel = args.repairLevel;
    if (typeof repairLevel !== 'number' || !Number.isSafeInteger(repairLevel) || repairLevel < 1) {
      throw new Error('--repair-from requires a positive --repair-level');
    }
    repairGrant = createRepairGrant(args.repairFrom,
      { level: repairLevel, rounds: args.fixRounds });
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
      selectionRequest: campaignSelection(config.selectionRequest, 'repair configuration.selectionRequest'),
      skills: config.skills,
      packIds: [...(campaignSelection(config.selectionRequest,
        'repair configuration.selectionRequest').packs ?? [])],
      checkKeys: [...(campaignSelection(config.selectionRequest,
        'repair configuration.selectionRequest').checks ?? [])],
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
  if (!args.backend) throw new Error('benchmark run requires a backend');
  const stackAdapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  const materializeCodingOutput = stackAdapter.id !== 'stub';
  const agentAdapter = AGENT_ADAPTER_REGISTRY.get(args.agentAdapter);
  if (process.env.STACK_BENCH_APPLIANCE !== '1' && agentAdapter.costLimit !== 'non-billable') {
    throw new Error(`agent adapter ${agentAdapter.id} requires the Docker appliance`);
  }
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
  applyAgentCredential(args, agentAdapter);
  args.model ??= agentAdapter.defaultModel;
  if (!args.model) throw new Error(`agent adapter ${agentAdapter.id} has no default model`);
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
  if (args.retainBackend && !stackAdapter.runPolicy.retainHostSupported) {
    throw new Error(`stack adapter ${args.backend} does not support --retain-backend`);
  }
  const stackRuntime = stackAdapter.orchestrator.config(
    { root: ROOT, env: process.env, helpers: { exists: existsSync } });
  Object.assign(process.env, stackRuntime.environment);
  process.env.STACK_BENCH_NODE_BIN = process.platform === 'win32' ? 'node.exe' : process.execPath;
  const track = loadTrack(args.track);
  // Resolve the requested scope for every level before probing the sandbox,
  // acquiring a backend lease or paying for a build. A pack that exists at L2
  // but not L1 is not a late grading surprise; it is an invalid run request.
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
      const progressionSelection = args.featureCatalog
        ? resolveProgressionRecipeLevelSelection(binding, args.featureCatalog, level,
          { cumulative: Boolean(args.progression) }) : null;
      const resolved = progressionSelection === null
        ? createBoundRecipeTaskRequest(binding, requested?.features
          ? { featureIds: requested.features,
              requestedSpecifications: requested.specifications?.requested,
              expectedSpecifications: requested.specifications?.expected,
              observedSpecifications: requested.specifications?.observed,
              checkKeys: requested.checks }
          : args) : null;
      const grader = progressionSelection?.grader ?? resolved;
      if (!grader) throw new Error(`L${level} has no recipe task request`);
      if (args.condition && !declared) {
        throw new Error(`study condition does not bind requested L${level}`);
      }
      const graderIdentity = recipeRequestIdentity(grader.request);
      if (declared && (declared.recipe.contentSha256 !== graderIdentity.recipeSha256
        || declared.selection.sha256 !== graderIdentity.selectionSha256
        || JSON.stringify(declared.selection.taskPacks) !== JSON.stringify(graderIdentity.taskPacks)
        || declared.task.sha256 !== graderIdentity.taskSha256)) {
        throw new Error(`study condition requested scope changed before L${level}`);
      }
      if (progressionSelection) {
        const progressionGrader = progressionSelection.grader;
        args.recipeTasks.set(level, {
          request: progressionGrader.request,
          selection: progressionGrader.selection,
          task: progressionGrader.task,
          agentRequest: progressionSelection.agent.request,
        });
      } else if (resolved) {
        args.recipeTasks.set(level, {
          ...resolved,
          agentRequest: createAgentVisibleTaskRequest(binding, resolved),
        });
      }
    }
  }
  if (args.progression) {
    const state = progressionEngine.initialize(args.progression.definition);
    const declared = args.condition?.requested?.levels
      ?.find(entry => entry.level === state.level) ?? null;
    const binding = resolveRecipeRelease(track, state.level,
      declared ? `${declared.recipe.id}@${declared.recipe.version}` : null);
    if (!binding) throw new Error(`L${state.level} has no recipe release`);
    resolveProgressionRecipeAction(binding, state);
    if (!args.progressionOwner) {
      throw new Error('live dependency progression requires an exact compiled campaign attempt');
    }
  }
  if (repairGrant) {
    const expectedSelection = repairGrant.level.selection?.sha256 ?? null;
    const repairTask = args.recipeTasks.get(repairGrant.level.level);
    const resolvedSelection = repairTask ? recipeRequestIdentity(repairTask.request).selectionSha256 : null;
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
  // ports before any paid coding session begins.
  const admittedSmoke = args.campaignAdmission?.reusable === true
    ? { id: args.campaignAdmission.id, createdAt: args.campaignAdmission.createdAt }
    : null;
  const preflight = args.backend === 'stub' ? null : runPreflight({
    backends: [args.backend], track: args.track, levels: args.levels,
    levelList: args.levelList, runIndex: args.runIndex, agentAdapter: args.agentAdapter,
    guidance: args.guidance,
    recipe: args.recipe,
    ...(args.condition?.requested ? { requestedScopes: [args.condition.requested] } : {}),
    ...(args.featureCatalog ? { featureCatalog: args.featureCatalog } : {}),
    ...(args.runMode ? { mode: args.runMode } : {}),
    agentSkills: args.skills ?? null,
    packIds: args.packIds, checkKeys: args.checkKeys, smoke: admittedSmoke === null,
    ...(admittedSmoke ? { admittedSmoke } : {}),
    ...(process.env.STACK_BENCH_SUPERVISOR_STATE
      ? { supervisorState: process.env.STACK_BENCH_SUPERVISOR_STATE } : {}),
    image: process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE,
    resultsDir: resolve(args.out ?? stackBenchResultsRoot(ROOT)),
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
  if (process.env.STACK_BENCH_APPLIANCE === '1') {
    console.log('  sandbox    ... coding container is isolated from the controller and grading files');
  }
  let url = args.url ?? `http://localhost:${portsFor(track, args.backend, args.runIndex).vite}`;
  const runDir = resultsName(track, args.backend, args.runIndex);
  const runId = newRunId({ track: args.track, backend: args.backend, runIndex: args.runIndex });
  const artifactLabel = `${runDir}-${runId}`;
  // Default results never reuse a directory. The stable backend/run name is a
  // grouping directory only; every artifact beneath it belongs to one run id.
  args.out ??= join(stackBenchResultsRoot(ROOT), runDir, runId);
  if (!args.out) throw new Error('benchmark run has no results directory');
  const outputDir = args.out;
  mkdirSync(args.out, { recursive: true });
  if (existsSync(join(args.out, ARTIFACT_FILE.run))) {
    throw new Error(`refusing to reuse result directory containing ${ARTIFACT_FILE.run}: ${args.out}`);
  }
  if (preflight) writeArtifact(join(args.out, ARTIFACT_FILE.preflight), {
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
  const leasePath = join(runtimeDir, ARTIFACT_FILE.backendLease);
  const preparedLease = stackAdapter.lease.prepare({
    track,
    runIndex: args.runIndex,
    runtimeDir,
    serverUri: stackRuntime.lease.serverUri,
    env: process.env,
    helpers: { containerIdentity: runningContainerIdentity, dbName, moduleName },
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
  let activeRun: BenchmarkRunRecord | null = null;
  const recoveryPath = join(outputDir, ARTIFACT_FILE.recovery);
  const writeLeaseEvidence = (knownLease: BackendLease | null = null) => {
    const lease = knownLease ?? readBackendLease(leasePath,
      { token: initialLease.ownershipToken, backend: args.backend, runId });
    const out = join(outputDir, ARTIFACT_FILE.backendLease);
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
  const teardown = ({ reason = null, retainBackend = args.retainBackend }:
    { reason?: string | null; retainBackend?: boolean } = {}) => {
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
        activeRun.backendDiagnostics = captureApplicationDiagnostics(join(outputDir, 'backend.log'));
      } catch (error) {
        activeRun.backendDiagnostics = { captured: false,
          reason: errorMessage(error).split(/\r?\n/)[0] };
      }
    }
    let released = false;
    let cleanupError: unknown = null;
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
      reason: cleanupError === null ? reason ?? (released ? null : 'authenticated cleanup refused')
        : errorMessage(cleanupError) });
    if (activeRun) {
      activeRun.backendLease = evidence;
      activeRun.outcome ??= aggregateRunOutcome(activeRun.levels);
      writeRunJson(join(outputDir, ARTIFACT_FILE.run), activeRun);
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
    stackAdapter.lifecycle.activate({
      leasePath, leaseToken: initialLease.ownershipToken, lease: initialLease,
      ...stackRuntime.lifecycle,
    });
  } catch (error) {
    try { teardown({ reason: `backend activation failed: ${errorMessage(error)}`, retainBackend: false }); }
    catch (cleanupError) {
      console.error(`  activation cleanup quarantined: ${errorMessage(cleanupError).split(/\r?\n/)[0]}`);
    }
    throw error;
  }

  // Teardown stops only resources recorded in this run's lease.
  const interrupt = (signal: NodeJS.Signals, exitCode: number) => {
    console.log(`interrupted by ${signal} — stopping exact owned resources`);
    try { teardown({ reason: `interrupted by ${signal}` }); }
    catch (error) { console.error(`  cleanup quarantined: ${errorMessage(error).split(/\r?\n/)[0]}`); }
    process.exit(exitCode);
  };
  process.on('SIGINT', () => interrupt('SIGINT', 130));
  process.on('SIGTERM', () => interrupt('SIGTERM', 143));
  process.on('exit', () => {
    if (!tornDown) {
      try { teardown(); } catch (error) {
        console.error(`  cleanup failed: ${errorMessage(error).split('\n')[0]}`);
      }
    }
  });

  // Seed source only; the upgrade session installs its own dependencies.
  if (args.seedFrom) {
    const from = resolve(args.seedFrom);
    if (!existsSync(from)) { console.error(`--seed-from path does not exist: ${from}`); process.exit(2); }
    seedAppSource(from, appDir);
    if (args.progressionSeed) {
      const seeded = hashAppSource(appDir);
      if (seeded.sha256 !== args.progressionSeed.sourceSha256
        || seeded.files.length !== args.progressionSeed.sourceFiles) {
        throw new Error('extension source does not match its recorded identity');
      }
    }
    console.log(args.repairGrant
      ? `  restored L${args.levelList[0]} checkpoint from ${from} for a bounded repair continuation`
      : `  seeded from ${from} — level ${args.levelList[0]} will UPGRADE it, not rebuild`);
  }

  const started = Date.now();
  const run: BenchmarkRunRecord = { id: runId,
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
      version: args.progression ? DEPENDENCY_MODE_VERSION : '1.0.0' },
    track: args.track, backend: args.backend, model: args.model,
    pricing: args.pricing,
    guidance: args.guidance, condition: args.condition ?? null,
    skills: args.skills ?? [],
    runtime: { buildImage: process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE, url },
    selectionRequest: args.selectionRequest,
    featureCatalog: args.featureCatalog?.identity ?? null,
    dependencyPolicy: args.dependencyPolicy?.identity ?? null,
    ...(args.progressionOwner ? { progressionOwner: args.progressionOwner } : {}),
    ...(args.progressionSeed ? { progressionSeed: {
      fromDepth: args.progressionSeed.fromDepth,
      sourceSha256: args.progressionSeed.sourceSha256,
      sourceFiles: args.progressionSeed.sourceFiles,
      parent: structuredClone(args.progressionSeed.parent),
      validatedDepths: [],
    } } : {}),
    backendLease: publicBackendLease(readBackendLease(leasePath,
      { token: initialLease.ownershipToken, backend: args.backend, runId })),
    validation: {
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
      statePath: join(args.out, ARTIFACT_FILE.progressionState),
      runId,
      outputDir: args.out,
      appDir,
      track: args.track,
      backend: args.backend,
      identities: run.identities,
      recipeBindings: args.recipeBindings,
      resumeFrom: args.progressionResumeFrom ?? null,
      getRunArtifact: () => {
        writeRunJson(join(outputDir, ARTIFACT_FILE.run), run);
        return readArtifact(join(outputDir, ARTIFACT_FILE.run));
      },
      onState: status => {
        run.progressionStatus = status;
        writeRunJson(join(outputDir, ARTIFACT_FILE.run), run);
      },
    }) : null;
  const progressionStart = progressionExecution?.initialize() ?? null;
  if (progressionStart?.resumed) {
    const prior = progressionStart.priorRun;
    if (!prior) throw new Error('resumed dependency progression has no prior run artifact');
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
      stateSha256: progressionStart.stateSha256,
      action: progressionStart.action.type === 'terminal'
        ? { type: 'terminal' }
        : { type: progressionStart.action.type, level: progressionStart.action.level },
      inheritedLevels,
      priorTotals: prior.payload.totals ?? null,
    };
    run.progressionStatus = progressionStart.status;
    writeRunJson(join(args.out, ARTIFACT_FILE.run), run);
  }

  const bindProgressionAction = (level: number): ProgressionRecipeAction | null => {
    if (!progressionExecution) return null;
    const selected = progressionExecution.bind();
    if (!isProgressionWorkRecipeAction(selected)) return selected;
    if (!args.recipeTasks) throw new Error('recipe task map is unavailable');
    args.recipeTasks.set(level, {
      request: selected.grader.request,
      selection: selected.grader.selection,
      task: selected.grader.task,
      agentRequest: selected.agent.request,
      progressionAction: selected.action,
    });
    return selected;
  };

  const progressionBundles = new Map<number, GradeBundlePayload>();
  const recordProgressionGrade = (input: Parameters<NonNullable<typeof progressionExecution>['record']>[0]) => {
    if (input.bundle && input.selected && isProgressionWorkRecipeAction(input.selected)) {
      progressionBundles.set(input.selected.action.level, input.bundle as GradeBundlePayload);
    }
    return progressionExecution?.record(input) ?? null;
  };

  let runCostComplete = true;

  const runAgentForLevel = async (mode: AgentMode, level: number): Promise<ValidatedAgentResult> => {
    try {
      clearPrivateGradingEvidence(appDir);
      const result = await runAgent(args, agentAdapter, mode, level, appDir);
      if (result.costComplete !== true) runCostComplete = false;
      return result;
    } catch (error) {
      const reason = errorMessage(error).split(/\r?\n/)[0] ?? 'agent execution failed';
      run.outcome = { kind: 'harness_failure', phase: `agent-${mode}`,
        reason, appFailures: [], inconclusive: [], harnessFailures: [reason] };
      run.validation.ladder.stoppedAfterLevel = run.levels.at(-1)?.level ?? null;
      run.validation.ladder.blockedLevels = args.levelList.filter(candidate => candidate >= level);
      if (progressionExecution) {
        run.progressionStatus = progressionExecution.status();
        run.validation.ladder.completedLevels = [...new Set(requireProgressionState(progressionExecution.state).attempts
          .filter(attempt => attempt.outcome === 'conclusive')
          .map(attempt => attempt.level))];
      }
      finalizeRunTotals(run, started, { costComplete: false });
      run.completedAt = new Date().toISOString();
      writeRunJson(join(outputDir, ARTIFACT_FILE.run), run);
      throw error;
    }
  };

  // Stop before grading if a coding session read protected material or if the
  // audit itself failed. Keep the paid session and exact cost in the run artifact even
  // though no score may be used.
  const abortUnusableSession = (whichSession: string, audit: ContaminationAudit,
    levelRecord: UnknownRecord & { level: number },
    selected: ProgressionRecipeAction | null) => {
    const reason = audit.evidence.join('; ');
    const outcome: RunOutcome = { kind: audit.kind === 'harness_failure' ? 'harness_failure' : 'ungraded',
      phase: 'contamination-audit', reason,
      appFailures: [], inconclusive: [],
      harnessFailures: audit.kind === 'harness_failure' ? [reason] : [] };
    run.contaminated = audit.kind === 'contaminated';
    run.contamination = { evidence: audit.evidence, verdict: audit.verdict,
      detectedAt: whichSession };
    const record: RunLevelRecord = { ...levelRecord, error: reason, outcome,
      level: levelRecord.level, graded: false, score: null, max: null, selection: null };
    run.levels.push(record);
    if (progressionExecution) {
      recordProgressionGrade({ selected, bundle: null, level: levelRecord.level,
        failure: progressionFailure(outcome),
        repair: { status: 'ungraded', budgetRounds: 0, roundsUsed: 0,
          stopReason: audit.kind === 'harness_failure' ? 'audit-failure' : 'contaminated' } });
      run.progressionStatus = progressionExecution!.status();
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
    try { writeRunJson(join(outputDir, ARTIFACT_FILE.run), run); } catch { /* best effort */ }
    try { archiveTranscripts(appDir, artifactLabel); } catch { /* best effort */ }
    teardown();
    process.exit(4);
  };

  for (const level of args.levelList) {
    const t0 = Date.now();
    const continuing = Boolean(args.repairGrant);
    console.log(`\n================ ${args.backend} — level ${level} ================`);

    let progressionSelection = bindProgressionAction(level);
    if (progressionSelection?.action.type === 'terminal') break;
    if (args.dependencyPolicy?.definition.workSelection === 'all-at-once'
      && progressionSelection?.action.level !== level) continue;
    const applicationControl = materializeCodingOutput
      ? restartSpecFor(args, appDir, track) : null;
    if (args.seedThrough !== undefined && level <= args.seedThrough) {
      if (!progressionSelection || !isProgressionWorkRecipeAction(progressionSelection)
        || progressionSelection.action.level !== level || !args.seedFrom || !run.progressionSeed) {
        throw new Error(`extension cannot validate depth ${level}`);
      }
      let applicationFailure: RunOutcome | null = null;
      if (applicationControl) {
        try {
          await materializeAcceptedSource(args.seedFrom, appDir, applicationControl);
        } catch (error) {
          applicationFailure = materializationAppFailure(error);
        }
      } else {
        resetAppToSource(args.seedFrom, appDir);
      }
      const bundle = grade(args, appDir, url, `${args.backend}-extension-l${level}`,
        level, track, runId, { applicationFailure });
      const outcome = applicationFailure ?? classifyBundle(bundle);
      const next = recordProgressionGrade({ selected: progressionSelection, bundle, level,
        repair: { status: outcome.kind === 'passed' ? 'not-needed' : 'incomplete',
          budgetRounds: 0, roundsUsed: 0,
          stopReason: outcome.kind === 'passed' ? 'not-needed' : 'extension-validation-failed' } });
      const progressionState = requireProgressionState(progressionExecution?.state ?? null);
      const progressionAttempt = progressionState.attempts.at(-1) ?? null;
      const graded = levelGradeIsUsable(outcome, progressionAttempt);
      const passed = graded && outcome.kind === 'passed';
      const repair = { status: passed ? 'not-needed' as const : 'incomplete' as const,
        budgetRounds: 0, roundsUsed: 0,
        stopReason: passed ? 'not-needed' : 'extension-validation-failed' };
      let checkpoint = null;
      try {
        checkpoint = preserveLevelCheckpoint({ appDir, outputDir: args.out, runId,
          identities: run.identities, track: args.track, backend: args.backend, level,
          repair, outcome, selectionSha256: bundle?.selection?.sha256 ?? null });
      } catch (error) {
        throw new Error(`could not preserve extension depth ${level}: ${errorMessage(error)}`);
      }
      const source = hashAppSource(appDir);
      run.levels.push({ level, graded,
        score: graded ? bundle?.totals?.score ?? null : null,
        max: graded ? bundle?.totals?.max ?? null : null,
        selection: bundle?.selection ?? null,
        baseline: { kind: 'extension-validation', source: {
          sha256: source.sha256, files: source.files.length } },
        regression: bundle?.totals?.regression ?? null,
        contractPass: bundle?.totals?.contractPass ?? null,
        code: bundle?.code ?? null,
        repair,
        checkpoint,
        sessionTotals: summarizeSessions([]),
        costUsd: 0,
        fixRounds: 0,
        durationSec: Math.round((Date.now() - t0) / 1000),
        outcome });
      if (progressionAttempt?.outcome === 'conclusive') {
        run.validation.ladder.completedLevels.push(level);
      }
      run.progressionStatus = progressionExecution!.status();
      if (passed) run.progressionSeed.validatedDepths.push(level);
      if (!passed) {
        run.validation.ladder.stoppedAfterLevel = level > 1 ? level - 1 : null;
        run.validation.ladder.blockedLevels = args.levelList.filter(candidate => candidate >= level);
        writeRunJson(join(args.out, ARTIFACT_FILE.run), run);
        break;
      }
      writeRunJson(join(args.out, ARTIFACT_FILE.run), run);
      if (!next || next.type === 'terminal' || next.level === undefined || next.level <= level) {
        throw new Error(`extension did not advance after depth ${level}`);
      }
      continue;
    }
    const resumedRepair = progressionStart?.resumed === true
      && progressionStart.action.type === 'repair';
    const priorRepairRounds = resumedRepair
      ? progressionStart.priorRun?.payload.levels?.find(item => item.level === level)
        ?.repair?.roundsUsed ?? 0
      : 0;
    const repairBudgetFor = (selected: ProgressionRecipeAction | null,
      completedRepairRounds: number, initialGradePending = false) => selected
      && isProgressionWorkRecipeAction(selected)
      ? dependencyRepairBudget(selected.action, completedRepairRounds, initialGradePending)
      : args.fixRounds;
    const levelStrikeNodeIds = new Set(progressionSelection?.action.strikes.nodes
      .map(node => node.nodeId) ?? []);
    let progressionRepairBudgetRounds = repairBudgetFor(
      progressionSelection, priorRepairRounds, !resumedRepair);
    const trackProgressionBudget = (selected: ProgressionRecipeAction | null,
      completedRepairRounds: number) => {
      if (!selected || !isProgressionWorkRecipeAction(selected)) return;
      selected.action.strikes.nodes.forEach(node => levelStrikeNodeIds.add(node.nodeId));
      progressionRepairBudgetRounds = Math.max(
        progressionRepairBudgetRounds, repairBudgetFor(selected, completedRepairRounds));
    };
    if (resumedRepair) {
      sh('node', [join(ROOT, 'dist', 'commands', 'report-bugs.js'), '--app', appDir,
        '--history-json', '[]', '--archive', join(args.out, 'repair-reports',
          `bug-report-l${level}-resume.md`), ...repairReportArgs(progressionSelection)],
      { stdio: 'pipe' });
    }

    const firstMode = resumedRepair ? 'fix'
      : continuing ? 'resume' : args.seedFrom ? 'upgrade' : 'build';
    const build = await runAgentForLevel(
      resumedRepair || run.levels.length === 0 ? firstMode : 'upgrade', level);
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
    // Record the session setup needed to compare runs.
    run.setup ??= build.setup;
    if (continuing) {
      requireContinuation(run).resumeSetup = {
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
      console.log(`  ABORTED: ${buildFailure.reason}. Details will be kept in ${join(args.out, ARTIFACT_FILE.run)}`);
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
    const gradeAcceptedSource = async (sourcePath: string,
      label: string): Promise<GradeBundlePayload | null> => {
      let failure: RunOutcome | null = null;
      if (applicationControl) {
        try {
          await materializeAcceptedSource(sourcePath, appDir, applicationControl);
        } catch (error) {
          failure = materializationAppFailure(error);
        }
      } else {
        resetAppToSource(sourcePath, appDir);
      }
      return grade(args, appDir, url, label, level, track, runId,
        { applicationFailure: failure });
    };
    const restoreAcceptedRepair = async (sourcePath: string, gradingPath: string): Promise<void> => {
      if (applicationControl) await materializeAcceptedSource(sourcePath, appDir, applicationControl);
      else resetAppToSource(sourcePath, appDir);
      restorePrivateGradingEvidence(appDir, gradingPath);
    };
    if (continuing) {
      // A resume may restore runtime state but cannot change checkpoint source.
      const resumed = hashAppSource(appDir);
      const repairGrant = args.repairGrant;
      if (!repairGrant) throw new Error('repair continuation has no grant');
      if (resumed.sha256 !== repairGrant.checkpoint.payload.source.sha256
        || resumed.files.length !== repairGrant.checkpoint.payload.source.files) {
        throw new Error('resume setup changed the parent checkpoint source');
      }
      const continuation = requireContinuation(run);
      if (!continuation.resumeSetup) throw new Error('repair continuation did not record resume setup');
      continuation.resumeSetup.sourceVerified = true;
    }
    if (args.referenceMutationOnly) {
      run.levels.push({ level, score: null, max: null, graded: false, contractPass: null,
        selection: null,
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
    let materializationOutcome: RunOutcome | null = null;
    try {
      const liveSource = hashAppSource(appDir);
      snapshotSource(appDir, firstBuildPath);
      const preservedSource = hashDirectory(firstBuildPath);
      if (liveSource.sha256 !== preservedSource.sha256) {
        throw new Error('preserved first-build source differs from the live application source');
      }
      firstBuildSource = { sha256: liveSource.sha256, files: liveSource.files.length };
      if (applicationControl) {
        await materializeAcceptedSource(firstBuildPath, appDir, applicationControl);
      }
      console.log(`  kept the ${continuing ? 'continuation baseline' : 'unaided'} source at ${firstBuildPath}`);
    } catch (error) {
      if (firstBuildSource) {
        materializationOutcome = materializationAppFailure(error);
      }
      console.log(materializationOutcome
        ? `  !! ${materializationOutcome.reason}`
        : `  !! could not bind the first-build source: ${errorMessage(error).split('\n')[0]}`);
    }
    const firstBuildLabel = `${args.backend}-l${level}`;
    let bundle = firstBuildSource
      ? grade(args, appDir, url, firstBuildLabel, level, track, runId,
        { applicationFailure: materializationOutcome }) : null;

    // What the model built BEFORE being handed the answers. Every backend can
    // reach the same total given enough fix rounds, so the post-fix score stops
    // discriminating — what it got right unaided is the comparison that survives.
    const firstBuild: FirstBuildRecord = {
      score: bundle?.totals?.score ?? null,
      max: bundle?.totals?.max ?? null,
      regression: bundle?.totals?.regression ?? null,
      contractPass: bundle?.totals?.contractPass ?? null,
      outcome: materializationOutcome ?? sourceBoundFirstBuildOutcome(bundle, firstBuildSource),
      source: firstBuildSource,
      missed: Object.values(bundle?.suites ?? {}).flatMap(s =>
        (s?.features ?? []).flatMap(f =>
          (f.criteria ?? []).filter(c => !evidencePassed(criterionEvidence(c)))
            .map(c => `${f.name}/${c.id}`))),
    };

    if (continuing) {
      const repairGrant = args.repairGrant;
      if (!repairGrant) throw new Error('repair continuation has no grant');
      if (firstBuild.score === null || firstBuild.max === null || firstBuildSource === null) {
        throw new Error('repair continuation did not produce a source-bound baseline score');
      }
      const reproduction = compareRepairBaseline(repairGrant.level, {
        score: firstBuild.score,
        max: firstBuild.max,
        selectionSha256: bundle?.selection?.sha256 ?? null,
        sourceSha256: firstBuildSource.sha256,
        expectedSourceSha256: repairGrant.checkpoint.payload.source.sha256,
        outcome: repairOutcome(firstBuild.outcome),
      });
      requireContinuation(run).baseline = {
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

    const selectedObservedChecks = checksForGrade(args.recipeTasks?.get(level), 'observed');
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
        selectionSha256: args.recipeTasks?.get(level)?.selection.sha256 ?? null,
        selectedChecks: selectedObservedChecks.map(check => check.stableKey),
        reportedChecks: observationBundle?.selection?.reportedChecks ?? [],
        passedPoints: observationBundle?.totals?.score ?? null,
        observedPoints: observationBundle?.totals?.max ?? null,
        scoreContribution: false,
        repairVisible: false,
        artifact: observationBundle
          ? `first-build-l${level}-observed/${ARTIFACT_FILE.gradeBundle}` : null,
        outcome: observationOutcome,
      };
    }

    // Preserve the first source and scored grading before repair overwrites the
    // app. Observed evidence remains in its own source-bound result directory.
    const acceptedGradingDirectory = continuing
      ? `baseline-l${level}-grading` : `first-build-l${level}-grading`;
    try {
      const gradingFrom = join(appDir, 'stack-bench');
      if (existsSync(gradingFrom)) {
        cpSync(gradingFrom, join(args.out, acceptedGradingDirectory), {
          recursive: true,
          filter: src => !/[\\/]media([\\/]|$)/.test(src),
        });
        console.log(`  kept the ${continuing ? 'continuation baseline' : 'unaided'} grading at ${join(args.out, acceptedGradingDirectory)}`);
      }
    } catch (e) {
      // Never worth losing a run over: the score is already recorded.
      console.log(`  !! could not keep the first build: ${errorMessage(e).split('\n')[0]}`);
    }

    let fixRounds = resumedRepair ? 1 : 0;
    let fixCost = resumedRepair ? build.costUsd : 0;
    const fixSessions = resumedRepair
      ? [runSessionRecord(build, priorRepairRounds + 1)] : [];
    const repairHistory: ReturnType<typeof repairHistoryEntry>[] = [];
    let regressed = false;
    let repairStopReason: string | null = null;
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
    const initialProgressionAttempt = args.progression
      ? requireProgressionState(progressionExecution?.state ?? null).attempts.at(-1) ?? null
      : null;
    const initialGradeUsable = levelGradeIsUsable(initialBundleOutcome,
      initialProgressionAttempt);
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
      || progressionNext?.type === 'repair';
    const recordRepairProgression = ({ failure = null }: { failure?: ProgressionFailure | null } = {}) => {
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
    const writeRepairReport = (): 0 | 3 | 4 => {
      try {
        sh('node', [join(ROOT, 'dist', 'commands', 'report-bugs.js'), '--app', appDir,
          '--history-json', JSON.stringify(repairHistory),
          '--archive', join(outputDir, 'repair-reports',
            `bug-report-l${level}-round${fixRounds + 1}.md`),
          ...repairReportArgs(progressionSelection)], { stdio: 'pipe' });
        return 0;
      } catch (error) {
        const failure = commandFailure(error);
        if (failure.status === 3 || failure.status === 4) return failure.status;
        throw failure;
      }
    };
    const recordMissingRepairFeedback = (status: 3 | 4): void => {
      repairStopReason = 'no-actionable-findings';
      if (!args.progression) return;
      const reason = status === 3
        ? 'selected repair checks contain no failures'
        : 'selected repair checks produced no actionable findings';
      const failure: RunOutcome = {
        kind: 'harness_failure', phase: 'repair-report', reason,
        appFailures: [], inconclusive: [], harnessFailures: [reason],
      };
      bundle = { outcome: failure };
      recordRepairProgression({ failure: progressionFailure(failure) });
    };

    // Hand back findings and let the agent fix, until clean or out of rounds.
    while (levelGradeIsUsable(classifyBundle(bundle), args.progression
      ? requireProgressionState(progressionExecution?.state ?? null).attempts.at(-1) ?? null
      : null) && progressionMayRepair()
      && (args.progression || fixRounds < args.fixRounds)) {
      let reportReady = false;
      if (args.progression) {
        progressionSelection = bindProgressionAction(level);
        trackProgressionBudget(progressionSelection, priorRepairRounds + fixRounds);
        if (progressionSelection && isProgressionWorkRecipeAction(progressionSelection)
          && bundle?.selection?.sha256 !== progressionSelection.grader.selectionSha256) {
          const sequence = requireProgressionState(progressionExecution?.state ?? null).attempts.length + 1;
          bundle = grade(args, appDir, url,
            `${args.backend}-l${level}-repair-baseline${sequence}`, level, track, runId);
          const refreshOutcome = classifyBundle(bundle);
          const refreshUsable = levelGradeIsUsable(refreshOutcome);
          if (!refreshUsable) {
            recordProgressionGrade({
              selected: progressionSelection,
              bundle,
              level,
              repair: {
                status: 'ungraded',
                budgetRounds: progressionRepairBudgetRounds,
                roundsUsed: priorRepairRounds + fixRounds,
                stopReason: 'refresh-grading-failed',
              },
            });
            repairStopReason = 'refresh-grading-failed';
            break;
          }
          const refreshReportStatus = writeRepairReport();
          if (refreshReportStatus === 3) {
            recordRepairProgression();
            continue;
          }
          if (refreshReportStatus === 4) {
            recordMissingRepairFeedback(refreshReportStatus);
            break;
          }
          reportReady = true;
        }
      }
      const reportStatus = reportReady ? 0 : writeRepairReport();
      if (reportStatus !== 0) {
        recordMissingRepairFeedback(reportStatus);
        break;
      }

      const before = bundle?.totals?.score ?? 0;
      const beforeMax = bundle?.totals?.max ?? 0;
      const beforeBundle = bundle;
      // Keep the accepted source outside paths visible to the coding session.
      const snapshot = join(tmpdir(), `stack-bench-snapshot-${args.backend}-${args.track}-run${args.runIndex}-l${level}`);
      const gradingSnapshot = `${snapshot}-grading`;
      const acceptedSource = hashAppSource(appDir);
      snapshotSource(appDir, snapshot);
      rmSync(gradingSnapshot, { recursive: true, force: true });
      if (existsSync(join(appDir, 'stack-bench'))) {
        cpSync(join(appDir, 'stack-bench'), gradingSnapshot, { recursive: true });
      }
      const cleanupRepairSnapshots = () => {
        rmSync(snapshot, { recursive: true, force: true });
        rmSync(gradingSnapshot, { recursive: true, force: true });
      };
      try {
      fixRounds += 1;
      const displayedRepairBudget = args.progression
        ? progressionRepairBudgetRounds
        : args.fixRounds;
      if (args.dependencyPolicy?.definition.repairSelection === 'feature'
        && progressionSelection && isProgressionWorkRecipeAction(progressionSelection)) {
        const [strike] = progressionSelection.action.strikes.nodes;
        const node = requireProgressionState(progressionExecution?.state ?? null).definition.nodes
          .find(candidate => candidate.id === strike?.nodeId);
        if (!strike || !node) {
          throw new Error('feature repair has no selected feature');
        }
        console.log(`--- feature repair ${strike.used}/${strike.budget - 1}: ${node.title} ---`);
      } else {
        console.log(`--- repair round ${fixRounds}/${displayedRepairBudget} ---`);
      }
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
        recordRepairProgression({ failure: progressionFailure(fixFailure) });
        break;
      }

      // Reject contaminated repairs before spending time on grading.
      const fixLeak = auditContamination(appDir);
      if (fixLeak) {
        const buildSession = runSessionRecord(build);
        const sessions = resumedRepair ? fixSessions : [buildSession, ...fixSessions];
        const sessionTotals = summarizeSessions(sessions);
        cleanupRepairSnapshots();
        abortUnusableSession(`repair round ${fixRounds}`, fixLeak, {
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
      if (hashAppSource(appDir).sha256 === acceptedSource.sha256) {
        clearPrivateGradingEvidence(appDir);
        if (existsSync(gradingSnapshot)) {
          cpSync(gradingSnapshot, join(appDir, 'stack-bench'), { recursive: true });
        }
        const reason = 'repair made no source change';
        console.log(`    ${reason}; ${args.progression
          ? 'counting the failed attempt'
          : 'pausing before another paid round'}`);
        repairHistory.push(repairHistoryEntry(fixRounds, beforeBundle, beforeBundle, reason));
        if (args.progression) {
          if (!recordRepairProgression()) break;
          continue;
        }
        repairStopReason = 'no-source-change';
        break;
      }
      const repairedSource = `${snapshot}-accepted`;
      snapshotSource(appDir, repairedSource);
      try {
        bundle = await gradeAcceptedSource(repairedSource,
          `${args.backend}-l${level}-fix${fixRounds}`);
      } finally {
        rmSync(repairedSource, { recursive: true, force: true });
      }

      const after = bundle?.totals?.score ?? 0;
      const afterMax = bundle?.totals?.max ?? 0;
      // Lost or inconclusive evidence cannot hide a repair regression.
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
        await restoreAcceptedRepair(snapshot, gradingSnapshot);
        bundle = beforeBundle;
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
        await restoreAcceptedRepair(snapshot, gradingSnapshot);
        bundle = beforeBundle;
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
          + (remaining > 0 ? `${remaining} repair round(s) remain` : 'repair budget exhausted'));
      }
      repairHistory.push(repairHistoryEntry(fixRounds, beforeBundle, bundle,
        shared.after === shared.before ? 'kept with no score gain' : 'kept'));
      if (!recordRepairProgression()) break;
      if (pauseForRepeatedFindings()) break;
      } finally {
        cleanupRepairSnapshots();
      }
    }

    // Missing grade evidence is not a zero score.
    const progressionState = progressionExecution
      ? requireProgressionState(progressionExecution.state) : null;
    const progressionAttempt = progressionState
      ? progressionState.attempts.findLast(attempt => attempt.level === level) ?? null
      : null;
    const levelBundle = progressionAttempt?.outcome === 'inconclusive'
      ? bundle : progressionBundles.get(level) ?? bundle;
    const finalBundleOutcome = classifyBundle(levelBundle);
    // Progression uses stricter evidence rules than a regular scored bundle.
    // Store one answer when a selected check is not measured: the raw bundle
    // remains available for diagnosis, but the level is not a usable grade.
    const graded = levelGradeIsUsable(finalBundleOutcome,
      args.progression ? progressionAttempt : null);
    const finalTotals = graded ? levelBundle?.totals ?? null : null;
    const nodeStrikes = progressionState
      ? dependencyStrikeRecords(progressionState, level, levelStrikeNodeIds)
      : null;
    const repairBudgetRounds = progressionExecution
      ? Math.max(priorRepairRounds + fixRounds, progressionRepairBudgetRounds)
      : args.fixRounds;
    const repairBudgetExhausted = progressionExecution
      ? finalBundleOutcome.kind === 'app_failure'
        && progressionNext?.type !== 'repair'
      : fixRounds >= args.fixRounds;
    const repairStatus: RepairStatus = repairStopReason === 'no-source-change' ? 'incomplete'
      : !graded ? 'ungraded'
      : finalBundleOutcome.kind === 'passed' ? (fixRounds > 0 ? 'corrected' : 'not-needed')
        : repairBudgetExhausted ? 'budget-exhausted' : 'incomplete';
    const stopReasons: Record<RepairStatus, string | null> = {
      'not-needed': 'not-needed',
      corrected: 'passed',
      'budget-exhausted': 'budget-exhausted',
      incomplete: null,
      ungraded: null,
    };
    const stopReason = repairStopReason ?? stopReasons[repairStatus];
    const repair = {
      status: repairStatus,
      budgetRounds: repairBudgetRounds,
      roundsUsed: priorRepairRounds + fixRounds,
      ...(!args.progression ? { stallLimitRounds: args.maxStalledRepairs } : {}),
      stopReason,
      ...(nodeStrikes ? {
        strikeScope: progressionState?.definition.strikePolicy,
        nodeStrikes,
      } : {}),
    };
    const latestProgressionAttempt = progressionState?.attempts.at(-1) ?? null;
    if (progressionState && latestProgressionAttempt && latestProgressionAttempt.level !== level) {
      const prior = run.levels.find(item => item.level === latestProgressionAttempt.level);
      const latestBundle = progressionBundles.get(latestProgressionAttempt.level);
      if (prior && latestBundle) {
        const latestOutcome = classifyBundle(latestBundle);
        const latestGraded = levelGradeIsUsable(latestOutcome, latestProgressionAttempt);
        prior.graded = latestGraded;
        prior.score = latestGraded ? latestBundle.totals?.score ?? null : null;
        prior.max = latestGraded ? latestBundle.totals?.max ?? null : null;
        prior.regression = latestBundle.totals?.regression ?? null;
        prior.selection = latestBundle.selection ?? null;
        prior.contractPass = latestBundle.totals?.contractPass ?? null;
        prior.code = latestBundle.code ?? null;
        prior.repair = { ...repair,
          nodeStrikes: dependencyStrikeRecords(
            progressionState, latestProgressionAttempt.level, levelStrikeNodeIds),
        };
        prior.stalled = repairStatus === 'budget-exhausted';
        prior.outcome = latestOutcome;
      }
    }
    if (continuing) {
      const continuation = requireContinuation(run);
      continuation.cumulativeRoundsAfter = continuation.cumulativeRoundsBefore + fixRounds;
    }
    let checkpoint = null;
    if (!latestProgressionAttempt || latestProgressionAttempt.level === level) {
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
          selectionSha256: levelBundle?.selection?.sha256 ?? null,
        });
        console.log(`  kept the L${level} source checkpoint at ${join(args.out, checkpoint.directory)}`);
      } catch (error) {
        console.log(`  !! could not keep the L${level} source checkpoint: ${errorMessage(error).split('\n')[0]}`);
      }
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
      score: finalTotals?.score ?? null,
      max: finalTotals?.max ?? null,
      // Preserve earlier-level guarantees in the durable result.
      regression: levelBundle?.totals?.regression ?? null,
      selection: levelBundle?.selection ?? null,
      ...(resumedRepair
        ? { resumedRepair: firstBuild }
        : continuing
        ? { baseline: firstBuild, resumeCostUsd: build.costUsd, resumeSession: buildSession }
        : { firstBuild, buildCostUsd: build.costUsd, buildSession }),
      contractPass: levelBundle?.totals?.contractPass ?? null,
      code: levelBundle?.code ?? null,
      fixCostUsd: addCostUsd(fixCost),
      fixSessions,
      repairHistory,
      sessionTotals,
      tokens: sessionTotals.tokens,
      usage: sessionTotals.usage,
      turns: sessionTotals.turns,
      promptBytes: sessionTotals.promptBytes,
      tokensPerTurn: sessionTotals.turns
        ? Math.round(sessionTotals.tokens / sessionTotals.turns) : null,
      // Record actual reasoning because the provider default is not pinned.
      thinking: sessionTotals.thinking,
      fixRounds,
      ...(resumedRepair ? { priorRepairRounds,
        cumulativeFixRounds: priorRepairRounds + fixRounds } : {}),
      repair,
      checkpoint,
      // Keep the summary flag derived from the typed status so the two cannot drift.
      stalled: repairStatus === 'budget-exhausted'
        || ['repeated-findings', 'no-source-change'].includes(repairStopReason ?? ''),
      regressed,
      outcome: finalBundleOutcome,
      durationSec: Math.round((Date.now() - t0) / 1000),
    });
    if (!args.progression || requireProgressionState(progressionExecution?.state ?? null).attempts
      .some(attempt => attempt.level === level && attempt.outcome === 'conclusive')) {
      run.validation.ladder.completedLevels.push(level);
    }
    writeRunJson(join(args.out, ARTIFACT_FILE.run), run);
    const blockedLevels = args.levelList.filter(candidate => candidate > level);
    if (args.progression) {
      const progressionState = requireProgressionState(progressionExecution?.state ?? null);
      if (progressionState.phase === 'terminal') {
        if (blockedLevels.length) run.validation.ladder.stoppedAfterLevel = level;
        run.validation.ladder.blockedLevels = blockedLevels;
        writeRunJson(join(args.out, ARTIFACT_FILE.run), run);
        break;
      }
      if (progressionState.level <= level) {
        if (progressionState.attempts.at(-1)?.outcome === 'inconclusive') {
          run.validation.ladder.stoppedAfterLevel = level;
          run.validation.ladder.blockedLevels = [level, ...blockedLevels];
          writeRunJson(join(args.out, ARTIFACT_FILE.run), run);
          break;
        }
        throw new Error(`dependency progression did not leave L${level} after its strike budget`);
      }
      continue;
    }
    if (blockedLevels.length && !ladderMayAdvance(finalBundleOutcome)) {
      run.validation.ladder.stoppedAfterLevel = level;
      run.validation.ladder.blockedLevels = blockedLevels;
      writeRunJson(join(args.out, ARTIFACT_FILE.run), run);
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
      const baselineBundle = pristineMutationBaselinePath(args);
      if (baselineBundle) args.mutationBaselineBundle = baselineBundle;
      else delete args.mutationBaselineBundle;
      run.mutationControl = runMutationControl(args, appDir, url, track,
        run.setup?.isolation?.imageId ?? null);
    } else {
      console.log(`  skipped: pristine outcome is ${pristineOutcome.kind}`);
      run.mutationControl = { ok: false, skipped: true,
        outcome: { kind: pristineOutcome.kind, phase: 'mutation-control-prerequisite',
          reason: `pristine outcome is ${pristineOutcome.kind}` } };
    }
    writeRunJson(join(args.out, ARTIFACT_FILE.run), run);
  }

  // Record a final transcript audit in addition to the per-session hard gates.
  // The same retry and diagnostic path is used at both gates.
  let finalAuditFailure = null;
  const finalAudit = auditContamination(appDir);
  if (!finalAudit) {
    run.contaminated = false;
    run.contamination = { evidence: 'no agent access to private benchmark files detected',
      verdict: 'private-access audit passed' };
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
  try { archiveTranscripts(appDir, artifactLabel); }
  catch { console.log('  (transcript archiving failed — evidence is on a 30-day timer)'); }

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

  if (finalPackageEvidenceRequired(run.outcome, run.levels)) {
    try {
      preserveFinalPackageEvidence({ appDir, outputDir });
      console.log(`  source kept at ${join(outputDir, 'source')}`);
      console.log(`  grading detail kept at ${join(outputDir, 'grading')}`);
    } catch (error) {
      const reason = errorMessage(error).split(/\r?\n/)[0] ?? 'evidence preservation failed';
      run.outcome = { kind: 'harness_failure', phase: 'evidence-preservation', reason,
        appFailures: [], inconclusive: [], harnessFailures: [reason] };
      console.log(`  !! ${reason}`);
    }
  }

  finalizeRunTotals(run, started, { costComplete: runCostComplete });
  if (args.repairGrant) {
    const continuation = requireContinuation(run);
    const totals = requireRunTotals(run);
    continuation.cumulativeCostAfterUsd = addCostUsd(continuation.cumulativeCostBeforeUsd, totals.costUsd);
    continuation.cumulativeDurationAfterSec = continuation.cumulativeDurationBeforeSec + totals.durationSec;
  }
  run.completedAt = new Date().toISOString();
  writeRunJson(join(args.out, ARTIFACT_FILE.run), run);

  console.log(`\n================ ${args.backend} summary ================`);
  for (const l of run.levels) {
    console.log(`  ${formatLevelSummary(l)}`);
  }
  const totals = requireRunTotals(run);
  console.log(`  TOTAL ${totals.score}/${totals.max}  ` +
    `$${totals.costUsd}  ${totals.fixRounds} repair round(s)  ${totals.durationSec}s`);
  console.log(`  ${join(outputDir, ARTIFACT_FILE.run)}`);

  teardown();

  // Remove only the temporary directory created by this run.
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
    console.error(error instanceof Error ? error.stack ?? error.message : errorMessage(error));
    try { emergencyTeardown?.(); }
    catch (cleanupError) {
      console.error(`cleanup after failure also failed: ${errorMessage(cleanupError).split(/\r?\n/)[0]}`);
    }
    process.exitCode = 1;
  });
}
