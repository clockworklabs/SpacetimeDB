import type { CampaignRunLevelResult, CampaignRunResult, DependencyProgress }
  from '../../src/campaigns/campaign-inspection.js';
import type { CostEvidence } from '../../src/evidence/cost-proof.js';
import type { CheckCompletion } from '../../src/evidence/check-completion.js';

// The dashboard's vocabulary in one place: Unaided, Score, Repairs, Regressions,
// Stalling and Excluded are defined here and nowhere else, so the server-rendered
// sheet and the browser read the same numbers from the same evidence.

const EXCLUDED_OUTCOMES = new Set(['harness_failure', 'inconclusive', 'ungraded', 'contaminated']);
const SILENCE_MINUTES = 10;

export interface MetricExecution {
  outcome: string | null;
  reason: string | null;
}

export interface MetricAttempt {
  id: string;
  stack: string;
  status: string;
  repetition?: number;
  logUpdatedAt?: string | null;
  activityUpdatedAt?: string | null;
  paused?: boolean;
  execution: MetricExecution | null;
  result: CampaignRunResult | null;
  dependency: DependencyProgress | null;
  spend?: CostEvidence;
  completion?: CheckCompletion | null;
  comparisonKey?: string;
}

export interface AttemptMetrics {
  first: number | null;
  final: number;
  repairs: number;
  spend: number | null;
  duration: number | null;
  scope: string;
  abortedFirst: number;
  raw: {
    first: { score: number; max: number } | null;
    final: { score: number; max: number } | null;
  };
}

export interface ComparisonEntry<Attempt extends MetricAttempt> {
  stack: string;
  runs: Array<{ attempt: Attempt; metrics: AttemptMetrics }>;
  excluded: Array<{ attempt: Attempt; reason: string }>;
  pending: number;
  spendSoFar: number | null;
  abortedFirst: number;
}

export type ComparisonRow<Attempt extends MetricAttempt> = ComparisonEntry<Attempt> & {
  n: number; scopes: string[]; first: number | null; final: number | null;
  repairs: number | null; spend: number | null; duration: number | null;
  costPerValidRun: number | null;
  firstRange: { min: number; max: number } | null;
  spendRange: { min: number; max: number } | null;
  durationRange: { min: number; max: number } | null;
};

export function median(values: readonly number[]): number | null {
  if (!values.length) return null;
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle]! : (sorted[middle - 1]! + sorted[middle]!) / 2;
}

export function attemptSpend(attempt: MetricAttempt): number | null {
  return attempt.spend?.status === 'exact' ? attempt.spend.costUsd : null;
}

// An ungraded first build has no score; it is not a zero.
export function attemptMetrics(attempt: MetricAttempt): AttemptMetrics | null {
  const run = attempt.result;
  if (!run || run.unreadable) return null;
  const dependency = attempt.dependency;
  if (dependency) {
    const score = dependency.score;
    const unique = score?.uniqueChecks;
    if (score?.status !== 'final' || unique?.percentage == null) return null;
    const available = unique.availablePoints ?? 0;
    return {
      first: dependency.history ? dependency.history.firstTryPercentage / 100 : null,
      // Passed points over every selected point in the graph, the same scale
      // as the first build. The questline average is the sheet's secondary view.
      final: unique.percentage / 100,
      repairs: dependency.history?.repairAttempts ?? 0,
      spend: attemptSpend(attempt),
      duration: run.durationSec ?? null,
      scope: `${attempt.comparisonKey ?? ''}:dependency:${dependency.nodes.length}:${available}`,
      abortedFirst: 0,
      raw: { first: null, final: unique.passedPoints == null
        ? null : { score: unique.passedPoints, max: available } },
    };
  }
  type FinalLevel = CampaignRunLevelResult & { finalScore: { score: number; max: number } };
  type ScoredLevel = FinalLevel & { firstScore: { score: number; max: number } };
  const levels = (run.levels ?? [])
    .filter((level): level is FinalLevel => level.finalScore !== null);
  if (!levels.length) return null;
  const sum = <Level extends CampaignRunLevelResult>(list: readonly Level[],
    pick: (level: Level) => number): number => list.reduce((total, item) => total + pick(item), 0);
  const scored = levels.filter((level): level is ScoredLevel =>
    level.firstScore !== null && level.firstAbort === null);
  const abortedFirst = levels.filter(level => level.firstAbort).length;
  const firstMax = sum(scored, level => level.firstScore.max);
  const finalMax = sum(levels, level => level.finalScore.max);
  return {
    first: firstMax ? sum(scored, level => level.firstScore.score) / firstMax : null,
    final: sum(levels, level => level.finalScore.score) / finalMax,
    repairs: sum(levels, level => level.used ?? 0),
    spend: attemptSpend(attempt),
    duration: run.durationSec ?? null,
    scope: `${attempt.comparisonKey ?? ''}:sequential:${levels.map(level => level.level).join(',')}`,
    abortedFirst,
    // Raw sums over the same set of levels, so a first and a final score shown
    // side by side are always out of the same total.
    raw: { first: firstMax ? { score: sum(scored, l => l.firstScore.score), max: firstMax } : null,
      final: { score: sum(levels, l => l.finalScore.score), max: finalMax } },
  };
}

export function attemptExcluded(attempt: MetricAttempt): string | null {
  const outcome = attempt.execution?.outcome ?? attempt.result?.outcome;
  if (attempt.status === 'invalid') return attempt.execution?.reason ?? outcome ?? 'excluded';
  if (attempt.result?.unreadable && attempt.status !== 'running') return 'result could not be read';
  // 'ungraded' on an attempt still running means "not yet", not "thrown out".
  if (outcome && EXCLUDED_OUTCOMES.has(outcome) && attempt.status === 'completed') return outcome;
  return null;
}

// Compare results only when they share the same recorded test plan.
export function compareCampaign<Attempt extends MetricAttempt>(campaign: {
  attempts?: readonly Attempt[];
}): { rows: Array<ComparisonRow<Attempt>>; usable: Array<ComparisonRow<Attempt>>;
  priced: Array<ComparisonRow<Attempt>>; burn: Map<string, number | null>;
  mixedScope: boolean; comparable: boolean } {
  const byStack = new Map<string, ComparisonEntry<Attempt>>();
  const unknownSpend = new Set<string>();
  for (const attempt of campaign.attempts ?? []) {
    const entry = byStack.get(attempt.stack)
      ?? { stack: attempt.stack, runs: [], excluded: [], pending: 0, spendSoFar: null, abortedFirst: 0 };
    byStack.set(attempt.stack, entry);
    // Excluded attempts still contribute to actual spend.
    const incurred = attemptSpend(attempt);
    if (incurred === null) unknownSpend.add(attempt.stack);
    else entry.spendSoFar = (entry.spendSoFar ?? 0) + incurred;
    const reason = attemptExcluded(attempt);
    if (reason) { entry.excluded.push({ attempt, reason }); continue; }
    const metrics = attempt.status === 'completed' ? attemptMetrics(attempt) : null;
    if (metrics) {
      entry.runs.push({ attempt, metrics });
      entry.abortedFirst += metrics.abortedFirst;
    } else entry.pending += 1;
  }
  const rows = [...byStack.values()]
    .map(entry => {
      const pick = (key: 'first' | 'final' | 'repairs' | 'spend' | 'duration'): number[] =>
        entry.runs.map(run => run.metrics[key]).filter((value): value is number => value !== null);
      const range = (values: readonly number[]): { min: number; max: number } | null =>
        values.length ? { min: Math.min(...values), max: Math.max(...values) } : null;
      const spend = pick('spend');
      const duration = pick('duration');
      const first = pick('first');
      const scopes = [...new Set(entry.runs.map(run => run.metrics.scope))].sort();
      return { ...entry, spendSoFar: unknownSpend.has(entry.stack) ? null : entry.spendSoFar,
        n: entry.runs.length, scopes,
        first: scopes.length === 1 ? median(first) : null, firstRange: range(first),
        final: scopes.length === 1 ? median(pick('final')) : null,
        repairs: scopes.length === 1 ? median(pick('repairs')) : null,
        spend: scopes.length === 1 ? median(spend) : null, spendRange: scopes.length === 1 ? range(spend) : null,
        costPerValidRun: scopes.length === 1 && spend.length > 0 && spend.length === entry.runs.length
          ? spend.reduce((total, cost) => total + cost, 0) / spend.length : null,
        duration: scopes.length === 1 ? median(duration) : null,
        durationRange: scopes.length === 1 ? range(duration) : null };
    });
  const usable = rows.filter(row => row.n > 0);
  const scopes = new Set(usable.flatMap(row => row.scopes));
  const priced = usable.filter(row => row.spend != null);
  return { rows, usable, priced,
    burn: new Map(rows.map(entry => [entry.stack, entry.spendSoFar])),
    mixedScope: scopes.size > 1,
    comparable: priced.length > 1 && scopes.size === 1 };
}

export function outputSilentMinutes(attempt: Pick<MetricAttempt,
  'status' | 'paused' | 'activityUpdatedAt'>, now = Date.now()): number {
  if (attempt.status !== 'running' || attempt.paused || !attempt.activityUpdatedAt) return 0;
  const updated = Date.parse(attempt.activityUpdatedAt);
  return Number.isFinite(updated) ? Math.max(0, Math.floor((now - updated) / 60000)) : 0;
}

// Flag observed agent inactivity, never infer it from controller output or scores.
export function attemptStalling(attempt: Pick<MetricAttempt, 'status' | 'paused' | 'activityUpdatedAt'>,
  now = Date.now()): boolean {
  return outputSilentMinutes(attempt, now) >= SILENCE_MINUTES;
}
