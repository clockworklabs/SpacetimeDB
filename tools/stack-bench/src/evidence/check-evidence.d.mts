export type CheckEvidenceStatus = 'passed' | 'failed' | 'inconclusive' | 'harness_failure';

export type CheckOutcomeKind = 'passed' | 'app_failure' | 'inconclusive' | 'harness_failure';

export interface CheckEvidenceTiming {
  startedAtMs: number;
  completedAtMs: number;
  durationMs: number;
}

export interface CheckEvidenceDisposition {
  status: CheckEvidenceStatus;
  label: string;
  outcomeKind: CheckOutcomeKind;
  passed: boolean;
  measured: boolean;
  applicationFailure: boolean;
  repairable: boolean;
}

export interface CheckEvidence {
  schemaVersion: 1;
  status: CheckEvidenceStatus;
  code: string;
  phase: 'setup' | 'assertion';
  actor: string | null;
  summary: string | null;
  observation: unknown;
  expected: unknown;
  retryable: boolean;
  timing: CheckEvidenceTiming;
  actions: unknown[];
  attachments: unknown[];
  sensitivity: string[];
  [key: string]: unknown;
}

export const CHECK_EVIDENCE_DISPOSITIONS: Readonly<Record<CheckEvidenceStatus,
  Readonly<CheckEvidenceDisposition>>>;

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
export function validateCheckEvidence(value: unknown, options?: { at?: string }): CheckEvidence;
export function evidenceDisposition(value: CheckEvidence | CheckEvidenceStatus): CheckEvidenceDisposition;
export function evidenceIsMeasured(evidence: CheckEvidence): boolean;
export function evidencePassed(evidence: CheckEvidence): boolean;
export function evidenceIsApplicationFailure(evidence: CheckEvidence): boolean;
export function evidenceIsRepairable(evidence: CheckEvidence): boolean;
