import { criterionEvidence, evidenceDisposition } from './check-evidence.mjs';

function criteria(bundle) {
  return Object.entries(bundle?.suites ?? {}).flatMap(([suiteId, suite]) =>
    (suite?.features ?? []).flatMap(feature =>
      (feature.criteria ?? []).map(criterion => ({
        ...criterion,
        key: `${suiteId}/${feature.id ?? feature.name}/${criterion.id}`,
      }))));
}

function selectedScoreMismatch(bundle, all) {
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
    if (!Number.isSafeInteger(planned?.points) || planned.points < 0
        || criterion.points !== planned.points) {
      return `graded points disagree with the selected definition for ${criterion.stableKey}`;
    }
    if (criterion.points <= 0) continue;
    evidenceMax += criterion.points;
    if (evidenceDisposition(criterionEvidence(criterion)).passed) evidenceScore += criterion.points;
  }

  const regression = bundle.totals?.regression;
  const totalScore = bundle.totals?.score;
  const totalMax = bundle.totals?.max;
  const reportedScore = totalScore + (regression?.score ?? 0);
  const reportedMax = totalMax + (regression?.max ?? 0);
  if (![totalScore, totalMax, reportedScore, reportedMax].every(Number.isSafeInteger)
      || reportedScore !== evidenceScore || reportedMax !== evidenceMax) {
    return `reported score ${String(reportedScore)}/${String(reportedMax)} disagrees with `
      + `check evidence ${evidenceScore}/${evidenceMax}`;
  }
  return null;
}

export function classifyBundle(bundle) {
  if (!bundle) return { kind: 'ungraded', phase: 'grading', reason: 'no grading bundle was produced',
    appFailures: [], inconclusive: [], harnessFailures: [] };
  if (bundle.outcome?.kind === 'harness_failure') {
    return { ...bundle.outcome, appFailures: [], inconclusive: [], harnessFailures: [] };
  }
  if (bundle.outcome?.kind === 'app_failure') {
    return { ...bundle.outcome, appFailures: bundle.outcome.appFailures ?? [], inconclusive: [],
      harnessFailures: [] };
  }
  const selectedScopeComplete = Array.isArray(bundle.selection?.checks)
    && bundle.selection.checks.length > 0
    && bundle.selection.notRun?.length === 0
    && bundle.selection.reportedChecks?.length === bundle.selection.checks.length;
  if (!(bundle.totals?.max > 0) && !selectedScopeComplete) {
    return { kind: 'ungraded', phase: 'grading', reason: bundle.error ?? 'bundle has no scored denominator',
      appFailures: [], inconclusive: [], harnessFailures: [] };
  }
  const all = criteria(bundle);
  const scoreMismatch = selectedScoreMismatch(bundle, all);
  if (scoreMismatch) {
    return { kind: 'harness_failure', phase: 'grading', reason: scoreMismatch,
      appFailures: [], inconclusive: [], harnessFailures: ['score-consistency'] };
  }
  // Point-bearing checks define an ordinary benchmark outcome. Zero-point
  // criteria are retained as test-development evidence, but they must not
  // fail, invalidate, or repair a scored run. A deliberately selected
  // zero-only scope still receives a useful outcome for qualification work.
  const pointBearing = all.filter(criterion => Number(criterion.points) > 0);
  const outcomeCriteria = pointBearing.length ? pointBearing : all;
  const classified = outcomeCriteria.map(criterion => {
    const evidence = criterionEvidence(criterion);
    return { disposition: evidenceDisposition(evidence), key: criterion.key };
  });
  const keysFor = kind => classified.filter(item => item.disposition.outcomeKind === kind)
    .map(item => item.key);
  const harnessFailures = keysFor('harness_failure');
  const inconclusive = keysFor('inconclusive');
  const appFailures = keysFor('app_failure');
  if (bundle.suites?.lint?.pass === false) appFailures.unshift('contract-lint');
  const kind = harnessFailures.length ? 'harness_failure'
    : appFailures.length ? 'app_failure' : inconclusive.length ? 'inconclusive' : 'passed';
  return { kind, phase: 'grading', reason: null, appFailures, inconclusive, harnessFailures };
}

export function aggregateRunOutcome(levels) {
  const priority = ['harness_failure', 'ungraded', 'app_failure', 'inconclusive', 'passed'];
  const kinds = levels.map(level => level.outcome?.kind ?? 'ungraded');
  return {
    kind: priority.find(kind => kinds.includes(kind)) ?? 'ungraded',
    levels: Object.fromEntries(levels.map(level => [String(level.level), level.outcome ?? {
      kind: 'ungraded', reason: 'level has no structured outcome',
    }])),
  };
}

export function runExitCode(outcome) {
  return ['harness_failure', 'ungraded'].includes(outcome?.kind) ? 1 : 0;
}

// A ladder level builds on the source produced by the previous level. If that
// level was not graded, proceeding would spend another model session on an
// artifact whose baseline is unknown and produce a run that cannot be compared.
export function ladderMayContinue(outcome) {
  return !['harness_failure', 'ungraded'].includes(outcome?.kind);
}

// Building and repairing a level can continue while its failures are ordinary
// application failures. Advancing to the next level is stricter: the next
// upgrade must start from a level that actually passed, otherwise later work
// hides unresolved lower-level defects and has to be thrown away when that
// lower level is repaired.
export function ladderMayAdvance(outcome) {
  return outcome?.kind === 'passed';
}

export function mutationControlEligible(outcome) {
  return outcome?.kind === 'passed';
}
