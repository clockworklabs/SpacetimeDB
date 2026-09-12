import type { CheckEvidence, CheckEvidencePhase, CheckEvidenceStatus }
  from './check-evidence.js';

export interface GradeCleanupFailure {
  actor: string | null;
  stage: string;
  reason: string;
}

export interface GradeCriterionResult {
  id: string;
  desc: string;
  points: number;
  evidence: CheckEvidence;
  stableKey?: string;
  serverCheck?: string;
}

export interface CompletedGradeFeatureResult {
  id: number;
  name: string;
  score: number;
  max: number;
  criteria: GradeCriterionResult[];
  consoleErrors: string[];
  setupEvidence: CheckEvidence;
  cleanupEvidence?: { status: 'harness_failure'; failures: GradeCleanupFailure[] };
  inconclusive?: Array<{
    id: string;
    points: number;
    status: CheckEvidenceStatus;
    code: string;
    phase: CheckEvidencePhase;
    summary: string | null;
  }>;
  screenshots?: string[];
  videos?: string[];
  unverified?: string[];
  verified?: string[];
}

export interface GradeSelectionCheck {
  stableKey: string;
  packId: string;
}

export interface CompletedGradeReport {
  selection: { checks: GradeSelectionCheck[] } | null;
  features: CompletedGradeFeatureResult[];
  total: number;
  max: number;
}
