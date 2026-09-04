import {
  criterionEvidence,
  evidenceDisposition,
  type CheckEvidenceStatus,
} from './check-evidence.js';

type FailureStage = 'setup' | 'assertion';
type NullCriterionStatus = 'expected_fail' | 'vacuous_pass' | 'oracle_gap';

interface NullReportCriterion {
  id?: string;
  points?: unknown;
  evidence?: unknown;
}

interface NullReportFeature {
  id: string | number;
  name?: string | null;
  criteria?: readonly NullReportCriterion[] | null;
}

export interface NullReportSuite {
  track: string;
  level: number;
  id: string;
  scenario?: string;
  report?: {
    features?: readonly NullReportFeature[] | null;
  } | null;
}

interface NullCriterionResult {
  track: string;
  level: number;
  suite: string;
  scenario: string | undefined;
  feature: string | number;
  featureName: string | null | undefined;
  criterion: string | undefined;
  points: number;
  status: NullCriterionStatus;
  evidenceStatus: CheckEvidenceStatus;
  failureStage: FailureStage | null;
  detail: string | null;
}

interface CriteriaSummary {
  criteria: number;
  points: number;
}

interface UnscoredResult {
  status: CheckEvidenceStatus;
  passed: boolean;
  applicationFailure: boolean;
  measured: boolean;
}

export function analyseNullReports(suites: readonly NullReportSuite[]) {
  const criteria: NullCriterionResult[] = [];
  const unscored: UnscoredResult[] = [];

  for (const [suiteIndex, suite] of suites.entries()) {
    const suiteAt = `suites[${suiteIndex}]`;
    if (!suite.track || !suite.id || !Number.isSafeInteger(suite.level) || suite.level < 1) {
      throw new Error(`${suiteAt} has an invalid track, level, or id`);
    }
    for (const [featureIndex, feature] of (suite.report?.features ?? []).entries()) {
      const featureAt = `${suiteAt}.features[${featureIndex}]`;
      if ((typeof feature.id !== 'string' || !feature.id)
          && !Number.isSafeInteger(feature.id)) {
        throw new Error(`${featureAt}.id is invalid`);
      }
      for (const [criterionIndex, criterion] of (feature.criteria ?? []).entries()) {
        const points = criterion.points ?? 1;
        if (typeof points !== 'number' || !Number.isSafeInteger(points) || points < 0) {
          throw new Error(`${featureAt}.criteria[${criterionIndex}].points must be a non-negative integer`);
        }
        const evidence = criterionEvidence(criterion);
        const disposition = evidenceDisposition(evidence);
        if (points <= 0) {
          unscored.push({
            status: evidence.status,
            passed: disposition.passed,
            applicationFailure: disposition.applicationFailure,
            measured: disposition.measured,
          });
          continue;
        }
        const status: NullCriterionStatus = disposition.passed
          ? 'vacuous_pass'
          : disposition.applicationFailure ? 'expected_fail' : 'oracle_gap';
        const failureStage = status === 'expected_fail' ? evidence.phase : null;
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

  const summarizeWhere = (
    predicate: (criterion: NullCriterionResult) => boolean,
  ): CriteriaSummary => {
    const matches = criteria.filter(predicate);
    return {
      criteria: matches.length,
      points: matches.reduce((total, item) => total + item.points, 0),
    };
  };
  const summarize = (status: NullCriterionStatus): CriteriaSummary =>
    summarizeWhere(item => item.status === status);

  const summary = {
    criteria: criteria.length,
    points: criteria.reduce((total, item) => total + item.points, 0),
    expectedFailures: summarize('expected_fail'),
    expectedFailureStages: {
      setup: summarizeWhere(
        item => item.status === 'expected_fail' && item.failureStage === 'setup',
      ),
      assertion: summarizeWhere(
        item => item.status === 'expected_fail' && item.failureStage === 'assertion',
      ),
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
}
