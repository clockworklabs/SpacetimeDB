import { criterionEvidence, evidenceDisposition } from './check-evidence.js';

interface CleanupEvidence {
  readonly status?: string;
}

export interface OutcomeCriterion {
  readonly evidence?: unknown;
  readonly id?: string;
  readonly name?: string;
  readonly points?: number;
  readonly stableKey?: string;
}

export interface OutcomeFeature {
  readonly cleanupEvidence?: CleanupEvidence;
  readonly criteria?: readonly OutcomeCriterion[];
  readonly id?: string;
  readonly name?: string;
}

interface OutcomeSuite {
  readonly cleanupEvidence?: CleanupEvidence;
  readonly features?: readonly OutcomeFeature[];
  readonly pass?: boolean;
}

interface SelectedCheck {
  readonly points: number;
  readonly stableKey: string;
}

interface OutcomeSelection {
  readonly checks?: readonly SelectedCheck[];
  readonly notRun?: readonly unknown[];
  readonly reportedChecks?: readonly string[];
}

interface ScoreTotals {
  readonly max?: number | null;
  readonly regression?: { readonly max?: number | null; readonly score?: number | null } | null;
  readonly score?: number | null;
}

export interface RunOutcome {
  readonly kind: string;
  readonly phase?: string;
  readonly reason?: string | null;
  readonly appFailures?: readonly string[];
  readonly inconclusive?: readonly string[];
  readonly harnessFailures?: readonly string[];
  readonly provider?: unknown;
}

export interface OutcomeBundle {
  readonly error?: string;
  readonly outcome?: RunOutcome;
  readonly selection?: OutcomeSelection | null;
  readonly suites?: Readonly<Record<string, OutcomeSuite | null>>;
  readonly totals?: ScoreTotals;
}

interface KeyedCriterion extends OutcomeCriterion { readonly key: string }

interface ClassifiedOutcome extends RunOutcome {
  readonly appFailures: readonly string[];
  readonly harnessFailures: readonly string[];
  readonly inconclusive: readonly string[];
}

interface LevelResult {
  readonly level: number | string;
  readonly outcome?: RunOutcome;
}

interface AggregateOutcome extends RunOutcome {
  readonly levels: Readonly<Record<string, RunOutcome>>;
}

function criteria(bundle: OutcomeBundle): KeyedCriterion[] {
  return Object.entries(bundle?.suites ?? {}).flatMap(([suiteId, suite]) =>
    (suite?.features ?? []).flatMap(feature =>
      (feature.criteria ?? []).map(criterion => ({
        ...criterion,
        key: `${suiteId}/${feature.id ?? feature.name}/${criterion.id}`,
      }))));
}

function cleanupFailureKeys(bundle: OutcomeBundle): string[] {
  return Object.entries(bundle?.suites ?? {}).flatMap(([suiteId, suite]) => {
    const keys = suite?.cleanupEvidence?.status === 'harness_failure'
      ? [`${suiteId}/cleanup`] : [];
    for (const feature of suite?.features ?? []) {
      if (feature?.cleanupEvidence?.status === 'harness_failure') {
        keys.push(`${suiteId}/${feature.id ?? feature.name}/cleanup`);
      }
    }
    return keys;
  });
}

function selectedScoreMismatch(bundle: OutcomeBundle, all: readonly KeyedCriterion[]): string | null {
  const selection = bundle?.selection;
  if (!Array.isArray(selection?.checks) || !Array.isArray(selection?.reportedChecks)
      || selection.notRun?.length !== 0
      || selection.reportedChecks.length !== selection.checks.length) return null;

  const selected = new Map(selection.checks.map(check => [check.stableKey, check]));
  if (selected.size !== selection.checks.length
      || selection.reportedChecks.some(key => !selected.has(key))
      || new Set(selection.reportedChecks).size !== selection.reportedChecks.length) {
    return 'reported check scope disagrees with the selected check scope';
  }

  const reported = all.filter(criterion => criterion.stableKey !== undefined);
  if (reported.length !== selected.size
      || new Set(reported.map(criterion => criterion.stableKey)).size !== reported.length
      || reported.some(criterion => !selected.has(criterion.stableKey))) {
    return 'graded check evidence disagrees with the selected check scope';
  }

  let evidenceScore = 0;
  let evidenceMax = 0;
  for (const criterion of reported) {
    const planned = selected.get(criterion.stableKey);
    const points = criterion.points;
    if (!Number.isSafeInteger(planned?.points) || planned.points < 0
        || typeof points !== 'number' || points !== planned.points) {
      return `graded points disagree with the selected definition for ${criterion.stableKey}`;
    }
    if (points <= 0) continue;
    evidenceMax += points;
    if (evidenceDisposition(criterionEvidence(criterion)).passed) evidenceScore += points;
  }

  const regression = bundle.totals?.regression;
  const totalScore = bundle.totals?.score;
  const totalMax = bundle.totals?.max;
  const reportedScore = Number(totalScore) + Number(regression?.score ?? 0);
  const reportedMax = Number(totalMax) + Number(regression?.max ?? 0);
  if (![totalScore, totalMax, reportedScore, reportedMax].every(Number.isSafeInteger)
      || reportedScore !== evidenceScore || reportedMax !== evidenceMax) {
    return `reported score ${String(reportedScore)}/${String(reportedMax)} disagrees with `
      + `check evidence ${evidenceScore}/${evidenceMax}`;
  }
  return null;
}

export function classifyBundle(bundle: OutcomeBundle | null | undefined): ClassifiedOutcome {
  if (!bundle) return { kind: 'ungraded', phase: 'grading', reason: 'no grading bundle was produced',
    appFailures: [], inconclusive: [], harnessFailures: [] };
  const cleanupFailures = cleanupFailureKeys(bundle);
  if (cleanupFailures.length) {
    return { kind: 'harness_failure', phase: 'grading-cleanup',
      reason: 'grader cleanup did not complete',
      appFailures: bundle.outcome?.appFailures ?? [], inconclusive: [],
      harnessFailures: cleanupFailures };
  }
  const declaredOutcome = bundle.outcome;
  if (declaredOutcome && ['provider_failure', 'harness_failure'].includes(declaredOutcome.kind)) {
    return { ...declaredOutcome, appFailures: [], inconclusive: [], harnessFailures: [] };
  }
  if (bundle.outcome?.kind === 'app_failure') {
    return { ...bundle.outcome, appFailures: bundle.outcome.appFailures ?? [], inconclusive: [],
      harnessFailures: [] };
  }
  const selectedScopeComplete = Array.isArray(bundle.selection?.checks)
    && bundle.selection.checks.length > 0
    && bundle.selection.notRun?.length === 0
    && bundle.selection.reportedChecks?.length === bundle.selection.checks.length;
  if (!((bundle.totals?.max ?? 0) > 0) && !selectedScopeComplete) {
    return { kind: 'ungraded', phase: 'grading', reason: bundle.error ?? 'bundle has no scored denominator',
      appFailures: [], inconclusive: [], harnessFailures: [] };
  }
  const all = criteria(bundle);
  const scoreMismatch = selectedScoreMismatch(bundle, all);
  if (scoreMismatch) {
    return { kind: 'harness_failure', phase: 'grading', reason: scoreMismatch,
      appFailures: [], inconclusive: [], harnessFailures: ['score-consistency'] };
  }
  // Zero-point criteria affect an outcome only in a zero-point-only scope.
  const pointBearing = all.filter(criterion => Number(criterion.points) > 0);
  const outcomeCriteria = pointBearing.length ? pointBearing : all;
  const classified = outcomeCriteria.map(criterion => {
    const evidence = criterionEvidence(criterion);
    return { disposition: evidenceDisposition(evidence), key: criterion.key };
  });
  const keysFor = (kind: string): string[] =>
    classified.filter(item => item.disposition.outcomeKind === kind)
      .map(item => item.key);
  const harnessFailures = keysFor('harness_failure');
  const inconclusive = keysFor('inconclusive');
  const appFailures = keysFor('app_failure');
  if (bundle.suites?.lint?.pass === false) appFailures.unshift('contract-lint');
  const kind = harnessFailures.length ? 'harness_failure'
    : appFailures.length ? 'app_failure' : inconclusive.length ? 'inconclusive' : 'passed';
  return { kind, phase: 'grading', reason: null, appFailures, inconclusive, harnessFailures };
}

export function aggregateRunOutcome(levels: readonly LevelResult[]): AggregateOutcome {
  const priority = ['harness_failure', 'provider_failure', 'ungraded', 'app_failure',
    'inconclusive', 'passed'];
  const kinds = levels.map(level => level.outcome?.kind ?? 'ungraded');
  const kind = priority.find(candidate => kinds.includes(candidate)) ?? 'ungraded';
  const selected = levels.find(level => (level.outcome?.kind ?? 'ungraded') === kind)?.outcome
    ?? { kind };
  return {
    ...selected,
    kind,
    levels: Object.fromEntries(levels.map(level => [String(level.level), level.outcome ?? {
      kind: 'ungraded', reason: 'level has no structured outcome',
    }])),
  };
}

export function runExitCode(outcome: RunOutcome | null | undefined): 0 | 1 {
  return ['provider_failure', 'harness_failure', 'ungraded', 'incomplete']
    .includes(outcome?.kind ?? '') ? 1 : 0;
}

// Continue only from a measured baseline.
export function ladderMayContinue(outcome: RunOutcome | null | undefined): boolean {
  return !['provider_failure', 'harness_failure', 'ungraded'].includes(outcome?.kind ?? '');
}

// Advance only after the current level passes.
export function ladderMayAdvance(outcome: RunOutcome | null | undefined): boolean {
  return outcome?.kind === 'passed';
}

export function mutationControlEligible(outcome: RunOutcome | null | undefined): boolean {
  return outcome?.kind === 'passed';
}
