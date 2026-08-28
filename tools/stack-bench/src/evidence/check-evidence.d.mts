export type CheckEvidenceStatus = 'passed' | 'failed' | 'inconclusive' | 'harness_failure';

export interface CheckEvidence {
  status: CheckEvidenceStatus;
  summary: string | null;
  [key: string]: unknown;
}

export function createCheckEvidence(input: {
  status: CheckEvidenceStatus;
  code: string;
  phase: 'setup' | 'assertion';
  actor?: string | null;
  summary?: string | null;
  observation?: unknown;
  expected?: unknown;
  retryable?: boolean;
  startedAtMs: number;
  completedAtMs: number;
  actions?: unknown[];
  attachments?: unknown[];
  sensitivity?: string[];
}): CheckEvidence;
export function criterionEvidence(criterion: { id?: string; evidence?: unknown }): CheckEvidence;
export function evidenceIsRepairable(evidence: CheckEvidence): boolean;
