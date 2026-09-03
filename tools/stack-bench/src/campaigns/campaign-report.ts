import { randomUUID } from 'node:crypto';
import { mkdirSync, renameSync, writeFileSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';

import { ARTIFACT_FILE, emptyArtifactIdentities, readArtifactPayload, writeArtifact }
  from '../evidence/artifacts.js';
import { inspectCampaign } from './campaign-runner.js';
import { validateCampaignRun } from './campaign-run-validation.js';
import { canonicalDefinitionJson, canonicalizeDefinition } from '../composition/definition-plan.js';
import { sha256 } from '../evidence/provenance.js';
import { campaignGradingQualification } from './campaign-compiler.js';
import type { CampaignAttemptPlan, CampaignGradingQualification,
  CompiledCampaignPlan } from './campaign-compiler.js';
import type { CampaignExecution, CampaignState } from './campaign-scheduler.js';
import type { RunOutcome } from '../evidence/outcomes.js';
import { CAMPAIGN_FILE } from './campaign-path.js';
import type { BenchmarkRunRecord, GradeBundleSelection, RunLevelRecord, RunTotals }
  from '../evidence/benchmark-run.js';

export const CAMPAIGN_REPORT_SCHEMA_VERSION = 5;

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
'id' | 'parentAttemptId' | 'outcome'>> {
  levels?: RunLevel[];
  condition?: { requested?: { levels?: Array<{ level: number; selection?: RunSelection }> } };
  outcome?: RunOutcome;
  totals?: Partial<RunTotals>;
  progressionStatus?: {
    phase?: string;
    score?: { questlineAveragePercentage?: number | null; uniqueChecks?: {
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
  agent: { adapter: string; model: string };
  condition: { id: string; version: string; sha256: string; requested?: {
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
  [key: string]: unknown;
}

interface CampaignReportAttempt extends CampaignAttemptPlan {
  status: string;
  executions: CampaignReportExecution[];
  metrics: Record<string, number | null> | null;
  firstBuildObservations: CampaignRunObservationSummary | null;
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
    [key: string]: unknown };
  attempts: CampaignReportAttempt[];
  conditions: CampaignReportCondition[];
  summary: { campaignStatus: string; plannedAttempts: number; completedAttempts: number;
    invalidAttempts: number; invalidAttemptRate: number; pendingAttempts: number;
    runningAttempts: number; executions: number; invalidExecutions: number;
    invalidExecutionRate: number };
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
  const levels = run.levels ?? [];
  const completeFirstBuild = levels.length > 0 && levels.every(level =>
    number(level.firstBuild?.score) !== null && number(level.firstBuild?.max) !== null);
  const firstScore = completeFirstBuild
    ? levels.reduce((total, level) => total + level.firstBuild!.score!, 0) : null;
  const firstMax = completeFirstBuild
    ? levels.reduce((total, level) => total + level.firstBuild!.max!, 0) : null;
  const correctionNeeded = completeFirstBuild
    ? levels.some(level => level.firstBuild!.score! < level.firstBuild!.max!) : null;
  const completeCorrectionSpend = correctionNeeded === true
    && levels.every(level => number(level.repairCostUsd) !== null);
  const correctionSpendUsd = completeCorrectionSpend
    ? Number(levels.reduce((total, level) => total + level.repairCostUsd!, 0).toFixed(6)) : null;
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
    ? null : Math.max(0, (run.totals!.durationSec! * 1000) - throttleWaitMs);
  return {
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
    totalCostUsd: run.totals?.costComplete === true ? number(run.totals?.costUsd) : null,
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

function conditionKey(attempt: CampaignAttemptPlan): string {
  return canonicalDefinitionJson({ stack: attempt.stack, agentAdapter: attempt.agentAdapter,
    model: attempt.model, condition: attempt.condition?.sha256 }).trim();
}

function reportMetricNames(policy: CampaignReport['policy']): string[] {
  return [...new Set([policy.primaryMetric, ...policy.secondaryMetrics,
    'firstBuildCoverageRate', 'finalCoverageRate'])]
    .filter(metric => metric !== 'invalidAttemptRate');
}

function reportConditions(rows: CampaignReportAttempt[], policy: CampaignReport['policy']):
CampaignReportCondition[] {
  const groups = new Map<string, CampaignReportAttempt[]>();
  for (const row of rows) {
    const key = conditionKey(row);
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
      agent: { adapter: attempts[0]!.agentAdapter, model: attempts[0]!.model },
      condition: attempts[0]!.condition,
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
    'invalidAttempts', 'missingData', 'comparisonUnit']), 'campaign report.policy');
  exactFields(report.summary, new Set(['campaignStatus', 'plannedAttempts', 'completedAttempts',
    'invalidAttempts', 'invalidAttemptRate', 'pendingAttempts', 'runningAttempts', 'executions',
    'invalidExecutions', 'invalidExecutionRate']), 'campaign report.summary');
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
    || report.scope.parallelism > 21
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
    exactFields(row, new Set(['key', 'stack', 'agent', 'condition', 'sample', 'metrics',
      'firstBuildObservations']), at);
    if (typeof row.stack !== 'string' || !row.stack
      || !row.condition || typeof row.condition !== 'object' || Array.isArray(row.condition)
      || typeof row.condition.id !== 'string' || !row.condition.id
      || typeof row.condition.version !== 'string' || !row.condition.version
      || !/^[a-f0-9]{64}$/.test(row.condition.sha256)) {
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
    for (const [executionIndex, execution] of attempt.executions.entries()) {
      if (!isRecord(execution) || typeof execution.id !== 'string' || !execution.id
        || typeof execution.status !== 'string') {
        throw new Error(`campaign report.attempts[${index}].executions[${executionIndex}] is invalid`);
      }
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
): CampaignReport {
  if (state.campaignSha256 !== plan.contentSha256) throw new Error('report state does not match campaign plan');
  const rows: CampaignReportAttempt[] = [];
  for (const attempt of state.attempts) {
    const executions = attempt.executions.map(execution => {
      let run = null;
      if (execution.status === 'completed') {
        run = readRun(attempt.plan, execution);
        if (!run || run.outcome?.kind !== execution.outcome) {
          throw new Error(`completed execution ${execution.id} has missing or mismatched run evidence`);
        }
      }
      return {
        id: execution.id, ordinal: execution.ordinal, status: execution.status,
        outcome: execution.outcome, reason: execution.reason, exitCode: execution.exitCode,
        startedAt: execution.startedAt, completedAt: execution.completedAt,
        admissionId: execution.admissionId,
        admissionEvidence: `admissions/${execution.admissionId}.json`,
        evidence: execution.status === 'completed'
          ? `${execution.output}/${ARTIFACT_FILE.run}` : CAMPAIGN_FILE.state,
        metrics: run ? campaignRunMetrics(run) : null,
        firstBuildObservations: run ? campaignRunFirstBuildObservations(run) : null,
      };
    });
    const latest = executions.at(-1);
    rows.push({ ...attempt.plan, status: attempt.status, executions,
      metrics: latest?.status === 'completed' ? latest.metrics : null,
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
      ...(grading.status === 'pending'
        ? ['Grading qualification is pending. Treat these scores as provisional.'] : []),
      'Statistics describe only the exact scope and conditions recorded above.',
      'Score rates are passed points over all selected points; the questline average weighs every questline equally and is a secondary view.',
      'Score rates use measurable points; read them with coverage and the typed execution outcome.',
      'A spread is reported only from three or more completed attempts; below that, only the centre and the range.',
      'Usage in USD is the provider CLI token count priced at the recorded API rates. It is not an invoice.',
      'Durations exclude time spent waiting for the provider to lift a rate limit.',
      'Results describe one coding agent and the model ids recorded per session. They do not generalize to other agents.',
      'Observed specifications are diagnostic, contribute zero score, and are never shown to repairs.',
      'Invalid executions are reported separately and are not imputed into outcome metrics.',
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
    + `<td>${escape(condition.agent.adapter)} / ${escape(condition.agent.model)}</td>`
    + `<td>${escape(condition.condition.id)}@${escape(condition.condition.version)}</td>`
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
    + `${escape(formatDurationMs(condition.metrics.totalDurationMs?.center))}</small></td></tr>`).join('');
  const treatmentRows = report.scope.conditions.flatMap(condition =>
    (condition.requested?.levels ?? []).flatMap(level => {
      const specifications = level.selection?.schemaVersion === 3
        ? level.selection.specifications : null;
      if (!specifications) return [];
      const list = (values: string[]): string => values.length ? values.join(', ') : 'none';
      return [`<tr><td>${escape(`${condition.id}@${condition.version}`)}</td>`
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
      + `<td>${escape(condition.agent.adapter)} / ${escape(condition.agent.model)}</td>`
      + `<td>${escape(condition.condition.id)}@${escape(condition.condition.version)}</td>`
      + `<td>${condition.firstBuildObservations.sample.measuredAttempts}/${condition.firstBuildObservations.sample.selectedAttempts}</td>`
      + `<td>${escape(formatRate(condition.firstBuildObservations.metrics.passRate.center))}`
      + `<br><small>${escape(formatRate(condition.firstBuildObservations.metrics.coverageRate.center))} coverage</small></td></tr>`)
    .join('');
  const observationSection = observationRows ? `<h2>Additional first-build measurements</h2><p>These checks record selected behavior in the original build. They are shown separately, add no points to the score, and do not enter repair feedback.</p><table><thead><tr><th>Stack</th><th>Agent / model</th><th>Run setup</th><th>Measured</th><th>Pass rate</th></tr></thead><tbody>${observationRows}</tbody></table>` : '';
  const qualificationWarning = report.scope.grading.status === 'pending'
    ? '<div class="warn"><strong>Provisional scores.</strong> Grading qualification is pending.</div>'
    : '';
  return `<!doctype html>\n<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${escape(report.campaign.title)}</title><style>body{font:16px system-ui;max-width:1100px;margin:40px auto;padding:0 20px;color:#17202a}code{font-size:.85em}table{border-collapse:collapse;width:100%}th,td{padding:.65rem;border-bottom:1px solid #ccd;text-align:left}.meta{color:#566} .warn{background:#fff4cf;padding:1rem}</style></head><body><h1>${escape(report.campaign.title)}</h1><p class="meta">Campaign <code>${escape(report.campaign.id)}</code> · ${escape(report.campaign.sha256)} · status ${escape(report.summary.campaignStatus)}</p>${qualificationWarning}<p>This report shows exactly what ran: ${report.summary.completedAttempts} completed of ${report.summary.plannedAttempts} planned attempts, with ${report.summary.invalidExecutions} invalid execution(s) retained.</p><h2>Conditions</h2><table><thead><tr><th>Stack</th><th>Agent / model</th><th>Study condition</th><th>Completed</th><th>Invalid executions</th><th>${escape(primaryLabel)}</th></tr></thead><tbody>${rows}</tbody></table>${treatmentSection}${observationSection}<h2>Scope</h2><pre>${escape(JSON.stringify(report.scope, null, 2))}</pre><h2>Attempts and raw evidence</h2><ul>${report.attempts.map(attempt => `<li><strong>${escape(attempt.id)}</strong> — ${escape(attempt.status)}${attempt.executions.map(execution => ` · <a href="${escape(`${evidencePrefix}/${execution.evidence}`)}">${escape(execution.id)}</a> (${escape(execution.outcome ?? execution.status)}) · <a href="${escape(`${evidencePrefix}/${execution.admissionEvidence}`)}">admission</a>${(execution.firstBuildObservations?.levels ?? []).filter(level => level.artifact).map(level => ` · <a href="${escape(`${evidencePrefix}/${execution.evidence.slice(0, -ARTIFACT_FILE.run.length)}${level.artifact}`)}">L${escape(level.level)} observations</a>`).join('')}`).join('')}</li>`).join('')}</ul><div class="warn"><strong>Limitations</strong><ul>${report.limitations.map(item => `<li>${escape(item)}</li>`).join('')}</ul></div><p class="meta">Report identity: <code>${escape(report.contentSha256)}</code></p></body></html>\n`;
}

export interface GeneratedCampaignReport { report: CampaignReport; reportPath: string;
  htmlPath: string; relativeOutput: string }

export function generateCampaignReport(directory: string,
  { output: requestedOutput }: { output?: string } = {},
): GeneratedCampaignReport {
  let output = requestedOutput ?? join(resolve(directory), 'report');
  const { plan, state, paths } = inspectCampaign(directory, { requireCurrentInputs: false });
  if (state.status === 'running') throw new Error('cannot report while a campaign attempt is running');
  output = resolve(output);
  const outputRelative = relative(paths.root, output);
  if (outputRelative === '' || outputRelative === '..' || outputRelative.startsWith(`..${sep}`)) {
    throw new Error('report output must be a child of the campaign directory');
  }
  const report = buildCampaignReport(plan, state, (attempt, execution) =>
    validateCampaignRun(plan, attempt,
      readArtifactPayload(join(paths.root, execution.output, ARTIFACT_FILE.run),
        { expectedKind: 'benchmark_run' }),
      { resultDir: join(paths.root, execution.output) }) as BenchmarkRun);
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
  return { report, reportPath, htmlPath,
    relativeOutput: outputRelative.replaceAll('\\', '/') };
}
