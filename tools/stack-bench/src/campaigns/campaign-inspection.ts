import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';

import { readCampaignState } from './campaign-scheduler.js';
import type { CampaignAttemptState } from './campaign-scheduler.js';
import { ARTIFACT_FILE, readArtifactPayload } from '../evidence/artifacts.js';
import { progressionEngine } from '../progression/progression-engine.js';
import { readProgressionState } from '../progression/progression-state.js';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../progression/progression-definition.js';
import type { DependencyEvent, DependencyState } from '../progression/dependency-mode.js';
import { campaignGradingQualification, campaignProgressionOwner } from './campaign-compiler.js';
import type { CampaignAttemptPlan, CompiledCampaignPlan } from './campaign-compiler.js';
import type { DependencyPromptSelection } from '../progression/dependency-mode.js';
import { validateCampaignRun } from './campaign-run-validation.js';

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
}

interface RunLevel {
  level: number;
  score: number;
  max: number;
  graded?: boolean;
  durationSec?: number | null;
  buildCostUsd?: number | null;
  fixCostUsd?: number | null;
  firstBuild?: Score & { outcome?: RunOutcome; missed?: Array<string | CheckFailure> };
  repair?: { roundsUsed?: number; status?: string | null };
  outcome?: RunOutcome;
  missed?: Array<string | CheckFailure>;
}

interface BenchmarkRunPayload {
  outcome?: RunOutcome;
  totals?: Score & {
    costUsd?: number | null;
    costComplete?: boolean | null;
    durationSec?: number | null;
  };
  backendLease?: { state?: string | null };
  levels?: RunLevel[];
}

interface CampaignRunLevelResult {
  level: number;
  firstScore: Score | null;
  firstAbort: { phase: string; reason: string | null } | null;
  finalScore: Score | null;
  roundsUsed: number;
  repairStatus: string | null;
  outcome: string | null;
  durationSec: number | null;
  costUsd: number | null;
  failures: string[];
}

interface CampaignRunResult {
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
  attempt: CampaignAttemptPlan): CampaignRunResult | null {
  if (!existsSync(path)) return null;
  try {
    const run = readArtifactPayload<BenchmarkRunPayload>(path, { expectedKind: 'benchmark_run' });
    validateCampaignRun(plan, attempt, run, { resultDir: dirname(path) });
    return {
      outcome: run.outcome?.kind ?? 'ungraded',
      outcomePhase: run.outcome?.phase ?? null,
      outcomeReason: run.outcome?.reason ?? null,
      score: run.totals ? { score: run.totals.score, max: run.totals.max } : null,
      costUsd: run.totals?.costUsd ?? null,
      costComplete: run.totals?.costComplete ?? null,
      durationSec: run.totals?.durationSec ?? null,
      cleanup: run.backendLease?.state ?? null,
      levels: (run.levels ?? []).map(level => ({
        level: level.level,
        firstScore: level.firstBuild
          ? { score: level.firstBuild.score, max: level.firstBuild.max } : null,
        firstAbort: firstGradeAbort(level.firstBuild),
        finalScore: level.graded ? { score: level.score, max: level.max } : null,
        roundsUsed: level.repair?.roundsUsed ?? 0,
        repairStatus: level.repair?.status ?? null,
        outcome: level.outcome?.kind ?? null,
        durationSec: level.durationSec ?? null,
        costUsd: level.buildCostUsd == null && level.fixCostUsd == null
          ? null : (level.buildCostUsd ?? 0) + (level.fixCostUsd ?? 0),
        failures: (level.outcome?.appFailures
          ?? (level.graded ? [] : level.missed ?? level.firstBuild?.missed ?? [])).map(item =>
          typeof item === 'string' ? item : item.stableKey ?? item.description ?? 'Failed check'),
      })),
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
      version: level.recipe?.version ?? null,
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
  strikes: { budget: number; used: number; remaining: number };
  exhaustionReason: unknown;
  checks: { passed: number; failed: number; total: number };
}

export interface DependencyProgressScore {
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
  evidence?: DependencyProgressEvidence[];
  unreadable?: string;
  [key: string]: unknown;
}

function dependencyHistory(state: DependencyState): NonNullable<DependencyProgress['history']> {
  const firstOutcomes = new Map<string, string>();
  let replay = progressionEngine.initialize(state.definition);
  let repairAttempts = 0;
  for (const event of state.events as DependencyEvent[]) {
    if (event.type === 'strikes-granted') {
      replay = progressionEngine.grantStrikes(replay, event.grant);
      continue;
    }
    const action = progressionEngine.nextAction(replay);
    if (event.result.outcome === 'conclusive') {
      if (action.type === 'repair') repairAttempts += 1;
      for (const node of event.result.nodes) {
        for (const check of node.checks) {
          const key = `${node.id}\u0000${check.id}`;
          if (!firstOutcomes.has(key)) firstOutcomes.set(key, check.outcome);
        }
      }
    }
    replay = progressionEngine.recordResult(replay, event.result);
  }
  const byNode = new Map(state.definition.nodes.map(node => [node.id, node]));
  const questlinePercentages = state.definition.questlines.map(questline => {
    let passed = 0;
    let available = 0;
    for (const nodeId of questline.nodes) {
      const node = byNode.get(nodeId);
      if (!node) continue;
      for (const check of node.gradingChecks) {
        available += check.points;
        if (firstOutcomes.get(`${nodeId}\u0000${check.id}`) === 'pass') passed += check.points;
      }
    }
    return available ? (passed / available) * 100 : 0;
  });
  return {
    firstTryPercentage: questlinePercentages.length
      ? questlinePercentages.reduce((sum, value) => sum + value, 0)
        / questlinePercentages.length
      : 0,
    repairAttempts,
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
        strikes: { ...node.strikes, remaining: node.strikes.budget - node.strikes.used },
        exhaustionReason: node.exhaustionReason,
        checks: {
          passed: checks.filter(value => value === 'pass').length,
          failed: checks.filter(value => value === 'fail').length,
          total: checks.length,
        },
      };
    });
    const action = progressionEngine.nextAction(state);
    const strikes = action.type === 'terminal' ? null : action.strikes;
    const prompt = action.type === 'terminal' ? null : action.prompt as DependencyPromptSelection;
    const currentIds = new Set(prompt?.nodeIds ?? []);
    const activeDepths = [...new Set(nodes.filter(node => currentIds.has(node.id))
      .map(node => node.depth))].sort((left, right) => left - right);
    return {
      phase: state.phase,
      activeDepths,
      attempts: {
        total: state.attempts.length,
        maxRemaining: strikes?.maxRemaining ?? 0,
        features: strikes?.nodes ?? [],
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
      history: dependencyHistory(state as DependencyState),
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
  return {
    id: attempt.plan.id,
    stack: attempt.plan.stack,
    model: attempt.plan.model,
    guidance: attempt.plan.guidance,
    repetition: attempt.plan.repetition,
    levels: attempt.plan.levels,
    status: attempt.status,
    executions: attempt.executions.length,
    execution: execution ? {
      id: execution.id,
      ordinal: execution.ordinal,
      status: execution.status,
      outcome: execution.outcome,
      reason: execution.reason,
      output: execution.output,
      startedAt: execution.startedAt,
      completedAt: execution.completedAt,
      runIndex: execution.runIndex,
    } : null,
    result: executionDirectory
      ? readCampaignRunResult(join(executionDirectory, ARTIFACT_FILE.run), plan, attempt.plan)
      : null,
    artifacts: executionDirectory ? {
      run: artifactPath(ARTIFACT_FILE.run),
      progression: artifactPath(ARTIFACT_FILE.progressionState),
      process: artifactPath(ARTIFACT_FILE.process),
      preflight: artifactPath(ARTIFACT_FILE.preflight),
      recovery: artifactPath(ARTIFACT_FILE.recovery),
    } : null,
    dependency: dependencyProgress(plan, attempt.plan, executionDirectory),
  };
}

export function inspectCampaignSummary(directory: string) {
  const { plan, state } = readCampaignState(directory, { requireCurrentInputs: false });
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
    summary: state.summary,
    budgets: plan.definition.budgets,
    facts: campaignFacts(plan),
    attempts: state.attempts.map(attempt => inspectCampaignAttempt(plan, attempt, directory)),
  };
}
