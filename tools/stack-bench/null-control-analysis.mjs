import { criterionEvidence, evidenceDisposition } from './check-evidence.mjs';

export function analyseNullReports(suites) {
  const criteria = [];
  const unscored = [];
  for (const suite of suites) {
    for (const feature of suite.report?.features ?? []) {
      for (const criterion of feature.criteria ?? []) {
        const points = Number(criterion.points ?? 1);
        const evidence = criterionEvidence(criterion);
        const disposition = evidenceDisposition(evidence);
        if (points <= 0) {
          unscored.push({ status: evidence.status, passed: disposition.passed,
            applicationFailure: disposition.applicationFailure, measured: disposition.measured });
          continue;
        }
        const status = disposition.passed
          ? 'vacuous_pass'
          : disposition.applicationFailure ? 'expected_fail' : 'oracle_gap';
        const failureStage = status === 'expected_fail'
          ? evidence.phase
          : null;
        criteria.push({
          track: suite.track,
          level: suite.level,
          suite: suite.id,
          scenario: suite.scenario,
          feature: feature.id,
          featureName: feature.name,
          criterion: criterion.id,
          points,
          status,
          evidenceStatus: evidence.status,
          failureStage,
          detail: evidence.summary,
        });
      }
    }
  }

  const count = status => criteria.filter(item => item.status === status);
  const summarize = status => {
    const matches = count(status);
    return { criteria: matches.length, points: matches.reduce((total, item) => total + item.points, 0) };
  };
  const summary = {
    criteria: criteria.length,
    points: criteria.reduce((total, item) => total + item.points, 0),
    expectedFailures: summarize('expected_fail'),
    expectedFailureStages: {
      setup: summarizeWhere(item => item.status === 'expected_fail' && item.failureStage === 'setup'),
      assertion: summarizeWhere(item => item.status === 'expected_fail' && item.failureStage === 'assertion'),
    },
    vacuousPasses: summarize('vacuous_pass'),
    oracleGaps: summarize('oracle_gap'),
    unscored: {
      criteria: unscored.length,
      passed: unscored.filter(item => item.passed).length,
      failed: unscored.filter(item => item.applicationFailure).length,
      inconclusive: unscored.filter(item => !item.measured).length,
    },
  };
  return {
    ok: summary.vacuousPasses.criteria === 0 && summary.oracleGaps.criteria === 0,
    summary,
    criteria,
  };

  function summarizeWhere(predicate) {
    const matches = criteria.filter(predicate);
    return { criteria: matches.length, points: matches.reduce((total, item) => total + item.points, 0) };
  }
}
