import { criterionEvidence, evidenceDisposition } from './check-evidence.mjs';

function criteria(bundle) {
  return Object.entries(bundle?.suites ?? {}).flatMap(([suiteId, suite]) =>
    (suite?.features ?? []).flatMap(feature =>
      (feature.criteria ?? []).map(criterion => ({
        ...criterion,
        key: `${suiteId}/${feature.id ?? feature.name}/${criterion.id}`,
      }))));
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
