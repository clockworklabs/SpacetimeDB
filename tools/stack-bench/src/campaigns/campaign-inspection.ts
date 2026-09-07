import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';

import { classifyCampaignExecution, readCampaignState } from './campaign-scheduler.js';
import type { CampaignAttemptState, CampaignExecution } from './campaign-scheduler.js';
import { ARTIFACT_FILE, readArtifactPayload } from '../evidence/artifacts.js';
import { progressionEngine } from '../progression/progression-engine.js';
import { readProgressionState } from '../progression/progression-state.js';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../progression/progression-definition.js';
import type { DependencyEvent, DependencyState } from '../progression/dependency-mode.js';
import { campaignCohortKey, campaignComparisonKey, executionSpend } from './campaign-report.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import type { RunCheckpoint } from '../evidence/run-checkpoints.js';
import { campaignGradingQualification, campaignProgressionOwner } from './campaign-compiler.js';
import type { CampaignAttemptPlan, CompiledCampaignPlan } from './campaign-compiler.js';
import type { DependencyPromptSelection } from '../progression/dependency-mode.js';
import { validateCampaignRun } from './campaign-run-validation.js';
import { runCostEvidence, sessionCostEvidence, type CostEvidence } from '../evidence/cost-proof.js';
import type { RunSessionRecord } from '../evidence/benchmark-run.js';
import type { CheckCompletion } from '../evidence/check-completion.js';

interface Score {
  score: number;
  max: number;
}

interface CheckFailure {
  stableKey?: string;
  description?: string;
}

interface RunOutcome {
  kind?: string;
  phase?: string;
  reason?: string | null;
  appFailures?: Array<string | CheckFailure>;
  inconclusive?: unknown[];
}

interface RunNodeRepairs {
  used?: number;
}

interface RunLevel {
  buildSessions?: RunSessionRecord[];
  repairSessions?: RunSessionRecord[];
  resumeSession?: RunSessionRecord;
  level: number;
  score: number;
  max: number;
  graded?: boolean;
  durationSec?: number | null;
  buildCostUsd?: number | null;
  repairCostUsd?: number | null;
  firstBuild?: Score & { outcome?: RunOutcome; missed?: Array<string | CheckFailure> };
  repair?: { used?: number; status?: string | null; nodeRepairs?: RunNodeRepairs[] };
  regression?: { score?: number | null; max?: number | null } | null;
  resumedRepair?: unknown;
  outcome?: RunOutcome;
  missed?: Array<string | CheckFailure>;
}

interface BenchmarkRunPayload {
  checkpoints?: RunCheckpoint[];
  progressionStatus?: { phase?: string; score?: { completion?: CheckCompletion } };
  outcome?: RunOutcome;
  totals?: Score & {
    costUsd?: number | null;
    costComplete?: boolean | null;
    durationSec?: number | null;
  };
  backendLease?: { state?: string | null };
  levels?: RunLevel[];
}

export interface CampaignRunLevelResult {
  cost: CostEvidence;
  level: number;
  firstScore: Score | null;
  firstAbort: { phase: string; reason: string | null } | null;
  finalScore: Score | null;
  used: number;
  repairStatus: string | null;
  outcome: string | null;
  durationSec: number | null;
  costUsd: number | null;
  failures: string[];
  // Checks that passed and then failed: the regression suite's missing points.
  regressions: number;
  repairs: { used: number } | null;
  continued: boolean;
}

export interface CampaignRunResult {
  measurementClassification?: ReturnType<typeof classifyCampaignExecution>;
  completion?: CheckCompletion | null;
  cost?: CostEvidence;
  unreadable?: string;
  outcome?: string;
  outcomePhase?: string | null;
  outcomeReason?: string | null;
  score?: Score | null;
  costUsd?: number | null;
  costComplete?: boolean | null;
  durationSec?: number | null;
  cleanup?: string | null;
  levels?: CampaignRunLevelResult[];
}

export function firstGradeAbort(firstBuild: (Score & { outcome?: RunOutcome }) | null | undefined): {
  phase: string;
  reason: string | null;
} | null {
  const outcome = firstBuild?.outcome;
  if (!outcome || outcome.kind === 'passed' || !outcome.phase || outcome.phase === 'grading') {
    return null;
  }
  return { phase: outcome.phase, reason: outcome.reason ?? null };
}

function readCampaignRunResult(path: string, plan: CompiledCampaignPlan,
  attempt: CampaignAttemptPlan, execution: CampaignExecution): CampaignRunResult | null {
  if (!existsSync(path)) return null;
  try {
    const run = readArtifactPayload<BenchmarkRunPayload>(path, { expectedKind: 'benchmark_run' });
    validateCampaignRun(plan, attempt, run, { resultDir: dirname(path) });
    const cost = runCostEvidence(run, 'execution');
    const incompleteMeasurement = (run.progressionStatus !== undefined
      && run.progressionStatus.phase !== 'terminal') || (run.outcome?.inconclusive?.length ?? 0) > 0;
    return {
      ...(execution.status === 'completed' && incompleteMeasurement
        ? { measurementClassification: classifyCampaignExecution({ exitCode: execution.exitCode, run }) } : {}),
      completion: run.progressionStatus?.score?.completion ?? run.checkpoints?.findLast(point => point.accepted)?.completion ?? null,
      outcome: run.outcome?.kind ?? 'ungraded',
      outcomePhase: run.outcome?.phase ?? null,
      outcomeReason: run.outcome?.reason ?? null,
      score: run.totals ? { score: run.totals.score, max: run.totals.max } : null,
      cost,
      costUsd: cost.status === 'exact' ? cost.costUsd : null,
      costComplete: cost.status !== 'unknown',
      durationSec: run.totals?.durationSec ?? null,
      cleanup: run.backendLease?.state ?? null,
      levels: (run.levels ?? []).map(level => {
        const sessions = [...(level.buildSessions ?? []), ...(level.repairSessions ?? []),
          ...(level.resumeSession ? [level.resumeSession] : [])];
        const cost = sessions.length ? sessionCostEvidence(sessions)
          : { status: 'unknown' as const, costUsd: null };
        return {
        cost,
        level: level.level,
        firstScore: level.firstBuild
          ? { score: level.firstBuild.score, max: level.firstBuild.max } : null,
        firstAbort: firstGradeAbort(level.firstBuild),
        finalScore: level.graded ? { score: level.score, max: level.max } : null,
        used: level.repair?.used ?? 0,
        repairStatus: level.repair?.status ?? null,
        outcome: level.outcome?.kind ?? null,
        durationSec: level.durationSec ?? null,
        costUsd: cost.status === 'exact' ? cost.costUsd : null,
        regressions: level.regression
          ? Math.max(0, (level.regression.max ?? 0) - (level.regression.score ?? 0)) : 0,
        repairs: level.repair?.nodeRepairs
          ? { used: level.repair.nodeRepairs.reduce((total, node) => total + (node.used ?? 0), 0) }
          : null,
        continued: level.resumedRepair !== undefined && level.resumedRepair !== null,
        failures: (level.outcome?.appFailures
          ?? (level.graded ? [] : level.missed ?? level.firstBuild?.missed ?? [])).map(item =>
          typeof item === 'string' ? item : item.stableKey ?? item.description ?? 'Failed check'),
      }; }),
    };
  } catch (error) {
    return { unreadable: error instanceof Error ? error.message : String(error) };
  }
}

export function campaignFacts(plan: CompiledCampaignPlan) {
  const requested = plan.attempts[0]?.condition.requested.levels;
  return {
    mode: plan.definition.mode?.id ?? 'sequential',
    grading: campaignGradingQualification(plan),
    agents: plan.agents.map(agent => ({
      adapter: agent.adapter,
      version: agent.adapterVersion,
      model: agent.model,
    })),
    recipes: (requested ?? []).map(level => ({
      level: level.level,
      id: level.recipe?.id ?? null,
      contentSha256: level.recipe?.contentSha256 ?? null,
    })),
    runtime: {
      controllerImage: plan.definition.runtime.controllerImage ?? null,
      buildImage: plan.definition.runtime.buildImage ?? null,
    },
  };
}

export interface DependencyProgressNode {
  id: string;
  title: string;
  depth: number;
  questline: string;
  dependencies: string[];
  blockedBy: string[];
  status: string;
  repairs: { used: number };
  exhaustionReason: unknown;
  checks: { passed: number; failed: number; total: number };
}

export interface DependencyProgressScore {
  completion?: CheckCompletion;
  status?: string;
  questlineAveragePercentage?: number | null;
  uniqueChecks?: { percentage?: number | null; availablePoints?: number; passedPoints?: number };
  questlines?: Array<{
    id: string;
    percentage?: number | null;
    availablePoints?: number;
    passedPoints?: number;
  }>;
}

export interface DependencyProgressQuestline {
  id: string;
  title: string;
  nodes: string[];
}

export interface DependencyProgressEvidence {
  attempt: number;
  depth: number;
  outcome: string;
  runId: string | null;
  sourceSha256: string | null;
  selectionSha256: string | null;
}

export interface DependencyProgress {
  phase: string;
  activeDepths: number[];
  attempts: {
    total: number;
    maxRemaining: number;
    features: Array<Record<string, unknown>>;
  };
  work: {
    current: DependencyProgressNode[];
    working: DependencyProgressNode[];
    passed: DependencyProgressNode[];
    failed: DependencyProgressNode[];
    blocked: DependencyProgressNode[];
    waiting: DependencyProgressNode[];
  };
  nodes: DependencyProgressNode[];
  questlines?: DependencyProgressQuestline[];
  score?: DependencyProgressScore;
  history?: {
    firstTryPercentage: number;
    repairAttempts: number;
  };
  // Checks that passed in one conclusive grade and failed in a later one.
  regressions?: number;
  evidence?: DependencyProgressEvidence[];
  unreadable?: string;
  [key: string]: unknown;
}

function dependencyHistory(state: DependencyState): {
  history: NonNullable<DependencyProgress['history']>;
  regressions: number;
} {
  const firstOutcomes = new Map<string, string>();
  const lastOutcomes = new Map<string, string>();
  const regressed = new Set<string>();
  let replay = progressionEngine.initialize(state.definition);
  let repairAttempts = 0;
  for (const event of state.events as DependencyEvent[]) {
    if (event.type === 'repairs-granted') {
      replay = progressionEngine.grantRepairs(replay, event.grant);
      continue;
    }
    // A completed coding session is one repair whether or not its grade finished.
    if (event.result.completedRepair === true) repairAttempts += 1;
    if (event.result.outcome === 'conclusive') {
      for (const node of event.result.nodes) {
        for (const check of node.checks) {
          const key = `${node.id}\u0000${check.id}`;
          if (!firstOutcomes.has(key)) firstOutcomes.set(key, check.outcome);
          if (check.outcome === 'fail' && lastOutcomes.get(key) === 'pass') regressed.add(key);
          if (check.outcome !== 'not-run') lastOutcomes.set(key, check.outcome);
        }
      }
    }
    replay = progressionEngine.recordResult(replay, event.result);
  }
  // First-try points over every selected point, the same scale as the final
  // score, so the two read side by side.
  let passed = 0;
  let available = 0;
  for (const node of state.definition.nodes) {
    for (const check of node.gradingChecks) {
      available += check.points;
      if (firstOutcomes.get(`${node.id}\u0000${check.id}`) === 'pass') passed += check.points;
    }
  }
  return {
    history: {
      firstTryPercentage: available ? (passed / available) * 100 : 0,
      repairAttempts,
    },
    regressions: regressed.size,
  };
}

export function dependencyProgress(plan: CompiledCampaignPlan, attempt: CampaignAttemptPlan,
  executionDirectory: string | null): DependencyProgress | null {
  if (attempt.mode?.id !== 'dependency' || !plan.featureCatalog
    || !plan.dependencyPolicy || !executionDirectory) return null;
  const statePath = join(executionDirectory, ARTIFACT_FILE.progressionState);
  if (!existsSync(statePath)) return null;
  try {
    const stored = readProgressionState(statePath, {
      progression: compileProgressionInput(dependencyRuntimeDefinition(
        plan.featureCatalog, plan.dependencyPolicy)),
      featureCatalogIdentity: plan.featureCatalog.identity,
      dependencyPolicyIdentity: plan.dependencyPolicy.identity,
      owner: campaignProgressionOwner(plan, attempt, { workspace: true }),
    });
    const state = stored.state;
    const definitions = new Map(state.definition.nodes.map(node => [node.id, node]));
    const nodes = Object.entries(state.nodes).map(([id, node]) => {
      const definition = definitions.get(id);
      if (!definition) throw new Error(`progression state has unknown node ${id}`);
      const checks = Object.values(node.checks);
      return {
        id,
        title: definition.title,
        depth: definition.level,
        // The questline and the dependency ids let a view draw the graph the
        // engine walks, instead of re-deriving structure from prose.
        questline: definition.questline,
        dependencies: [...definition.dependencies],
        blockedBy: definition.dependencies.filter(parentId => {
          const parent = state.nodes[parentId];
          return parent?.status === 'failed' || parent?.status === 'blocked';
        }),
        status: node.status,
        repairs: { ...node.repairs },
        exhaustionReason: node.exhaustionReason,
        checks: {
          passed: checks.filter(value => value === 'pass').length,
          failed: checks.filter(value => value === 'fail').length,
          total: checks.length,
        },
      };
    });
    const action = progressionEngine.nextAction(state);
    const repair = action.type === 'terminal' ? null : action.repair;
    const prompt = action.type === 'terminal' ? null : action.prompt as DependencyPromptSelection;
    const currentIds = new Set(prompt?.nodeIds ?? []);
    const activeDepths = [...new Set(nodes.filter(node => currentIds.has(node.id))
      .map(node => node.depth))].sort((left, right) => left - right);
    return {
      phase: state.phase,
      activeDepths,
      attempts: {
        total: state.attempts.length,
        maxRemaining: repair?.remaining ?? 0,
        features: repair?.nodeIds.map(nodeId => ({ nodeId })) ?? [],
      },
      work: {
        current: nodes.filter(node => currentIds.has(node.id)),
        working: nodes.filter(node => node.status === 'working'),
        passed: nodes.filter(node => node.status === 'passed'),
        failed: nodes.filter(node => node.status === 'failed'),
        blocked: nodes.filter(node => node.status === 'blocked'),
        waiting: nodes.filter(node => node.status === 'locked'),
      },
      questlines: state.definition.questlines.map(questline => ({
        id: questline.id, title: questline.title,
        // the declared, ordered membership — a view must never re-derive it
        nodes: [...questline.nodes] })),
      nodes,
      score: progressionEngine.score(state),
      ...dependencyHistory(state as DependencyState),
      evidence: state.attempts.map((item, index) => ({
        attempt: index + 1,
        depth: item.level,
        outcome: item.outcome,
        runId: item.runId ?? null,
        sourceSha256: item.sourceSha256 ?? null,
        selectionSha256: item.selectionSha256 ?? null,
      })),
      stateSha256: stored.stateSha256,
    };
  } catch (error) {
    return { unreadable: error instanceof Error ? error.message : String(error), phase: 'unreadable',
      activeDepths: [], attempts: { total: 0, maxRemaining: 0, features: [] },
      work: { current: [], working: [], passed: [], failed: [], blocked: [], waiting: [] },
      nodes: [] };
  }
}

export function inspectCampaignAttempt(plan: CompiledCampaignPlan, attempt: CampaignAttemptState,
  directory: string) {
  const execution = attempt.executions.at(-1) ?? null;
  const executionDirectory = execution ? join(directory, execution.output) : null;
  const artifactPath = (name: string): string | null => execution && executionDirectory
    && existsSync(join(executionDirectory, name))
    ? join(execution.output, name).replaceAll('\\', '/') : null;
  const result = executionDirectory && execution
    ? readCampaignRunResult(join(executionDirectory, ARTIFACT_FILE.run), plan, attempt.plan, execution) : null;
  const classified = result?.measurementClassification;
  const costs = attempt.executions.map(item => {
    try {
      const run = readArtifactPayload(join(directory, item.output, ARTIFACT_FILE.run), { expectedKind: 'benchmark_run' });
      if (run.artifactEnvelope && typeof run.artifactEnvelope === 'object'
        && (run.artifactEnvelope as { attempt?: { parentId?: string } }).attempt?.parentId !== attempt.plan.id) {
        throw new Error('cost evidence belongs to another attempt');
      }
      if (run.backend !== attempt.plan.stack || run.model !== attempt.plan.model
        || canonicalDefinitionJson(run.pricing) !== canonicalDefinitionJson(attempt.plan.pricing)) {
        throw new Error('cost evidence belongs to another variant');
      }
      return { cost: runCostEvidence(run, 'execution') };
    } catch { return { cost: { status: 'unknown' as const, costUsd: null } }; }
  });
  const dependency = dependencyProgress(plan, attempt.plan, executionDirectory);
  return {
    id: attempt.plan.id,
    cohortKey: campaignCohortKey(attempt.plan),
    comparisonKey: campaignComparisonKey(attempt.plan),
    variantLabel: `${attempt.plan.model} / ${attempt.plan.guidance} / ${attempt.plan.condition.id}`,
    cost: costs.at(-1)?.cost ?? { status: 'unknown' as const, costUsd: null },
    spend: executionSpend(costs),
    completion: dependency?.score?.completion ?? result?.completion ?? null,
    stack: attempt.plan.stack,
    model: attempt.plan.model,
    guidance: attempt.plan.guidance,
    repetition: attempt.plan.repetition,
    levels: attempt.plan.levels,
    status: attempt.status === 'completed' && classified?.status === 'invalid' ? 'invalid' : attempt.status,
    executions: attempt.executions.length,
    execution: execution ? {
      id: execution.id,
      ordinal: execution.ordinal,
      status: classified?.status ?? execution.status,
      outcome: classified?.outcome ?? execution.outcome,
      reason: classified?.reason ?? execution.reason,
      ...(classified && (classified.status !== execution.status || classified.outcome !== execution.outcome)
        ? { recordedStatus: execution.status, recordedOutcome: execution.outcome } : {}),
      output: execution.output,
      startedAt: execution.startedAt,
      completedAt: execution.completedAt,
      runIndex: execution.runIndex,
    } : null,
    result,
    artifacts: executionDirectory ? {
      run: artifactPath(ARTIFACT_FILE.run),
      progression: artifactPath(ARTIFACT_FILE.progressionState),
      process: artifactPath(ARTIFACT_FILE.process),
      preflight: artifactPath(ARTIFACT_FILE.preflight),
      recovery: artifactPath(ARTIFACT_FILE.recovery),
    } : null,
    dependency,
  };
}

export function inspectCampaignSummary(directory: string) {
  const { plan, state } = readCampaignState(directory, { requireCurrentInputs: false });
  const attempts = state.attempts.map(attempt => inspectCampaignAttempt(plan, attempt, directory));
  const corrected = attempts.filter((attempt, index) => attempt.status !== state.attempts[index]!.status).length;
  return {
    schemaVersion: 1,
    id: plan.id,
    version: plan.version,
    sha256: plan.contentSha256,
    title: plan.title,
    state: plan.state,
    mode: plan.definition.mode?.id ?? 'sequential',
    status: state.status,
    track: plan.definition.track,
    levels: plan.definition.levels,
    stacks: plan.stacks.map(stack => stack.id),
    repetitions: plan.definition.repetitions,
    maxParallel: state.maxParallel,
    createdAt: state.createdAt,
    updatedAt: state.updatedAt,
    summary: corrected ? { ...state.summary, completed: state.summary.completed - corrected,
      invalid: state.summary.invalid + corrected } : state.summary,
    ...(corrected ? { recordedSummary: state.summary } : {}),
    budgets: plan.definition.budgets,
    budgetExposure: {
      plannedAttempts: plan.summary.attempts,
      maximumExecutions: plan.summary.attempts * (1 + plan.definition.attemptPolicy.retries),
      maxCostUsd: plan.definition.budgets.maxCostUsdPerAttempt === null ? null
        : Number((plan.summary.attempts * plan.definition.budgets.maxCostUsdPerAttempt).toFixed(6)),
      retriesShareAttemptCap: true,
      repairGrants: 'require-new-authority',
    },
    facts: campaignFacts(plan),
    attempts,
  };
}
