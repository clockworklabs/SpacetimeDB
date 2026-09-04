import { findingSchema } from '../actions/action-findings.js';
import type { Finding } from '../actions/action-findings.js';
import { z } from 'zod';
import { formatZodError } from '../zod-error.js';

export const CHECK_EVIDENCE_SCHEMA_VERSION = 2;

export const CHECK_EVIDENCE_STATUSES = Object.freeze([
  'passed',
  'failed',
  'inconclusive',
  'harness_failure',
] as const);

export const CHECK_EVIDENCE_PHASES = Object.freeze(['setup', 'assertion'] as const);

export type CheckEvidenceStatus = typeof CHECK_EVIDENCE_STATUSES[number];
export type CheckEvidencePhase = typeof CHECK_EVIDENCE_PHASES[number];
export type CheckOutcomeKind =
  | 'passed'
  | 'app_failure'
  | 'inconclusive'
  | 'harness_failure';

export interface CheckEvidenceDisposition {
  status: CheckEvidenceStatus;
  label: string;
  outcomeKind: CheckOutcomeKind;
  passed: boolean;
  measured: boolean;
  applicationFailure: boolean;
  repairable: boolean;
}

export interface CheckEvidenceTiming {
  startedAtMs: number;
  completedAtMs: number;
  durationMs: number;
}

export interface CheckEvidenceActionEntry {
  actor: string | null;
  evidence: unknown;
}

export interface CheckEvidenceAttachment {
  kind: string;
  ref: string;
}

export interface CheckEvidence {
  schemaVersion: 2;
  status: CheckEvidenceStatus;
  code: string;
  phase: CheckEvidencePhase;
  actor: string | null;
  summary: string | null;
  // The typed finding behind a failed or inconclusive check, from its
  // failing action; null when the check passed or the harness failed.
  finding: Finding | null;
  observation: unknown;
  expected: unknown;
  retryable: boolean;
  timing: CheckEvidenceTiming;
  actions: CheckEvidenceActionEntry[];
  attachments: CheckEvidenceAttachment[];
  sensitivity: string[];
}

export interface CreateCheckEvidenceInput {
  status: CheckEvidenceStatus;
  code: string;
  phase: CheckEvidencePhase;
  actor?: string | null;
  summary?: string | null;
  observation?: unknown;
  expected?: unknown;
  retryable?: boolean;
  startedAtMs: number;
  completedAtMs: number;
  actions?: readonly CheckEvidenceActionEntry[];
  attachments?: readonly CheckEvidenceAttachment[];
  sensitivity?: readonly string[];
  finding?: Finding | null;
}

// This is the single semantic interpretation of a check/action status. Keep
// diagnostic prose out of this table: summaries are presentation evidence and
// must never decide scoring, repair eligibility, or run outcome.
export const CHECK_EVIDENCE_DISPOSITIONS = Object.freeze({
  passed: Object.freeze({
    status: 'passed', label: 'PASS', outcomeKind: 'passed', passed: true,
    measured: true, applicationFailure: false, repairable: false,
  }),
  failed: Object.freeze({
    status: 'failed', label: 'FAIL', outcomeKind: 'app_failure', passed: false,
    measured: true, applicationFailure: true, repairable: true,
  }),
  inconclusive: Object.freeze({
    status: 'inconclusive', label: 'INCONCLUSIVE', outcomeKind: 'inconclusive', passed: false,
    measured: false, applicationFailure: false, repairable: false,
  }),
  harness_failure: Object.freeze({
    status: 'harness_failure', label: 'HARNESS FAILURE', outcomeKind: 'harness_failure',
    passed: false, measured: false, applicationFailure: false, repairable: false,
  }),
} satisfies Record<CheckEvidenceStatus, CheckEvidenceDisposition>);

const STATUS = new Set<string>(CHECK_EVIDENCE_STATUSES);
const CODE = /^[a-z][a-z0-9_]*(?:[.:-][a-z0-9_]+)*$/;
const nonEmptyStringSchema = z.string().refine(value => value.trim().length > 0);
const timingSchema = z.strictObject({
  startedAtMs: z.number().finite().nonnegative(),
  completedAtMs: z.number().finite().nonnegative(),
  durationMs: z.number().finite().nonnegative(),
});
const actionTimingSchema = timingSchema.extend({ deadlineMs: z.number().finite().nonnegative() });
const actionEvidenceSchema = z.strictObject({
  schemaVersion: z.literal(2),
  action: z.strictObject({ id: nonEmptyStringSchema, version: nonEmptyStringSchema }),
  status: z.enum(CHECK_EVIDENCE_STATUSES),
  type: nonEmptyStringSchema,
  code: nonEmptyStringSchema,
  phase: nonEmptyStringSchema,
  summary: z.string().nullable(),
  finding: findingSchema.nullable(),
  observation: z.unknown(),
  expected: z.unknown(),
  retryable: z.boolean(),
  timing: actionTimingSchema,
  attachments: z.array(z.unknown()),
  sensitivity: z.array(nonEmptyStringSchema),
});
const checkEvidenceSchema = z.strictObject({
  schemaVersion: z.literal(CHECK_EVIDENCE_SCHEMA_VERSION),
  status: z.enum(CHECK_EVIDENCE_STATUSES),
  code: z.string().regex(CODE),
  phase: z.enum(CHECK_EVIDENCE_PHASES),
  actor: nonEmptyStringSchema.nullable(),
  summary: z.string().nullable(),
  finding: findingSchema.nullable(),
  observation: z.unknown(),
  expected: z.unknown(),
  retryable: z.boolean(),
  timing: timingSchema,
  actions: z.array(z.strictObject({ actor: nonEmptyStringSchema.nullable(),
    evidence: actionEvidenceSchema })),
  attachments: z.array(z.strictObject({ kind: nonEmptyStringSchema, ref: nonEmptyStringSchema })),
  sensitivity: z.array(nonEmptyStringSchema),
});

function structured(value: unknown, at: string, seen = new Set<object>()): void {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return;
  if (typeof value === 'number' && Number.isFinite(value)) return;
  if (typeof value !== 'object' || seen.has(value)) {
    throw new Error(`${at} must be finite, acyclic JSON data`);
  }
  const prototype = Object.getPrototypeOf(value);
  if (!Array.isArray(value) && prototype !== Object.prototype && prototype !== null) {
    throw new Error(`${at} must be plain JSON data`);
  }
  seen.add(value);
  if (Array.isArray(value)) {
    value.forEach((item, index) => structured(item, `${at}[${index}]`, seen));
  } else {
    for (const [key, item] of Object.entries(value)) structured(item, `${at}.${key}`, seen);
  }
  seen.delete(value);
}

function validateTiming(
  value: CheckEvidenceTiming & { deadlineMs?: number },
  at: string,
): void {
  if (value.completedAtMs < value.startedAtMs) throw new Error(`${at} completes before it starts`);
  if (value.durationMs !== value.completedAtMs - value.startedAtMs) {
    throw new Error(`${at}.durationMs does not match its endpoints`);
  }
}

function validateStringList(value: string[], at: string): void {
  if (new Set(value).size !== value.length) throw new Error(`${at} contains duplicates`);
}

function isCheckEvidenceStatus(value: unknown): value is CheckEvidenceStatus {
  return typeof value === 'string' && STATUS.has(value);
}

function validateActionEvidence(value: z.infer<typeof actionEvidenceSchema>, at: string): void {
  structured(value.observation, `${at}.observation`);
  structured(value.expected, `${at}.expected`);
  validateTiming(value.timing, `${at}.timing`);
  structured(value.attachments, `${at}.attachments`);
  validateStringList(value.sensitivity, `${at}.sensitivity`);
}

export function validateCheckEvidence(
  value: unknown,
  { at = 'check evidence' }: { at?: string } = {},
): CheckEvidence {
  const parsed = checkEvidenceSchema.safeParse(value);
  if (!parsed.success) {
    throw new Error(formatZodError(parsed.error, at));
  }
  const evidence = parsed.data;
  structured(evidence.observation, `${at}.observation`);
  structured(evidence.expected, `${at}.expected`);
  validateTiming(evidence.timing, `${at}.timing`);
  evidence.actions.forEach((entry, index) => {
    const entryAt = `${at}.actions[${index}]`;
    validateActionEvidence(entry.evidence, `${entryAt}.evidence`);
  });
  validateStringList(evidence.sensitivity, `${at}.sensitivity`);
  return value as CheckEvidence;
}

export function createCheckEvidence({
  status,
  code,
  phase,
  actor = null,
  summary = null,
  observation = null,
  expected = null,
  finding = null,
  retryable = false,
  startedAtMs,
  completedAtMs,
  actions = [],
  attachments = [],
  sensitivity = [],
}: CreateCheckEvidenceInput): CheckEvidence {
  const value = {
    schemaVersion: CHECK_EVIDENCE_SCHEMA_VERSION,
    status,
    code,
    phase,
    actor,
    summary: summary == null ? null : String(summary).slice(0, 2_000),
    finding,
    observation,
    expected,
    retryable,
    timing: {
      startedAtMs,
      completedAtMs,
      durationMs: Math.max(0, completedAtMs - startedAtMs),
    },
    actions: actions.map(entry => ({ actor: entry.actor ?? null, evidence: entry.evidence })),
    attachments: attachments.map(attachment => ({ ...attachment })),
    sensitivity: [...new Set(sensitivity)].sort(),
  };
  return validateCheckEvidence(value);
}

export function criterionEvidence(criterion: {
  id?: string;
  evidence?: unknown;
} | null | undefined): CheckEvidence {
  if (criterion?.evidence === undefined) {
    throw new Error(`criterion ${criterion?.id ?? '<unknown>'}.evidence is required`);
  }
  return validateCheckEvidence(criterion.evidence, {
    at: `criterion ${criterion.id ?? '<unknown>'}.evidence`,
  });
}

export function evidenceDisposition(
  value: CheckEvidence | CheckEvidenceStatus,
): CheckEvidenceDisposition {
  const status = typeof value === 'string' ? value : value?.status;
  if (!isCheckEvidenceStatus(status)) {
    throw new Error(`check evidence status is invalid: ${String(status)}`);
  }
  return CHECK_EVIDENCE_DISPOSITIONS[status];
}

export function evidenceIsMeasured(evidence: CheckEvidence): boolean {
  return evidenceDisposition(evidence).measured;
}

export function evidencePassed(evidence: CheckEvidence): boolean {
  return evidenceDisposition(evidence).passed;
}

export function evidenceIsApplicationFailure(evidence: CheckEvidence): boolean {
  return evidenceDisposition(evidence).applicationFailure;
}

export function evidenceIsRepairable(evidence: CheckEvidence): boolean {
  return evidenceDisposition(evidence).repairable;
}
