import { retainedRunCost } from '../evidence/retained-run-cost.js';
import { randomUUID } from 'node:crypto';
import { lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync,
  renameSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve, sep } from 'node:path';

import { ARTIFACT_FILE, emptyArtifactIdentities, readArtifact, readArtifactPayload, writeArtifact }
  from '../evidence/artifacts.js';
import { inspectCampaign } from './campaign-runner.js';
import { validateCampaignRun } from './campaign-run-validation.js';
import { canonicalDefinitionJson, canonicalizeDefinition } from '../composition/definition-plan.js';
import { RUN_INDEX_CAP } from '../composition/tracks.js';
import { sha256 } from '../evidence/provenance.js';
import { campaignGradingQualification } from './campaign-compiler.js';
import type { CampaignAttemptPlan, CampaignGradingQualification,
  CompiledCampaignPlan } from './campaign-compiler.js';
import type { CampaignExecution, CampaignState } from './campaign-scheduler.js';
import { campaignTimeBudget, timeGrantReceiptSchema } from './campaign-time-grant.js';
import type { CampaignTimeBudget } from './campaign-time-grant.js';
import { classifyCampaignExecution } from './campaign-scheduler.js';
import type { RunOutcome } from '../evidence/outcomes.js';
import { runCostEvidence, sessionCostEvidence, sumCostEvidence } from '../evidence/cost-proof.js';
import type { CostEvidence } from '../evidence/cost-proof.js';
import { checkpointSchema, completionSchema, costEvidenceSchema, completionCurve } from '../evidence/run-checkpoints.js';
import type { CompletionCurve } from '../evidence/run-checkpoints.js';
import { checkCompletion } from '../evidence/check-completion.js';
import type { CheckCompletion, CheckStatus } from '../evidence/check-completion.js';
import { CAMPAIGN_FILE, campaignChildPath } from './campaign-path.js';
import type { BenchmarkRunRecord, GradeBundleSelection, RunLevelRecord, RunTotals }
  from '../evidence/benchmark-run.js';
import { assessStoppedProviderContinuation, providerWaitSummary }
  from '../agents/provider-continuation-audit.js';
import type { HistoricalProviderContinuation, ProviderWaitSummary }
  from '../agents/provider-continuation-audit.js';
import { readCampaignProviderWaitHistory } from './campaign-provider-continuation.js';

export const CAMPAIGN_REPORT_SCHEMA_VERSION = 7;

interface RunCheck {
  executionId: string;
  featureId: string | number;
  criterionId: string;
  points: number;
  [key: string]: unknown;
}

export interface RunSelection extends Omit<GradeBundleSelection,
'checks' | 'observedChecks'> {
  sha256?: string;
  checks?: RunCheck[];
  observedChecks?: Array<{ points?: number; [key: string]: unknown }>;
  specifications?: {
    observed?: string[];
    requested?: string[];
    expected?: string[];
  };
  schemaVersion?: number;
}

interface RunObservation {
  reportedChecks?: unknown[];
  observedPoints?: number;
  passedPoints?: number;
  sourceSha256?: string;
  artifact?: string;
  outcome?: unknown;
  selectionSha256?: string;
  selectedChecks?: string[];
  scoreContribution?: boolean;
  repairVisible?: boolean;
}

interface RunLevel extends Omit<Partial<RunLevelRecord>,
'selection' | 'firstBuild' | 'repair'> {
  firstBuild?: {
    score?: number;
    max?: number;
    outcome?: RunOutcome;
    observations?: RunObservation;
    source?: unknown;
    [key: string]: unknown;
  };
  score?: number;
  max?: number;
  repairCostUsd?: number;
  repairs?: number;
  repair?: unknown;
  outcome?: RunOutcome;
  selection?: RunSelection;
}

export interface BenchmarkRun extends Partial<Pick<BenchmarkRunRecord,
'id' | 'parentAttemptId' | 'outcome' | 'checkpoints' | 'progressionResume' | 'pricing'>> {
  levels?: RunLevel[];
  condition?: { requested?: { levels?: Array<{ level: number; selection?: RunSelection }> } };
  outcome?: RunOutcome;
  totals?: Partial<RunTotals>;
  progressionStatus?: {
    phase?: string;
    score?: { completion?: CheckCompletion; questlines?: Array<{id: string; title: string; completion?: CheckCompletion}>;
      questlineAveragePercentage?: number | null; uniqueChecks?: {
      gradedPoints?: number;
      availablePoints?: number;
      percentage?: number | null;
    } };
  };
}

export interface MetricSummary {
  n: number;
  center: number | null;
  spread: { kind: string; [key: string]: number | string } | null;
  min: number | null;
  max: number | null;
}

export interface CampaignRunObservationSummary {
  selectedChecks: number;
  reportedChecks: number | null;
  selectedPoints: number | null;
  observedPoints: number | null;
  passedPoints: number | null;
  passRate: number | null;
  coverageRate: number | null;
  scoreContribution: false;
  repairVisible: false;
  levels: Array<{
    level: number;
    specifications: string[];
    selectedChecks: number;
    reportedChecks: number | null;
    selectedPoints: number | null;
    observedPoints: number | null;
    passedPoints: number | null;
    passRate: number | null;
    coverageRate: number | null;
    scoreContribution: false;
    repairVisible: false;
    sourceSha256: string | null;
    artifact: string | null;
    outcome: unknown;
  }>;
}

interface CampaignReportCondition {
  key: string;
  stack: string;
  agent: { adapter: string; model: string; providerRoute?: string; maxOutputTokens?: number };
  condition: { id: string; contentSha256: string; requested?: {
    levels?: Array<{ level: number; selection?: RunSelection }> } };
  sample: {
    plannedAttempts: number;
    completedAttempts: number;
    invalidAttempts: number;
    pendingAttempts: number;
    executions: number;
    invalidExecutions: number;
    invalidExecutionRate: number;
  };
  metrics: Record<string, MetricSummary>;
  spend: CampaignSpend;
  firstBuildObservations: {
    sample: { selectedAttempts: number; measuredAttempts: number };
    metrics: { passRate: MetricSummary; coverageRate: MetricSummary };
  } | null;
}

interface CampaignReportExecution {
  id: string;
  status: string;
  outcome: unknown;
  evidence: string;
  admissionEvidence: string;
  firstBuildObservations: CampaignRunObservationSummary | null;
  metrics?: Record<string, number | null> | null;
  cost: CostEvidence;
  recorded?: CostEvidence;
  usage: ExecutionUsage;
  providerContinuation?: HistoricalProviderContinuation | null;
  providerWaits?: ProviderWaitSummary | null;
  [key: string]: unknown;
}

export type CampaignSpend = CostEvidence & {
  /** Sum of available exact amounts and upper bounds; must not be assumed to be a lower bound. */
  knownCostUsd: number;
  unknownExecutions: number;
  boundedExecutions: number;
};

interface CampaignReportAttempt extends CampaignAttemptPlan {
  status: string;
  timeBudget?: CampaignTimeBudget;
  executions: CampaignReportExecution[];
  metrics: Record<string, number | null> | null;
  firstBuildObservations: CampaignRunObservationSummary | null;
  spend: CampaignSpend;
  completion: CheckCompletion;
  curve: CompletionCurve;
  groups: Array<{ id: string; title: string; completion: CheckCompletion;
    cost: CostEvidence; costAttribution: 'not-separable' | 'measured-work' }>;
}

export interface ExecutionUsage {
  input: number | null;
  output: number | null;
  cacheWrite: number | null;
  cacheRead: number | null;
  pricing: unknown;
  build: CostEvidence;
  repair: CostEvidence;
  resume: CostEvidence;
}

export interface CampaignReport {
  reportSchemaVersion: number;
  campaign: { id: string; version: string; state: string; sha256: string; title: string };
  scope: {
    track: string;
    levels: number[];
    selection: unknown;
    bindings: Array<{ level: number; [key: string]: unknown }>;
    grading: CampaignGradingQualification;
    stacks: Array<{ id: string; [key: string]: unknown }>;
    agents: unknown[];
    conditions: CampaignReportCondition['condition'][];
    repetitions: number;
    repetitionsByStack: Record<string, number>;
    parallelism: number;
    runtime: Record<string, unknown>;
    pricing: Record<string, unknown>;
  };
  policy: { primaryMetric: string; secondaryMetrics: string[]; dispersion: string;
    spendThresholdsUsd?: number[]; completionTargets?: number[];
    [key: string]: unknown };
  attempts: CampaignReportAttempt[];
  conditions: CampaignReportCondition[];
  summary: { campaignStatus: string; plannedAttempts: number; completedAttempts: number;
    invalidAttempts: number; invalidAttemptRate: number; pendingAttempts: number;
    runningAttempts: number; executions: number; invalidExecutions: number;
    invalidExecutionRate: number; spend: CampaignSpend };
  limitations: string[];
  contentSha256: string;
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const number = (value: unknown): number | null => typeof value === 'number' && Number.isFinite(value)
  ? value : null;
const ratio = (value: number | null, max: number | null): number | null =>
  value !== null && max !== null && Number.isFinite(value) && Number.isFinite(max) && max > 0
  ? Number((value / max).toFixed(6)) : null;
const mean = (values: number[]): number =>
  values.reduce((total, value) => total + value, 0) / values.length;

export function formatDurationMs(value: unknown): string {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) return '—';
  const totalSeconds = Math.round(value / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours) return `${hours}h ${minutes}m ${seconds}s`;
  if (minutes) return `${minutes}m ${seconds}s`;
  return `${seconds}s`;
}

function formatUsd(value: unknown): string {
  return typeof value === 'number' && Number.isFinite(value)
    ? `$${Number(value.toFixed(4))}` : '—';
}

export function formatCostEvidence(cost: CostEvidence): string {
  return cost.status === 'unknown' ? 'Unknown'
    : `${cost.status === 'upper-bound' ? '≤ ' : ''}${formatUsd(cost.costUsd)}`;
}

function formatRate(value: unknown): string {
  return typeof value === 'number' && Number.isFinite(value)
    ? `${Number((value * 100).toFixed(2))}%` : '—';
}

function formatTokens(value: unknown): string {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) return '—';
  if (value >= 1_000_000) return `${Number((value / 1_000_000).toFixed(1))}M tokens`;
  if (value >= 1_000) return `${Math.round(value / 1_000)}k tokens`;
  return `${Math.round(value)} tokens`;
}

function formatMetric(metric: string, value: unknown): string {
  if (metric.endsWith('Rate')) return formatRate(value);
  if (metric.endsWith('CostUsd') || metric.endsWith('SpendUsd')) return formatUsd(value);
  if (metric === 'totalDurationMs') return formatDurationMs(value);
  return typeof value === 'number' && Number.isFinite(value) ? String(value) : '—';
}

function declaredMax(measurableMax: unknown, outcome: RunOutcome | undefined,
  selection: RunSelection | undefined): number | null {
  if (typeof measurableMax !== 'number' || !Number.isFinite(measurableMax)
    || measurableMax < 0) return null;
  if (outcome?.inconclusive != null && !Array.isArray(outcome.inconclusive)) return null;
  if (outcome?.harnessFailures != null && !Array.isArray(outcome.harnessFailures)) return null;
  const unavailable = [...new Set([
    ...(outcome?.inconclusive ?? []),
    ...(outcome?.harnessFailures ?? []),
  ])];
  if (!unavailable.length) return measurableMax;
  if (!Array.isArray(selection?.checks)) return null;
  let unavailablePoints = 0;
  for (const key of unavailable) {
    const [executionId, featureId, criterionId, ...extra] = String(key).split('/');
    if (extra.length || !executionId || !featureId || !criterionId) return null;
    const check = selection.checks.find(item => item.executionId === executionId
      && String(item.featureId) === featureId && String(item.criterionId) === criterionId);
    if (!check || !Number.isFinite(check.points) || check.points < 0) return null;
    unavailablePoints += check.points;
  }
  return measurableMax + unavailablePoints;
}

function quantile(sorted: number[], p: number): number {
  if (sorted.length === 1) return sorted[0]!;
  const index = (sorted.length - 1) * p;
  const low = Math.floor(index);
  const high = Math.ceil(index);
  return sorted[low]! + (sorted[high]! - sorted[low]!) * (index - low);
}

// A spread needs a sample. Below three completed attempts the centre and the
// range are reported and the spread stays null rather than describing noise.
const MINIMUM_SPREAD_SAMPLE = 3;

function summarize(values: Array<number | null | undefined>, dispersion: string): MetricSummary {
  const present = values.filter((value): value is number => typeof value === 'number'
    && Number.isFinite(value)).sort((a, b) => a - b);
  if (!present.length) return { n: 0, center: null, spread: null, min: null, max: null };
  const spreadable = present.length >= MINIMUM_SPREAD_SAMPLE;
  if (dispersion === 'median-iqr') return {
    n: present.length,
    center: Number(quantile(present, 0.5).toFixed(6)),
    spread: spreadable ? { kind: 'iqr', q1: Number(quantile(present, 0.25).toFixed(6)),
      q3: Number(quantile(present, 0.75).toFixed(6)) } : null,
    min: present[0]!, max: present.at(-1)!,
  };
  const center = mean(present);
  const variance = present.length > 1
    ? present.reduce((total, value) => total + ((value - center) ** 2), 0) / (present.length - 1)
    : 0;
  return { n: present.length, center: Number(center.toFixed(6)),
    spread: spreadable ? { kind: 'sd', value: Number(Math.sqrt(variance).toFixed(6)) } : null,
    min: present[0]!, max: present.at(-1)! };
}

export function campaignRunMetrics(run: BenchmarkRun): Record<string, number | null> {
  const cost = runCostEvidence(run);
  const levels = run.levels ?? [];
  const completeFirstBuild = levels.length > 0 && levels.every(level =>
    number(level.firstBuild?.score) !== null && number(level.firstBuild?.max) !== null);
  const firstScore = completeFirstBuild
    ? levels.reduce((total, level) => total + level.firstBuild!.score!, 0) : null;
  const firstMax = completeFirstBuild
    ? levels.reduce((total, level) => total + level.firstBuild!.max!, 0) : null;
  const correctionNeeded = completeFirstBuild
    ? levels.some(level => level.firstBuild!.score! < level.firstBuild!.max!) : null;
  const repairCost = executionUsage(run).repair;
  const correctionSpendUsd = correctionNeeded === true && repairCost.status === 'exact'
    ? repairCost.costUsd : null;
  const correctionSuccessRate = correctionNeeded !== true ? null
    : run.outcome?.kind === 'passed' ? 1
      : run.outcome?.kind === 'app_failure' ? 0 : null;
  const firstDeclaredMaxima: Array<number | null> = completeFirstBuild
    ? levels.map(level => declaredMax(level.firstBuild!.max, level.firstBuild!.outcome,
      level.selection)) : [];
  const finalMeasuredMaxima: Array<number | null> = levels.map(level => number(level.max));
  const finalDeclaredMaxima: Array<number | null> = levels.map(level => declaredMax(level.max, level.outcome,
    level.selection));
  // Dependency mode scores passed points over every selected point in the
  // graph, the same scale as the first build. The equal-weight questline
  // average is a secondary view, because questlines range from 9 to 59 points.
  const terminal = run.progressionStatus?.phase === 'terminal';
  const progressionScore = run.progressionStatus === undefined
    ? undefined
    : terminal ? number(run.progressionStatus?.score?.uniqueChecks?.percentage) : null;
  const questlineAverage = run.progressionStatus === undefined
    ? null
    : terminal ? number(run.progressionStatus?.score?.questlineAveragePercentage) : null;
  const progressionCoverage = run.progressionStatus === undefined
    ? undefined
    : terminal
      ? ratio(number(run.progressionStatus?.score?.uniqueChecks?.gradedPoints),
        number(run.progressionStatus?.score?.uniqueChecks?.availablePoints)) : null;
  // Time spent waiting for the provider to lift a rate limit is recorded per
  // session and is not the stack's or the agent's to answer for.
  const throttleWaitMs = levels.reduce((total, level) =>
    total + (number(level.sessionTotals?.providerThrottle?.waitedMs) ?? 0), 0);
  const durationMs = number(run.totals?.durationSec) === null
    ? null : Math.max(0, (run.totals!.durationSec! * 1000) - throttleWaitMs
      - (number(run.totals?.pausedDurationSec) ?? 0) * 1000);
  return {
    checkCompletionRate: run.progressionStatus !== undefined && !terminal ? null
      : run.progressionStatus?.score?.completion?.rate
        ?? run.checkpoints?.findLast(checkpoint => checkpoint.accepted)?.completion.rate ?? null,
    firstBuildScoreRate: ratio(firstScore, firstMax),
    finalScoreRate: progressionScore === undefined
      ? ratio(number(run.totals?.score), number(run.totals?.max))
      : ratio(progressionScore, 100),
    questlineAverageRate: ratio(questlineAverage, 100),
    firstBuildCoverageRate: firstDeclaredMaxima.length
      && firstDeclaredMaxima.every(Number.isFinite)
      ? ratio(firstMax, firstDeclaredMaxima.reduce<number>((total, value) => total + (value ?? 0), 0)) : null,
    finalCoverageRate: progressionCoverage === undefined
      ? finalMeasuredMaxima.length
        && finalMeasuredMaxima.every(Number.isFinite) && finalDeclaredMaxima.every(Number.isFinite)
        ? ratio(finalMeasuredMaxima.reduce<number>((total, value) => total + (value ?? 0), 0),
          finalDeclaredMaxima.reduce<number>((total, value) => total + (value ?? 0), 0)) : null
      : progressionCoverage,
    totalCostUsd: cost.status === 'exact' ? cost.costUsd : null,
    totalCostUpperBoundUsd: cost.status === 'upper-bound' ? cost.costUsd : null,
    totalTokens: number(run.totals?.tokens),
    totalDurationMs: durationMs,
    repairs: number(run.totals?.repairs),
    correctionSuccessRate,
    correctionCostUsd: correctionSuccessRate === 1 ? correctionSpendUsd : null,
    correctionSpendUsd,
  };
}

export function campaignRunFirstBuildObservations(
  run: BenchmarkRun,
): CampaignRunObservationSummary | null {
  const actualByLevel = new Map<number, RunLevel>((run.levels ?? [])
    .filter((level): level is RunLevel & { level: number } => Number.isInteger(level.level))
    .map(level => [level.level, level]));
  const plannedByLevel = new Map((run.condition?.requested?.levels ?? [])
    .map(level => [level.level, level]));
  const levelNumbers = [...new Set([...actualByLevel.keys(), ...plannedByLevel.keys()])]
    .sort((left, right) => left - right);
  const levels = levelNumbers.flatMap(levelNumber => {
    const level = actualByLevel.get(levelNumber);
    const planned = plannedByLevel.get(levelNumber);
    const selected = Array.isArray(level?.selection?.observedChecks)
      ? level.selection.observedChecks : planned?.selection?.observedChecks;
    if (!Array.isArray(selected) || selected.length === 0) return [];
    const observation = level?.firstBuild?.observations ?? null;
    const selectedPoints = selected.every(check => typeof check?.points === 'number'
      && Number.isFinite(check.points) && check.points >= 0)
      ? selected.reduce((total, check) => total + check.points!, 0) : null;
    return [{
      level: levelNumber,
      specifications: [...(level?.selection?.specifications?.observed
        ?? planned?.selection?.specifications?.observed ?? [])],
      selectedChecks: selected.length,
      reportedChecks: Array.isArray(observation?.reportedChecks)
        ? observation.reportedChecks.length : null,
      selectedPoints,
      observedPoints: number(observation?.observedPoints),
      passedPoints: number(observation?.passedPoints),
      passRate: ratio(number(observation?.passedPoints), number(observation?.observedPoints)),
      coverageRate: ratio(number(observation?.observedPoints), selectedPoints),
      scoreContribution: false as const,
      repairVisible: false as const,
      sourceSha256: observation?.sourceSha256 ?? null,
      artifact: observation?.artifact ?? null,
      outcome: observation?.outcome ?? null,
    }];
  });
  if (!levels.length) return null;
  const totalsComplete = levels.every(level => Number.isFinite(level.selectedPoints)
    && Number.isFinite(level.observedPoints) && Number.isFinite(level.passedPoints));
  const selectedPoints = levels.every(level => Number.isFinite(level.selectedPoints))
    ? levels.reduce((total, level) => total + level.selectedPoints!, 0) : null;
  const observedPoints = totalsComplete
    ? levels.reduce((total, level) => total + level.observedPoints!, 0) : null;
  const passedPoints = totalsComplete
    ? levels.reduce((total, level) => total + level.passedPoints!, 0) : null;
  return {
    selectedChecks: levels.reduce((total, level) => total + level.selectedChecks, 0),
    reportedChecks: levels.every(level => Number.isInteger(level.reportedChecks))
      ? levels.reduce((total, level) => total + level.reportedChecks!, 0) : null,
    selectedPoints,
    observedPoints,
    passedPoints,
    passRate: ratio(passedPoints, observedPoints),
    coverageRate: ratio(observedPoints, selectedPoints),
    scoreContribution: false,
    repairVisible: false,
    levels,
  };
}

export function campaignCohortKey(attempt: CampaignAttemptPlan): string {
  return canonicalDefinitionJson({ stack: attempt.stack, skills: attempt.skills,
    comparison: campaignComparisonKey(attempt) }).trim();
}

/** The condition includes the declared guidance for every stack, including its skills. */
export function campaignComparisonKey(attempt: CampaignAttemptPlan): string {
  return canonicalDefinitionJson({ agentAdapter: attempt.agentAdapter,
    ...(attempt.effort ? { effort: attempt.effort } : {}),
    model: attempt.model, ...(attempt.providerRoute ? { providerRoute: attempt.providerRoute } : {}),
    ...(attempt.maxOutputTokens ? { maxOutputTokens: attempt.maxOutputTokens } : {}),
    condition: attempt.condition?.contentSha256, mode: attempt.mode,
    guidance: attempt.guidance, levels: attempt.levels, pricing: attempt.pricing,
    featureCatalog: attempt.featureCatalog ?? null, dependencyPolicy: attempt.dependencyPolicy ?? null }).trim();
}

function reportMetricNames(policy: CampaignReport['policy']): string[] {
  return [...new Set([policy.primaryMetric, ...policy.secondaryMetrics,
    'firstBuildCoverageRate', 'finalCoverageRate', 'checkCompletionRate', 'totalCostUpperBoundUsd'])]
    .filter(metric => metric !== 'invalidAttemptRate');
}

function reportConditions(rows: CampaignReportAttempt[], policy: CampaignReport['policy']):
CampaignReportCondition[] {
  const groups = new Map<string, CampaignReportAttempt[]>();
  for (const row of rows) {
    const key = campaignCohortKey(row);
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(row);
  }
  const metricNames = reportMetricNames(policy);
  return [...groups.entries()].map(([key, attempts]) => {
    const completed = attempts.filter(attempt => attempt.status === 'completed');
    const executions = attempts.flatMap(attempt => attempt.executions);
    const invalidExecutions = executions.filter(execution => execution.status === 'invalid').length;
    const observedAttempts = completed.filter((attempt): attempt is CampaignReportAttempt & {
      firstBuildObservations: CampaignRunObservationSummary;
    } => attempt.firstBuildObservations !== null);
    const measuredObservedAttempts = observedAttempts.filter(attempt =>
      Number.isFinite(attempt.firstBuildObservations.passRate));
    return {
      key: sha256(key),
      stack: attempts[0]!.stack,
      agent: { adapter: attempts[0]!.agentAdapter, model: attempts[0]!.model,
        ...(attempts[0]!.providerRoute ? { providerRoute: attempts[0]!.providerRoute } : {}),
        ...(attempts[0]!.maxOutputTokens ? { maxOutputTokens: attempts[0]!.maxOutputTokens } : {}) },
      condition: {
        id: attempts[0]!.condition.id,
        contentSha256: attempts[0]!.condition.contentSha256,
        requested: attempts[0]!.condition.requested,
      },
      sample: { plannedAttempts: attempts.length, completedAttempts: completed.length,
        invalidAttempts: attempts.filter(attempt => attempt.status === 'invalid').length,
        pendingAttempts: attempts.filter(attempt => attempt.status === 'pending').length,
        executions: executions.length, invalidExecutions,
        invalidExecutionRate: executions.length
          ? Number((invalidExecutions / executions.length).toFixed(6)) : 0 },
      metrics: { ...Object.fromEntries(metricNames.map(metric => [metric,
        summarize(completed.map(attempt => attempt.metrics?.[metric]), policy.dispersion)])),
      invalidAttemptRate: { n: attempts.length,
        center: Number((attempts.filter(attempt => attempt.status === 'invalid').length
          / attempts.length).toFixed(6)), spread: null, min: null, max: null } },
      spend: executionSpend(executions),
      firstBuildObservations: observedAttempts.length ? {
        sample: { selectedAttempts: observedAttempts.length,
          measuredAttempts: measuredObservedAttempts.length },
        metrics: {
          passRate: summarize(observedAttempts.map(attempt =>
            attempt.firstBuildObservations.passRate), policy.dispersion),
          coverageRate: summarize(observedAttempts.map(attempt =>
            attempt.firstBuildObservations.coverageRate), policy.dispersion),
        },
      } : null,
    };
  }).sort((left, right) => left.key.localeCompare(right.key));
}

export function executionSpend(executions: Array<{ cost: CostEvidence; recorded?: CostEvidence; knownCostUsd?: number }>): CampaignSpend {
  const unknownExecutions = executions.filter(execution => execution.cost.status === 'unknown').length;
  const boundedExecutions = executions.filter(execution => execution.cost.status === 'upper-bound').length;
  const knownCostUsd = Number(executions.reduce((total, execution) =>
    total + (execution.cost.costUsd ?? execution.knownCostUsd ?? execution.recorded?.costUsd ?? 0), 0).toFixed(6));
  return { ...sumCostEvidence(executions.map(execution => execution.cost)), knownCostUsd,
    unknownExecutions, boundedExecutions };
}

function executionUsage(run: BenchmarkRun | null): ExecutionUsage {
  const unknown: CostEvidence = { status: 'unknown', costUsd: null };
  const levels = (run?.levels ?? []).filter(level => !run?.progressionResume?.inheritedLevels.includes(level.level!));
  const sessions = levels.flatMap(level => [...(level.buildSessions ?? []),
    ...(level.resumeSession ? [level.resumeSession] : []), ...(level.repairSessions ?? [])]);
  const usage = Object.fromEntries((['input', 'output', 'cacheWrite', 'cacheRead'] as const).map(key => {
    const values = sessions.map(session => isRecord(session.usage) ? number(session.usage[key]) : null);
    return [key, run && values.length > 0 && values.every(value => value !== null && value >= 0)
      ? values.reduce<number>((sum, value) => sum + value!, 0) : null];
  })) as Pick<ExecutionUsage, 'input' | 'output' | 'cacheWrite' | 'cacheRead'>;
  const category = (kind: 'build' | 'repair' | 'resume'): CostEvidence => {
    if (!run || !levels.length) return unknown;
    const grouped = levels.flatMap(level => kind === 'resume' ? level.resumeSession ? [level.resumeSession] : []
      : kind === 'build' ? level.buildSessions ?? [] : level.repairSessions ?? []);
    if (!grouped.length && levels.some(level => Number(level[`${kind}CostUsd`]) > 0)) return unknown;
    return sessionCostEvidence(grouped);
  };
  return { ...usage, pricing: run?.pricing ?? null, build: category('build'),
    repair: category('repair'), resume: category('resume') };
}

function reportSummary(rows: CampaignReportAttempt[], campaignStatus: string):
CampaignReport['summary'] {
  const executions = rows.flatMap(attempt => attempt.executions);
  const invalidAttempts = rows.filter(attempt => attempt.status === 'invalid').length;
  const invalidExecutions = executions.filter(execution => execution.status === 'invalid').length;
  return { campaignStatus, plannedAttempts: rows.length,
    completedAttempts: rows.filter(attempt => attempt.status === 'completed').length,
    invalidAttempts,
    invalidAttemptRate: rows.length ? Number((invalidAttempts / rows.length).toFixed(6)) : 0,
    pendingAttempts: rows.filter(attempt => attempt.status === 'pending').length,
    runningAttempts: rows.filter(attempt => attempt.status === 'running').length,
    executions: executions.length, invalidExecutions,
    spend: executionSpend(executions),
    invalidExecutionRate: executions.length
      ? Number((invalidExecutions / executions.length).toFixed(6)) : 0 };
}

function exactFields(value: unknown, fields: Set<string>, at: string): asserts value is Record<string, unknown> {
  if (!isRecord(value)) {
    throw new Error(`${at} must be an object`);
  }
  for (const key of Object.keys(value)) {
    if (!fields.has(key)) throw new Error(`${at}.${key} is unknown`);
  }
}

export function validateCampaignReport(input: unknown): CampaignReport {
  if (!isRecord(input)) {
    throw new Error('campaign report must be an object');
  }
  const fields = new Set(['reportSchemaVersion', 'campaign', 'scope', 'policy', 'attempts',
    'conditions', 'summary', 'limitations', 'contentSha256']);
  for (const key of Object.keys(input)) if (!fields.has(key)) throw new Error(`campaign report.${key} is unknown`);
  if (input.reportSchemaVersion !== CAMPAIGN_REPORT_SCHEMA_VERSION
    || !isRecord(input.campaign)
    || typeof input.campaign.sha256 !== 'string'
    || !/^[a-f0-9]{64}$/.test(input.campaign.sha256)
    || !Array.isArray(input.attempts) || !Array.isArray(input.conditions)
    || !Array.isArray(input.limitations) || !input.summary || typeof input.summary !== 'object') {
    throw new Error('campaign report structure is invalid');
  }
  const report = input as unknown as CampaignReport;
  exactFields(report.campaign, new Set(['id', 'version', 'state', 'sha256', 'title']),
    'campaign report.campaign');
  exactFields(report.scope, new Set(['track', 'levels', 'selection', 'bindings', 'grading', 'stacks',
    'agents', 'conditions', 'repetitions', 'repetitionsByStack', 'parallelism',
    'runtime', 'pricing']), 'campaign report.scope');
  exactFields(report.policy, new Set(['primaryMetric', 'secondaryMetrics', 'dispersion',
    'invalidAttempts', 'missingData', 'comparisonUnit', 'spendThresholdsUsd', 'completionTargets']), 'campaign report.policy');
  for (const field of ['spendThresholdsUsd', 'completionTargets'] as const) {
    const values = report.policy[field];
    if (values !== undefined && (!Array.isArray(values) || new Set(values).size !== values.length
      || values.some(value => !Number.isFinite(value) || value < 0 || (field === 'completionTargets' && value > 1)))) {
      throw new Error(`campaign report.policy.${field} is invalid`);
    }
  }
  exactFields(report.summary, new Set(['campaignStatus', 'plannedAttempts', 'completedAttempts',
    'invalidAttempts', 'invalidAttemptRate', 'pendingAttempts', 'runningAttempts', 'executions',
    'invalidExecutions', 'invalidExecutionRate', 'spend']), 'campaign report.summary');
  if (typeof report.campaign.id !== 'string' || !report.campaign.id
    || typeof report.campaign.title !== 'string' || !report.campaign.title
    || typeof report.scope.track !== 'string' || !report.scope.track
    || !Array.isArray(report.scope.levels) || !Array.isArray(report.scope.bindings)
    || report.scope.bindings.some(binding => !isRecord(binding))
    || !isRecord(report.scope.grading) || !Array.isArray(report.scope.grading.levels)
    || !Array.isArray(report.scope.stacks) || !Array.isArray(report.scope.agents)
    || !Array.isArray(report.scope.conditions)
    || !Number.isInteger(report.scope.repetitions) || report.scope.repetitions < 1
    || !report.scope.repetitionsByStack || typeof report.scope.repetitionsByStack !== 'object'
    || Array.isArray(report.scope.repetitionsByStack)
    || !Number.isInteger(report.scope.parallelism) || report.scope.parallelism < 1
    || report.scope.parallelism > RUN_INDEX_CAP + 1
    || !report.scope.runtime || typeof report.scope.runtime !== 'object'
    || !report.scope.pricing || typeof report.scope.pricing !== 'object') {
    throw new Error('campaign report exact scope is invalid');
  }
  exactFields(report.scope.grading, new Set(['status', 'levels']),
    'campaign report.scope.grading');
  if (report.scope.levels.some(level => !Number.isSafeInteger(level) || level < 1)
    || new Set(report.scope.levels).size !== report.scope.levels.length) {
    throw new Error('campaign report.scope.levels must contain unique positive integers');
  }
  const expectedStackIds = report.scope.stacks.map(stack => stack.id).sort();
  const boundLevels = report.scope.bindings.map(binding => binding.level);
  const gradingLevels = report.scope.grading.levels.map(level => level.level);
  if (canonicalDefinitionJson(boundLevels) !== canonicalDefinitionJson(report.scope.levels)
    || canonicalDefinitionJson(gradingLevels) !== canonicalDefinitionJson(report.scope.levels)) {
    throw new Error('campaign report scope levels do not match bindings and grading');
  }
  if (canonicalDefinitionJson(Object.keys(report.scope.repetitionsByStack).sort())
      !== canonicalDefinitionJson(expectedStackIds)
    || Object.values(report.scope.repetitionsByStack)
      .some(value => !Number.isInteger(value) || value < 1)) {
    throw new Error('campaign report stack repetitions are invalid');
  }
  for (const [index, row] of report.conditions.entries()) {
    const at = `campaign report.conditions[${index}]`;
    exactFields(row, new Set(['key', 'stack', 'agent', 'condition', 'sample', 'metrics', 'spend',
      'firstBuildObservations']), at);
    exactFields(row.condition, new Set(['id', 'contentSha256', 'requested']),
      `${at}.condition`);
    if (typeof row.stack !== 'string' || !row.stack
      || !row.condition || typeof row.condition !== 'object' || Array.isArray(row.condition)
      || typeof row.condition.id !== 'string' || !row.condition.id
      || !/^[a-f0-9]{64}$/.test(row.condition.contentSha256)) {
      throw new Error(`${at}.condition is invalid`);
    }
    if (row.firstBuildObservations !== null) {
      exactFields(row.firstBuildObservations, new Set(['sample', 'metrics']),
        `${at}.firstBuildObservations`);
      exactFields(row.firstBuildObservations.sample,
        new Set(['selectedAttempts', 'measuredAttempts']), `${at}.firstBuildObservations.sample`);
      exactFields(row.firstBuildObservations.metrics, new Set(['passRate', 'coverageRate']),
        `${at}.firstBuildObservations.metrics`);
      if (!Number.isSafeInteger(row.firstBuildObservations.sample.selectedAttempts)
        || row.firstBuildObservations.sample.selectedAttempts < 1
        || !Number.isSafeInteger(row.firstBuildObservations.sample.measuredAttempts)
        || row.firstBuildObservations.sample.measuredAttempts < 0
        || row.firstBuildObservations.sample.measuredAttempts
          > row.firstBuildObservations.sample.selectedAttempts) {
        throw new Error(`${at}.firstBuildObservations.sample is invalid`);
      }
      for (const metric of ['passRate', 'coverageRate'] as const) {
        const summary = row.firstBuildObservations.metrics[metric];
        exactFields(summary, new Set(['n', 'center', 'spread', 'min', 'max']),
          `${at}.firstBuildObservations.metrics.${metric}`);
        if (!Number.isSafeInteger(summary.n) || summary.n < 0
          || (summary.center !== null && (!Number.isFinite(summary.center)
            || summary.center < 0 || summary.center > 1))) {
          throw new Error(`${at}.firstBuildObservations.metrics.${metric} is invalid`);
        }
      }
    }
  }
  if (!['qualified', 'pending'].includes(report.scope.grading.status)
    || !Array.isArray(report.scope.grading.levels)
    || report.scope.grading.levels.length !== report.scope.levels.length) {
    throw new Error('campaign report.scope.grading is invalid');
  }
  for (const [index, level] of report.scope.grading.levels.entries()) {
    const at = `campaign report.scope.grading.levels[${index}]`;
    exactFields(level, new Set(['level', 'status', 'reasons', 'evidenceSha256']), at);
    if (!Number.isSafeInteger(level.level)
      || !['qualified', 'pending'].includes(level.status)
      || !Array.isArray(level.reasons)
      || level.reasons.some(reason => typeof reason !== 'string' || !reason)
      || (level.evidenceSha256 !== null && !/^[a-f0-9]{64}$/.test(level.evidenceSha256))
      || (level.status === 'qualified' && level.reasons.length > 0)
      || (level.status === 'pending' && level.reasons.length === 0)) {
      throw new Error(`${at} is invalid`);
    }
  }
  const expectedGradingStatus = report.scope.grading.levels.some(level => level.status === 'pending')
    ? 'pending' : 'qualified';
  if (report.scope.grading.status !== expectedGradingStatus) {
    throw new Error('campaign report.scope.grading status does not match its levels');
  }
  for (const [index, attempt] of report.attempts.entries()) {
    if (!isRecord(attempt) || typeof attempt.id !== 'string' || !attempt.id
      || typeof attempt.status !== 'string' || !Array.isArray(attempt.executions)
      || (attempt.metrics !== null && !isRecord(attempt.metrics))) {
      throw new Error(`campaign report.attempts[${index}] is invalid`);
    }
    completionSchema.parse(attempt.completion);
    if (attempt.timeBudget) {
      const budget = attempt.timeBudget;
      for (const grant of budget.grants) timeGrantReceiptSchema.parse(grant);
      const accepted = budget.grants.filter(grant => grant.disposition === 'accepted');
      if (!Number.isSafeInteger(budget.originalMinutes) || budget.originalMinutes <= 0
        || !Number.isSafeInteger(budget.consumedMs) || budget.consumedMs < 0
        || budget.extensionCount !== accepted.length
        || budget.effectiveMinutes !== budget.originalMinutes
          + accepted.reduce((sum, grant) => sum + grant.request.minutes, 0)) {
        throw new Error(`campaign report.attempts[${index}].timeBudget is invalid`);
      }
    }
    for (const checkpoint of attempt.curve.checkpoints) checkpointSchema.parse(checkpoint);
    if (canonicalDefinitionJson(attempt.curve) !== canonicalDefinitionJson(completionCurve(
      attempt.curve.checkpoints, report.policy.spendThresholdsUsd ?? [],
      report.policy.completionTargets ?? []))) {
      throw new Error(`campaign report.attempts[${index}].curve does not match checkpoints`);
    }
    for (const group of attempt.groups) {
      completionSchema.parse(group.completion);
      costEvidenceSchema.parse(group.cost);
    }
    for (const [executionIndex, execution] of attempt.executions.entries()) {
      if (!isRecord(execution) || typeof execution.id !== 'string' || !execution.id
        || typeof execution.status !== 'string') {
        throw new Error(`campaign report.attempts[${index}].executions[${executionIndex}] is invalid`);
      }
      costEvidenceSchema.parse(execution.cost);
      if (execution.recorded !== undefined) costEvidenceSchema.parse(execution.recorded);
      if (execution.providerContinuation) {
        const assessment = execution.providerContinuation;
        if (assessment.eligible !== false || !['paid', 'zero-usage-candidate', 'unknown'].includes(assessment.work)
          || typeof assessment.reason !== 'string' || !assessment.reason) {
          throw new Error('Invalid historical provider continuation assessment');
        }
        costEvidenceSchema.parse(assessment.sessionCost);
        costEvidenceSchema.parse(assessment.runCost);
      }
      if (execution.providerWaits) {
        const waits = execution.providerWaits;
        if (![waits.waits, waits.waitedMs, waits.continued, waits.stopped, waits.waiting ?? 0]
          .every(value => Number.isSafeInteger(value) && value >= 0)
          || waits.waits !== waits.continued + waits.stopped + (waits.waiting ?? 0)
          || (waits.durationKind !== undefined && !['exact', 'lower-bound'].includes(waits.durationKind))) {
          throw new Error('Invalid provider wait summary');
        }
      }
      for (const kind of ['build', 'repair', 'resume'] as const) costEvidenceSchema.parse(execution.usage[kind]);
      for (const key of ['input', 'output', 'cacheWrite', 'cacheRead'] as const) {
        const value = execution.usage[key];
        if (value !== null && (!Number.isSafeInteger(value) || value < 0)) {
          throw new Error(`campaign report execution usage.${key} is invalid`);
        }
      }
    }
    if (canonicalDefinitionJson(attempt.spend) !== canonicalDefinitionJson(executionSpend(attempt.executions))) {
      throw new Error(`campaign report.attempts[${index}].spend does not match executions`);
    }
  }
  if (typeof report.summary.campaignStatus !== 'string'
    || canonicalDefinitionJson(report.summary)
      !== canonicalDefinitionJson(reportSummary(report.attempts, report.summary.campaignStatus))) {
    throw new Error('campaign report summary does not match its attempts');
  }
  if (canonicalDefinitionJson(report.conditions)
    !== canonicalDefinitionJson(reportConditions(report.attempts, report.policy))) {
    throw new Error('campaign report conditions do not match its attempts');
  }
  if (report.limitations.some(item => typeof item !== 'string' || !item)) {
    throw new Error('campaign report limitations are invalid');
  }
  const canonical = canonicalizeDefinition(report) as unknown as CampaignReport;
  const { contentSha256, ...body } = canonical;
  if (typeof contentSha256 !== 'string'
    || contentSha256 !== sha256(canonicalDefinitionJson(body))) {
    throw new Error('campaign report content identity is invalid');
  }
  return canonical;
}

export function buildCampaignReport(plan: CompiledCampaignPlan, state: CampaignState,
  readRun: (attempt: CampaignAttemptPlan, execution: CampaignExecution) => BenchmarkRun,
  readProviderWaits?: (attempt: CampaignAttemptPlan, execution: CampaignExecution) => ProviderWaitSummary | null,
): CampaignReport {
  if (state.campaignSha256 !== plan.contentSha256) throw new Error('report state does not match campaign plan');
  const rows: CampaignReportAttempt[] = [];
  for (const attempt of state.attempts) {
    const runs = new Map<string, BenchmarkRun>();
    const executions = attempt.executions.map(execution => {
      let run: BenchmarkRun | null = null;
      try {
        run = readRun(attempt.plan, execution);
      } catch (error) {
        if (execution.status === 'completed') throw error;
      }
      if (execution.status === 'completed') {
        if (!run || run.outcome?.kind !== execution.outcome) {
          throw new Error(`completed execution ${execution.id} has missing or mismatched run evidence`);
        }
      }
      if (run) runs.set(execution.id, run);
      // Reinterpret incomplete measurement in old completed states without rewriting evidence.
      const incompleteMeasurement = run && ((run.progressionStatus !== undefined
        && run.progressionStatus.phase !== 'terminal') || (run.outcome?.inconclusive?.length ?? 0) > 0);
      const classified = execution.status === 'completed' && incompleteMeasurement
        ? classifyCampaignExecution({ exitCode: execution.exitCode, run }) : execution;
      const retainedCost = run && execution.status !== 'running' ? retainedRunCost(run) : null;
      return {
        id: execution.id, ordinal: execution.ordinal, status: classified.status,
        outcome: classified.outcome, reason: classified.reason, exitCode: execution.exitCode,
        ...(classified.status !== execution.status || classified.outcome !== execution.outcome
          ? { recordedStatus: execution.status, recordedOutcome: execution.outcome } : {}),
        startedAt: execution.startedAt, completedAt: execution.completedAt,
        admissionId: execution.admissionId,
        admissionEvidence: `admissions/${execution.admissionId}.json`,
        cost: retainedCost?.cost ?? runCostEvidence(run, 'execution'),
        ...(retainedCost ? { recorded: retainedCost.recorded } : {}),
        usage: executionUsage(run),
        providerContinuation: ['running', 'pending'].includes(execution.status)
          ? null : assessStoppedProviderContinuation(run),
        providerWaits: readProviderWaits?.(attempt.plan, execution) ?? providerWaitSummary(run),
        evidence: run
          ? `${execution.output}/${ARTIFACT_FILE.run}` : CAMPAIGN_FILE.state,
        metrics: run ? campaignRunMetrics(run) : null,
        firstBuildObservations: run ? campaignRunFirstBuildObservations(run) : null,
      };
    });
    const latest = executions.at(-1);
    const latestRun = latest ? runs.get(latest.id) : undefined;
    const checkpoints = executions.flatMap((execution, index) =>
      (runs.get(execution.id)?.checkpoints ?? []).map(checkpoint => ({ ...checkpoint,
        excluded: execution.status === 'invalid',
        evidence: { ...checkpoint.evidence, path: `${execution.evidence.slice(0, -ARTIFACT_FILE.run.length)}${checkpoint.evidence.path}` },
        cost: sumCostEvidence([checkpoint.executionCost, ...executions.slice(0, index).map(item => item.cost)]),
      }))).map((checkpoint, index) => ({ ...checkpoint, sequence: index + 1 }));
    const selected = attempt.plan.condition.requested.levels.flatMap(level =>
      (level.selection.scoredChecks ?? []).map(check => ({ id: check.stableKey, points: check.points })));
    const accepted = latestRun?.checkpoints?.findLast(checkpoint => checkpoint.accepted);
    const outcomes = new Map<string, CheckStatus>((accepted?.checks ?? []).map(check => [check.id, check.status]));
    const completion = latestRun?.progressionStatus?.score?.completion ?? checkCompletion(selected, outcomes);
    const costForRun = (run: BenchmarkRun, seen = new Set<string>()): CostEvidence => {
      if (!run.progressionResume) return runCostEvidence(run, 'execution');
      const priorId = run.progressionResume.priorRunId;
      const prior = [...runs.values()].find(candidate => candidate.id === priorId);
      if (!prior || seen.has(priorId)) return { status: 'unknown', costUsd: null };
      seen.add(priorId);
      return sumCostEvidence([runCostEvidence(run, 'execution'), costForRun(prior, seen)]);
    };
    const validCost = latestRun ? costForRun(latestRun) : undefined;
    const metrics = latest?.status === 'completed' ? { ...latest.metrics,
      totalCostUsd: validCost?.status === 'exact' ? validCost.costUsd : null,
      totalCostUpperBoundUsd: validCost?.status === 'upper-bound' ? validCost.costUsd : null } : null;
    const groups = plan.featureCatalog?.definition.questlines.map(group => {
      let total = 0;
      let separable = checkpoints.length > 0;
      for (const [index, point] of checkpoints.entries()) {
        const before = index > 0 ? checkpoints[index - 1]!.cost : { status: 'exact' as const, costUsd: 0 };
        const owners = new Set(point.workNodeIds.map(id => plan.featureCatalog!.definition.nodes.find(node => node.id === id)?.questline));
        if (!owners.size || owners.has(undefined) || (owners.has(group.id) && (owners.size > 1 || before.status !== 'exact' || point.cost.status !== 'exact'))) {
          separable = false;
        } else if (owners.has(group.id)) total += point.cost.costUsd! - before.costUsd!;
      }
      return { id: group.id, title: group.title,
      completion: latestRun?.progressionStatus?.score?.questlines?.find(item => item.id === group.id)?.completion
        ?? checkCompletion(plan.featureCatalog!.definition.nodes.filter(node => group.nodes.includes(node.id))
          .flatMap(node => node.gradingChecks).filter(check => selected.some(item => item.id === check.id)), outcomes),
      cost: separable && total >= 0 ? { status: 'exact' as const, costUsd: Number(total.toFixed(6)) }
        : { status: 'unknown' as const, costUsd: null },
      costAttribution: separable ? 'measured-work' as const : 'not-separable' as const };
    }) ?? [];
    rows.push({ ...attempt.plan, status: attempt.status === 'completed' && latest?.status === 'invalid'
      ? 'invalid' : attempt.status, executions,
      ...(attempt.timeGrants?.length ? {
        timeBudget: campaignTimeBudget(plan, attempt, Date.parse(state.updatedAt)),
      } : {}),
      metrics, completion, groups, spend: executionSpend(executions),
      curve: completionCurve(checkpoints, plan.definition.analysis.spendThresholdsUsd ?? [],
        plan.definition.analysis.completionTargets ?? []),
      firstBuildObservations: latest?.status === 'completed'
        ? latest.firstBuildObservations : null });
  }
  const conditions = reportConditions(rows, plan.definition.analysis);
  const grading = campaignGradingQualification(plan);
  const body = canonicalizeDefinition({
    reportSchemaVersion: CAMPAIGN_REPORT_SCHEMA_VERSION,
    campaign: { id: plan.id, version: plan.version, state: plan.state,
      sha256: plan.contentSha256, title: plan.title },
    scope: { track: plan.definition.track, levels: plan.definition.levels,
      selection: plan.definition.selection, bindings: plan.bindings, grading, stacks: plan.stacks,
      conditions: plan.conditions,
      agents: plan.agents, repetitions: plan.definition.repetitions,
      repetitionsByStack: plan.summary.repetitionsByStack,
      parallelism: plan.summary.parallelism,
      runtime: plan.definition.runtime, pricing: plan.definition.pricing },
    policy: plan.definition.analysis,
    attempts: rows,
    conditions,
    summary: reportSummary(rows, state.status),
    limitations: [
      ...new Set(state.attempts.filter(attempt => attempt.extension).map(attempt => {
        const seed = attempt.extension!;
        return `Seeded continuation from ${seed.parent.campaignId} at L${seed.fromDepth}. Costs exclude the parent build. The source retains prior repairs; this is not a fresh first-build or uninterrupted-session result.`;
      })),
      ...(grading.status === 'pending'
        ? ['Grading qualification is pending. Treat these scores as provisional.'] : []),
      'Statistics describe only the exact scope and conditions recorded above.',
      'Score rates are passed points over all selected points; the questline average weighs every questline equally and is a secondary view.',
      'Check completion counts passed checks over the full selected scope, separately from weighted scores. Missing checkpoints cannot establish a cost/completion curve.',
      'Failed checks are observations, not a count of independent bugs. Findings describe symptoms; confirmed root causes require review of the retained evidence.',
      'UI visibility does not establish server authorization, and a missing stock number does not establish overselling. Use the direct-call and stored-state evidence for those claims.',
      'A spread is reported only from three or more completed attempts; below that, only the centre and the range.',
      'Usage in USD comes from retained receipts at the recorded API rates. Upper bounds remain marked; unknown spend is not zero. These are API-equivalent costs, not invoices.',
      'knownCostUsd sums available exact amounts and upper bounds. With bounded or unknown executions, it is not exact spend and must not be assumed to be a lower bound.',
      'Group costs are not allocated when one coding session covers several groups. No token spend is attributed by guessing.',
      'Durations exclude time spent waiting for the provider to lift a rate limit.',
      'Results describe only the agents and model ids recorded per attempt. They do not generalize to other agents.',
      ...(plan.agents.some(agent => agent.adapter === 'reference-fixture')
        ? ['Reference-fixture attempts use hand-written apps and make no model calls. They do not measure model implementation ability or comparative token efficiency.'] : []),
      'Observed specifications are diagnostic, contribute zero score, and are never shown to repairs.',
      'Invalid executions are excluded from outcome metrics; their measured spend remains in total spend.',
      ...(rows.some(attempt => attempt.executions.some(execution => execution.providerWaits))
        ? ['Provider wait continued counts mean accepted operator requests, not completed model work.'] : []),
      ...(rows.some(attempt => attempt.timeBudget?.extensionCount)
        ? ['Time extensions are recorded per logical attempt. They do not alone invalidate efficacy results; extended attempts do not represent the original fixed-time limit.'] : []),
      'Outcome classifications come from recorded artifacts. Review notes do not change them. Do not publish affected comparisons after a grader defect is confirmed until corrected grading evidence is available.',
      'The report makes no causal claim beyond the declared campaign design.',
    ],
  }) as unknown as Omit<CampaignReport, 'contentSha256'>;
  return validateCampaignReport({ ...body, contentSha256: sha256(canonicalDefinitionJson(body)) });
}

function escape(value: unknown): string {
  return String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;').replaceAll('"', '&quot;');
}

export function renderCampaignHtml(report: CampaignReport,
  { evidencePrefix = '..' }: { evidencePrefix?: string } = {},
): string {
  report = validateCampaignReport(report);
  const coverageMetric = report.policy.primaryMetric === 'finalScoreRate'
    ? 'finalCoverageRate' : report.policy.primaryMetric === 'firstBuildScoreRate'
      ? 'firstBuildCoverageRate' : null;
  const primaryLabel = report.policy.primaryMetric === 'finalScoreRate'
    ? 'Final score' : report.policy.primaryMetric === 'firstBuildScoreRate'
      ? 'First-build score' : report.policy.primaryMetric;
  const rows = report.conditions.map(condition => `<tr><td>${escape(condition.stack)}</td>`
    + `<td>${escape(condition.agent.adapter)} / ${escape(condition.agent.model)}${condition.agent.providerRoute ? ` / ${escape(condition.agent.providerRoute)}` : ''}</td>`
    + `<td>${escape(condition.condition.id)}</td>`
    + `<td>${condition.sample.completedAttempts}/${condition.sample.plannedAttempts}</td>`
    + `<td>${condition.sample.invalidExecutions}/${condition.sample.executions}</td>`
    + `<td>${escape(formatMetric(report.policy.primaryMetric,
      condition.metrics[report.policy.primaryMetric]?.center))}`
    + ` (n=${condition.metrics[report.policy.primaryMetric]?.n ?? 0})`
    + `<br><small>${coverageMetric
      ? `${escape(formatRate(condition.metrics[coverageMetric]?.center))} coverage · ` : ''}`
    + `${condition.metrics.questlineAverageRate?.center != null
      ? `${escape(formatRate(condition.metrics.questlineAverageRate.center))} questline average · ` : ''}`
    + `${escape(formatUsd(condition.metrics.totalCostUsd?.center))} API-equivalent usage`
    + `${condition.metrics.totalTokens?.center != null
      ? ` (${escape(formatTokens(condition.metrics.totalTokens.center))})` : ''} · `
    + `${escape(formatDurationMs(condition.metrics.totalDurationMs?.center))}</small>`
    + `<br><small>Check completion: ${escape(formatRate(condition.metrics.checkCompletionRate?.center))}`
    + ` · All execution spend: ${escape(formatCostEvidence(condition.spend))}`
    + ` · ${condition.spend.unknownExecutions} unknown, ${condition.spend.boundedExecutions} bounded execution(s)</small></td></tr>`).join('');
  const treatmentRows = report.scope.conditions.flatMap(condition =>
    (condition.requested?.levels ?? []).flatMap(level => {
      const specifications = level.selection?.schemaVersion === 3
        ? level.selection.specifications : null;
      if (!specifications) return [];
      const list = (values: string[]): string => values.length ? values.join(', ') : 'none';
      return [`<tr><td>${escape(condition.id)}</td>`
        + `<td>L${escape(level.level)}</td>`
        + `<td>${escape(list(specifications.requested ?? []))}</td>`
        + `<td>${escape(list(specifications.expected ?? []))}</td>`
        + `<td>${escape(list(specifications.observed ?? []))}</td></tr>`];
    })).join('');
  const treatmentSection = treatmentRows
    ? `<h2>What this run asks for and tests</h2><p>The build brief lists what the coding agent is asked to build. Scored checks affect the result and may be included in repair feedback. Additional measurements are reported separately and do not affect the score or repairs.</p><table><thead><tr><th>Run setup</th><th>Level</th><th>Build brief + score</th><th>Score</th><th>Additional measurements</th></tr></thead><tbody>${treatmentRows}</tbody></table>`
    : '';
  const observationRows = report.conditions.filter((condition): condition is CampaignReportCondition & {
    firstBuildObservations: NonNullable<CampaignReportCondition['firstBuildObservations']>;
  } => condition.firstBuildObservations !== null)
    .map(condition => `<tr><td>${escape(condition.stack)}</td>`
      + `<td>${escape(condition.agent.adapter)} / ${escape(condition.agent.model)}${condition.agent.providerRoute ? ` / ${escape(condition.agent.providerRoute)}` : ''}</td>`
      + `<td>${escape(condition.condition.id)}</td>`
      + `<td>${condition.firstBuildObservations.sample.measuredAttempts}/${condition.firstBuildObservations.sample.selectedAttempts}</td>`
      + `<td>${escape(formatRate(condition.firstBuildObservations.metrics.passRate.center))}`
      + `<br><small>${escape(formatRate(condition.firstBuildObservations.metrics.coverageRate.center))} coverage</small></td></tr>`)
    .join('');
  const observationSection = observationRows ? `<h2>Additional first-build measurements</h2><p>These checks record selected behavior in the original build. They are shown separately, add no points to the score, and do not enter repair feedback.</p><table><thead><tr><th>Stack</th><th>Agent / model</th><th>Run setup</th><th>Measured</th><th>Pass rate</th></tr></thead><tbody>${observationRows}</tbody></table>` : '';
  const qualificationWarning = report.scope.grading.status === 'pending'
    ? '<div class="warn"><strong>Provisional scores.</strong> Grading qualification is pending.</div>'
    : '';
  const curves = report.attempts.map(attempt => `<section><h3>${escape(attempt.id)}</h3>`
    + `<p>Check completion: ${attempt.completion.passed}/${attempt.completion.selected}`
    + ` (${escape(formatRate(attempt.completion.rate))}). ${attempt.completion.failed} failed, `
    + `${attempt.completion.blocked} blocked, ${attempt.completion.unmeasured} `
    + `${attempt.mode.id === 'dependency' ? 'without an accepted outcome' : 'unmeasured'}.</p>`
    + (attempt.mode.id === 'dependency'
      ? '<p>Blocked descendants receive no completion credit, even when a raw check passed. A guarantee may be deferred after a prerequisite fails. '
        + 'The saved summary does not separate prerequisite deferral from missing conclusive evidence in the unmeasured count. '
        + 'Use the linked grade evidence for raw outcomes; neither condition changes the full selected denominator.</p>' : '')
    + (attempt.curve.checkpoints.length ? '<table><thead><tr><th>Measurement</th><th>Phase</th><th>Completion</th><th>All execution spend</th><th>Source</th></tr></thead><tbody>'
      + attempt.curve.checkpoints.map(point => `<tr><td>${point.sequence}${point.excluded ? ' (excluded execution)' : point.accepted ? '' : ' (rejected regression)'}</td>`
        + `<td>${escape(point.phase)}</td><td>${point.completion.passed}/${point.completion.selected}</td>`
        + `<td>${escape(formatCostEvidence(point.cost))}</td><td><a href="${escape(`${evidencePrefix}/${point.evidence.path}`)}">Grade evidence</a><br><code>${escape(point.sourceSha256)}</code></td></tr>`).join('')
      + '</tbody></table>' : '<p>No recorded cost/completion checkpoints. Progress between grades is not estimated.</p>')
    + (attempt.curve.completionAtSpend.length ? '<ul>' + attempt.curve.completionAtSpend.map(point =>
      `<li>At or below ${escape(formatUsd(point.budgetUsd))}: ${point.completion
        ? `${point.completion.passed}/${point.completion.selected} at measurement ${point.sequence}` : 'unmeasured'}</li>`).join('') + '</ul>' : '')
    + (attempt.curve.costToCompletion.length ? '<ul>' + attempt.curve.costToCompletion.map(point =>
      `<li>${escape(formatRate(point.targetRate))} completion: ${point.status === 'reached'
        ? escape(formatCostEvidence(point.cost)) : escape(point.status)}</li>`).join('') + '</ul>' : '')
    + (attempt.groups.length ? '<table><thead><tr><th>Feature group</th><th>Completion</th><th>Cost attribution</th></tr></thead><tbody>'
      + attempt.groups.map(group => `<tr><td>${escape(group.title)}</td><td>${group.completion.passed}/${group.completion.selected}</td>`
        + `<td>${group.costAttribution === 'measured-work' ? escape(formatCostEvidence(group.cost))
          : 'Not separable from shared coding sessions'}</td></tr>`).join('') + '</tbody></table>' : '')
    + '<ul>' + attempt.executions.map(execution => `<li>${escape(execution.id)}: ${escape(formatCostEvidence(execution.cost))}`
      + ` · build ${escape(formatCostEvidence(execution.usage.build))}, repair ${escape(formatCostEvidence(execution.usage.repair))}, resume ${escape(formatCostEvidence(execution.usage.resume))}`
      + ` · input ${escape(execution.usage.input ?? 'unknown')}, output ${escape(execution.usage.output ?? 'unknown')}, cache read ${escape(execution.usage.cacheRead ?? 'unknown')}, cache write ${escape(execution.usage.cacheWrite ?? 'unknown')}`
      + (execution.providerWaits ? ` · provider waits ${execution.providerWaits.waits}, ${execution.providerWaits.durationKind === 'lower-bound' ? 'at least ' : ''}${escape(formatDurationMs(execution.providerWaits.waitedMs))}, continued ${execution.providerWaits.continued}, stopped ${execution.providerWaits.stopped}, unclosed ${execution.providerWaits.waiting ?? 0}` : '')
      + (execution.providerContinuation ? `<p>Provider continuation ineligible (${escape(execution.providerContinuation.work)}): ${escape(execution.providerContinuation.reason)}</p>` : '')
      + '</li>').join('')
    + '</ul></section>').join('');
  return `<!doctype html>\n<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${escape(report.campaign.title)}</title><style>body{font:16px system-ui;max-width:1100px;margin:40px auto;padding:0 20px;color:#17202a}code{font-size:.85em}table{border-collapse:collapse;width:100%}th,td{padding:.65rem;border-bottom:1px solid #ccd;text-align:left}.meta{color:#566} .warn{background:#fff4cf;padding:1rem}</style></head><body><h1>${escape(report.campaign.title)}</h1><p class="meta">Campaign <code>${escape(report.campaign.id)}</code> · ${escape(report.campaign.sha256)} · status ${escape(report.summary.campaignStatus)}</p>${qualificationWarning}<p>This report shows exactly what ran: ${report.summary.completedAttempts} completed of ${report.summary.plannedAttempts} planned attempts, with ${report.summary.invalidExecutions} invalid execution(s) retained.</p><h2>Conditions</h2><table><thead><tr><th>Stack</th><th>Agent / model</th><th>Study condition</th><th>Completed</th><th>Invalid executions</th><th>${escape(primaryLabel)}</th></tr></thead><tbody>${rows}</tbody></table>${treatmentSection}${observationSection}<h2>Cost and measured completion</h2>${curves}<h2>Scope</h2><pre>${escape(JSON.stringify(report.scope, null, 2))}</pre><h2>Attempts and raw evidence</h2><ul>${report.attempts.map(attempt => `<li><strong>${escape(attempt.id)}</strong> — ${escape(attempt.status)}${attempt.executions.map(execution => ` · <a href="${escape(`${evidencePrefix}/${execution.evidence}`)}">${escape(execution.id)}</a> (${escape(execution.outcome ?? execution.status)}) · <a href="${escape(`${evidencePrefix}/${execution.admissionEvidence}`)}">admission</a>${(execution.firstBuildObservations?.levels ?? []).filter(level => level.artifact).map(level => ` · <a href="${escape(`${evidencePrefix}/${execution.evidence.slice(0, -ARTIFACT_FILE.run.length)}${level.artifact}`)}">L${escape(level.level)} observations</a>`).join('')}`).join('')}</li>`).join('')}</ul><div class="warn"><strong>Limitations</strong><ul>${report.limitations.map(item => `<li>${escape(item)}</li>`).join('')}</ul></div><p class="meta">Report identity: <code>${escape(report.contentSha256)}</code></p></body></html>\n`;
}

export interface GeneratedCampaignReport { report: CampaignReport; reportPath: string;
  htmlPath: string; exportManifestPath: string; relativeOutput: string }

export function generateCampaignReport(directory: string,
  { output: requestedOutput }: { output?: string } = {},
): GeneratedCampaignReport {
  let output = requestedOutput ?? join(resolve(directory), 'report');
  const { plan, state, paths } = inspectCampaign(directory,
    { requireCurrentInputs: false, allowRelocatedEvidence: true });
  if (state.status === 'running') throw new Error('cannot report while a campaign attempt is running');
  output = campaignChildPath(paths.root, output, 'report output');
  const outputRelative = relative(paths.root, output);
  const report = buildCampaignReport(plan, state, (attempt, execution) => {
    const run = readArtifactPayload(join(paths.root, execution.output, ARTIFACT_FILE.run),
      { expectedKind: 'benchmark_run' });
    if (execution.status === 'completed') {
      validateCampaignRun(plan, attempt, run, { resultDir: join(paths.root, execution.output) });
    } else if ((run.artifactEnvelope as { attempt?: { parentId?: string } } | undefined)?.attempt?.parentId !== attempt.id
      || run.backend !== attempt.stack
      || run.model !== attempt.model || canonicalDefinitionJson(run.pricing) !== canonicalDefinitionJson(attempt.pricing)) {
      throw new Error(`invalid execution ${execution.id} cost evidence belongs to another attempt`);
    }
    return run as BenchmarkRun;
  }, (attempt, execution) => readCampaignProviderWaitHistory(paths.root, attempt.id, execution.id));
  mkdirSync(output, { recursive: true });
  const reportPath = join(output, CAMPAIGN_FILE.reportJson);
  writeArtifact(reportPath, { kind: 'campaign_report', id: `${plan.id}-report-${report.contentSha256.slice(0, 16)}`,
    timestamps: { startedAt: state.createdAt, completedAt: state.updatedAt },
    identities: emptyArtifactIdentities({ experiment: {
      id: plan.id, version: plan.version, sha256: plan.contentSha256, state: plan.state,
    } }), payload: report });
  const htmlPath = join(output, CAMPAIGN_FILE.reportHtml);
  const evidencePrefix = relative(output, paths.root).replaceAll('\\', '/') || '.';
  const temporaryHtml = `${htmlPath}.${process.pid}.${randomUUID()}.tmp`;
  writeFileSync(temporaryHtml, renderCampaignHtml(report, { evidencePrefix }), { flag: 'wx' });
  renameSync(temporaryHtml, htmlPath);
  const exportManifestPath = join(output, 'export-manifest.json');
  writeFileSync(exportManifestPath, `${JSON.stringify(publicExportManifest(paths.root, report, htmlPath), null, 2)}\n`);
  return { report, reportPath, htmlPath, exportManifestPath,
    relativeOutput: outputRelative.replaceAll('\\', '/') };
}

/** Index only: never copy transcripts, source trees, or private authority into an export. */
function publicExportManifest(root: string, report: CampaignReport, htmlPath: string) {
  const allowed = new Set(['campaign_plan', 'campaign_state', 'campaign_admission', 'campaign_report',
    'benchmark_run', 'grade_bundle', 'grade', 'action_check', 'contract_lint', 'progression_state',
    'source_checkpoint', 'reference_qualification', 'mutation_control', 'null_control']);
  const files: Array<{ path: string; kind: string; bytes: number; sha256: string }> = [];
  const omitted = { privateDirectories: 0, symbolicLinks: 0, otherFiles: 0, invalidArtifacts: 0 };
  const visit = (directory: string): void => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (entry.name === 'export-manifest.json') continue;
      const path = join(directory, entry.name);
      if (entry.isSymbolicLink()) { omitted.symbolicLinks++; continue; }
      if (entry.isDirectory()) {
        if (entry.name.startsWith('.') || ['source', 'node_modules', 'transcripts'].includes(entry.name)) {
          omitted.privateDirectories++; continue;
        }
        visit(path); continue;
      }
      if (!entry.isFile() || (!entry.name.endsWith('.json') && path !== htmlPath)) {
        omitted.otherFiles++; continue;
      }
      let kind = 'report_html';
      if (path !== htmlPath) {
        try { kind = readArtifact(path).kind; }
        catch { omitted.invalidArtifacts++; continue; }
        if (!allowed.has(kind)) { omitted.otherFiles++; continue; }
      }
      const bytes = readFileSync(path);
      files.push({ path: relative(root, path).replaceAll('\\', '/'), kind,
        bytes: bytes.length, sha256: sha256(bytes) });
    }
  };
  visit(root);
  return { schemaVersion: 1, campaignSha256: report.campaign.sha256, reportSha256: report.contentSha256,
    scope: 'Manifest of schema-validated public artifacts and generated report HTML. Paths are relative to the campaign directory. No files are copied.',
    reconstruction: 'Partial. Compiled plans and artifact identities retain prompt, definition, reference, and source hashes. Source files, raw transcripts, media, private credentials, and external referenced evidence are omitted; retain their original owners for full reconstruction. Review free-text artifact content before public release.',
    files: files.sort((a, b) => a.path.localeCompare(b.path)), omitted };
}

/** Blank cells mean unknown or unavailable, never zero. Quote and neutralize spreadsheet formulas. */
export function campaignReportCsv(report: CampaignReport): Record<string, string> {
  const csv = (rows: unknown[][]): string => rows.map(row => row.map(value => {
    let text = value === null || value === undefined ? '' : String(value);
    if (typeof value === 'string' && /^[\s]*[=+@-]/.test(text)) text = `'${text}`;
    return `"${text.replaceAll('"', '""')}"`;
  }).join(',')).join('\r\n') + '\r\n';
  return {
    'attempts.csv': csv([
      ['campaignId', 'campaignSha256', 'conditionId', 'conditionSha256', 'agentAdapter', 'levels',
        'attempt', 'stack', 'model', 'repetition', 'status', 'selectedChecks', 'diagnosticPassedChecks',
        'diagnosticFailedChecks', 'diagnosticBlockedChecks', 'diagnosticUnmeasuredChecks', 'checkCompletionRate', 'weightedScoreRate',
        'outcomeCostUsd', 'outcomeCostUpperBoundUsd', 'allExecutionCostStatus', 'allExecutionCostUsd',
        'sumOfAvailableExactCostsAndUpperBoundsUsd', 'unknownExecutions', 'boundedExecutions', 'totalTokens', 'durationMs',
        'originalTimeLimitMinutes', 'effectiveTimeLimitMinutes', 'consumedExecutionMs', 'timeExtensions', 'providerRoute', 'maxOutputTokens'],
      ...report.attempts.map(attempt => [report.campaign.id, report.campaign.sha256,
        attempt.condition.id, attempt.condition.contentSha256, attempt.agentAdapter, attempt.levels.join(';'),
        attempt.id, attempt.stack, attempt.model, attempt.repetition,
        attempt.status, attempt.completion.selected, attempt.completion.passed, attempt.completion.failed,
        attempt.completion.blocked, attempt.completion.unmeasured, attempt.metrics?.checkCompletionRate,
        attempt.metrics?.finalScoreRate, attempt.metrics?.totalCostUsd,
        attempt.metrics?.totalCostUpperBoundUsd, attempt.spend.status, attempt.spend.costUsd,
        attempt.spend.knownCostUsd, attempt.spend.unknownExecutions, attempt.spend.boundedExecutions,
        attempt.metrics?.totalTokens, attempt.metrics?.totalDurationMs,
        attempt.timeBudget?.originalMinutes, attempt.timeBudget?.effectiveMinutes,
        attempt.timeBudget?.consumedMs, attempt.timeBudget?.extensionCount, attempt.providerRoute, attempt.maxOutputTokens]),
    ]),
    'executions.csv': csv([
      ['attempt', 'execution', 'stack', 'status', 'outcome', 'recordedStatus', 'recordedOutcome', 'costStatus', 'costUsd',
        'inputTokens', 'outputTokens', 'cacheWriteTokens', 'cacheReadTokens', 'evidence', 'admissionEvidence',
        'providerWaits', 'providerWaitMs', 'providerContinued', 'providerStopped', 'providerUnclosed', 'providerWaitDurationKind', 'providerContinuationWork', 'providerContinuationReason'],
      ...report.attempts.flatMap(attempt => attempt.executions.map(execution => [attempt.id,
        execution.id, attempt.stack, execution.status, execution.outcome,
        execution.recordedStatus ?? execution.status, execution.recordedOutcome ?? execution.outcome, execution.cost.status,
        execution.cost.costUsd, execution.usage.input, execution.usage.output, execution.usage.cacheWrite,
        execution.usage.cacheRead, execution.evidence, execution.admissionEvidence,
        execution.providerWaits?.waits, execution.providerWaits?.waitedMs,
        execution.providerWaits?.continued, execution.providerWaits?.stopped,
        execution.providerWaits?.waiting, execution.providerWaits?.durationKind,
        execution.providerContinuation?.work, execution.providerContinuation?.reason])),
    ]),
  };
}

export function exportCampaignReport(directory: string, destination: string): string {
  const root = realpathSync(directory);
  const requested = resolve(destination);
  if (lstatSync(requested, { throwIfNoEntry: false })) throw new Error('export destination must not exist');
  // Resolve the existing parent so a symlink cannot place the export inside the campaign.
  const output = join(realpathSync(dirname(requested)), basename(requested));
  if (output === root || output.startsWith(`${root}${sep}`)) {
    throw new Error('export destination must be outside the campaign directory');
  }
  const generated = generateCampaignReport(root);
  const manifest = publicExportManifest(root, generated.report, generated.htmlPath);
  const staging = mkdtempSync(join(dirname(output), '.stack-bench-export-'));
  try {
    for (const file of manifest.files) {
      const source = campaignChildPath(root, file.path, 'export source');
      const bytes = readFileSync(source);
      if (bytes.length !== file.bytes || sha256(bytes) !== file.sha256) {
        throw new Error(`export source changed: ${file.path}`);
      }
      const target = campaignChildPath(staging, file.path, 'export target');
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, bytes, { flag: 'wx' });
    }
    const additional = {
      ...campaignReportCsv(generated.report),
      'README.txt': 'Partial research export. Open report/report.html.\n'
        + 'CSV cells left blank mean unknown or unavailable. USD is API-equivalent usage, not an invoice.\n'
        + 'Attempt outcome metrics exclude invalid executions; execution rows and all-execution spend retain them.\n'
        + 'Source, transcripts, media, private directories and external evidence are omitted.\n'
        + 'Links to omitted evidence will not open offline. Review free-text content before publication.\n',
    };
    for (const [path, text] of Object.entries(additional)) {
      const bytes = Buffer.from(text);
      writeFileSync(join(staging, path), bytes, { flag: 'wx' });
      manifest.files.push({ path, kind: path.endsWith('.csv') ? 'report_csv' : 'export_readme',
        bytes: bytes.length, sha256: sha256(bytes) });
    }
    manifest.scope = 'Partial portable copy of the indexed public artifacts and report tables. Paths are relative to this directory.';
    manifest.files.sort((left, right) => left.path.localeCompare(right.path));
    writeFileSync(join(staging, 'export-manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`, { flag: 'wx' });
    if (lstatSync(output, { throwIfNoEntry: false })) throw new Error('export destination must not exist');
    renameSync(staging, output);
  } finally { rmSync(staging, { recursive: true, force: true }); }
  return output;
}
