import type { CampaignRunLevelResult, CampaignRunResult, DependencyProgress }
  from '../../src/campaigns/campaign-inspection.js';

// The dashboard's vocabulary in one place: Unaided, Score, Repairs, Regressions,
// Stalling and Excluded are defined here and nowhere else, so the server-rendered
// sheet and the browser read the same numbers from the same evidence.

export const STACK_ORDER = ['spacetime', 'postgres', 'mongodb'];
const EXCLUDED_OUTCOMES = new Set(['harness_failure', 'inconclusive', 'ungraded', 'contaminated']);
const STALL_GRADES = 3;
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
  execution: MetricExecution | null;
  result: CampaignRunResult | null;
  dependency: DependencyProgress | null;
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

// Live attempts record cost by level before final totals exist.
export function attemptSpend(attempt: MetricAttempt): number | null {
  const run = attempt.result;
  if (!run || run.unreadable) return null;
  const levelled = (run.levels ?? []).reduce<number | null>((total, level) =>
    level.costUsd == null ? total : (total ?? 0) + level.costUsd, null);
  const spend = run.costUsd ?? levelled;
  return spend != null && Number.isFinite(spend) && spend >= 0 ? spend : null;
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
      spend: run.costComplete === true ? attemptSpend(attempt) : null,
      duration: run.durationSec ?? null,
      scope: `dependency:${dependency.nodes.length}:${available}`,
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
    spend: run.costComplete === true ? attemptSpend(attempt) : null,
    duration: run.durationSec ?? null,
    scope: `sequential:${levels.map(level => level.level).join(',')}`,
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
  for (const attempt of campaign.attempts ?? []) {
    const entry = byStack.get(attempt.stack)
      ?? { stack: attempt.stack, runs: [], excluded: [], pending: 0, spendSoFar: null, abortedFirst: 0 };
    byStack.set(attempt.stack, entry);
    // Excluded attempts still contribute to actual spend.
    const incurred = attemptSpend(attempt);
    if (incurred != null) entry.spendSoFar = (entry.spendSoFar ?? 0) + incurred;
    const reason = attemptExcluded(attempt);
    if (reason) { entry.excluded.push({ attempt, reason }); continue; }
    const metrics = attempt.status === 'completed' ? attemptMetrics(attempt) : null;
    if (metrics) {
      entry.runs.push({ attempt, metrics });
      entry.abortedFirst += metrics.abortedFirst;
    } else entry.pending += 1;
  }
  const rows = [...byStack.values()]
    .sort((left, right) => STACK_ORDER.indexOf(left.stack) - STACK_ORDER.indexOf(right.stack))
    .map(entry => {
      const pick = (key: 'first' | 'final' | 'repairs' | 'spend' | 'duration'): number[] =>
        entry.runs.map(run => run.metrics[key]).filter((value): value is number => value !== null);
      const range = (values: readonly number[]): { min: number; max: number } | null =>
        values.length ? { min: Math.min(...values), max: Math.max(...values) } : null;
      const spend = pick('spend');
      const duration = pick('duration');
      const first = pick('first');
      const scopes = [...new Set(entry.runs.map(run => run.metrics.scope))].sort();
      return { ...entry, n: entry.runs.length, scopes,
        first: median(first), firstRange: range(first),
        final: median(pick('final')),
        repairs: median(pick('repairs')),
        spend: median(spend), spendRange: range(spend),
        duration: median(duration), durationRange: range(duration) };
    });
  const usable = rows.filter(row => row.n > 0);
  const scopes = new Set(usable.flatMap(row => row.scopes));
  const priced = usable.filter(row => row.spend != null);
  return { rows, usable, priced,
    burn: new Map([...byStack.values()].map(entry => [entry.stack, entry.spendSoFar])),
    mixedScope: scopes.size > 1,
    comparable: priced.length > 1 && scopes.size === 1 };
}

// A trailing run of identical grades is the repair loop treading water.
export function stallRounds(
  series: readonly { score: number; max: number }[] | null | undefined): number {
  if (!series || series.length < STALL_GRADES + 1) return 0;
  const last = series.at(-1);
  if (!last) return 0;
  let flat = 0;
  for (let index = series.length - 2; index >= 0; index--) {
    const item = series[index];
    if (item && item.score === last.score && item.max === last.max) flat += 1;
    else break;
  }
  return flat >= STALL_GRADES ? flat : 0;
}

export function outputSilentMinutes(attempt: MetricAttempt, now = Date.now()): number {
  if (attempt.status !== 'running' || !attempt.logUpdatedAt) return 0;
  return Math.floor((now - Date.parse(attempt.logUpdatedAt)) / 60000);
}

// Stalling: three identical consecutive grades, or ten minutes of silence.
export function attemptStalling(attempt: MetricAttempt,
  series: readonly { score: number; max: number }[] | null | undefined,
  now = Date.now()): boolean {
  if (attempt.status !== 'running') return false;
  return stallRounds(series) > 0 || outputSilentMinutes(attempt, now) >= SILENCE_MINUTES;
}
