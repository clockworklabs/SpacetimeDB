export const CHECK_EVIDENCE_SCHEMA_VERSION = 1;

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
  schemaVersion: 1;
  status: CheckEvidenceStatus;
  code: string;
  phase: CheckEvidencePhase;
  actor: string | null;
  summary: string | null;
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

type UnknownRecord = Record<string, unknown>;

const STATUS = new Set<string>(CHECK_EVIDENCE_STATUSES);
const PHASE = new Set<string>(CHECK_EVIDENCE_PHASES);
const FIELDS = new Set([
  'schemaVersion', 'status', 'code', 'phase', 'actor', 'summary', 'observation', 'expected',
  'retryable', 'timing', 'actions', 'attachments', 'sensitivity',
]);
const TIMING_FIELDS = new Set(['startedAtMs', 'completedAtMs', 'durationMs']);
const ACTION_ENTRY_FIELDS = new Set(['actor', 'evidence']);
const ATTACHMENT_FIELDS = new Set(['kind', 'ref']);
const ACTION_EVIDENCE_FIELDS = new Set([
  'schemaVersion', 'action', 'status', 'type', 'code', 'phase', 'summary', 'observation',
  'expected', 'retryable', 'timing', 'attachments', 'sensitivity',
]);
const ACTION_FIELDS = new Set(['id', 'version']);
const ACTION_TIMING_FIELDS = new Set([
  'startedAtMs', 'completedAtMs', 'durationMs', 'deadlineMs',
]);
const CODE = /^[a-z][a-z0-9_]*(?:[.:-][a-z0-9_]+)*$/;

const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const nonEmpty = (value: unknown): value is string =>
  typeof value === 'string' && value.trim().length > 0;

function strict(value: unknown, fields: ReadonlySet<string>, at: string): asserts value is UnknownRecord {
  if (!object(value)) throw new Error(`${at} must be an object`);
  for (const key of Object.keys(value)) {
    if (!fields.has(key)) throw new Error(`${at}.${key} is unknown`);
  }
}

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

function finiteNonNegative(value: unknown, at: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    throw new Error(`${at} must be a non-negative number`);
  }
  return value;
}

function validateTiming(
  value: unknown,
  at: string,
  fields: ReadonlySet<string> = TIMING_FIELDS,
): void {
  strict(value, fields, at);
  const startedAtMs = finiteNonNegative(value.startedAtMs, `${at}.startedAtMs`);
  const completedAtMs = finiteNonNegative(value.completedAtMs, `${at}.completedAtMs`);
  const durationMs = finiteNonNegative(value.durationMs, `${at}.durationMs`);
  if (completedAtMs < startedAtMs) throw new Error(`${at} completes before it starts`);
  if (durationMs !== completedAtMs - startedAtMs) {
    throw new Error(`${at}.durationMs does not match its endpoints`);
  }
  if (fields.has('deadlineMs')) finiteNonNegative(value.deadlineMs, `${at}.deadlineMs`);
}

function validateStringList(value: unknown, at: string): void {
  if (!Array.isArray(value) || !value.every(nonEmpty)) {
    throw new Error(`${at} must be a string array`);
  }
  if (new Set(value).size !== value.length) throw new Error(`${at} contains duplicates`);
}

function isCheckEvidenceStatus(value: unknown): value is CheckEvidenceStatus {
  return typeof value === 'string' && STATUS.has(value);
}

function validateActionEvidence(value: unknown, at: string): void {
  strict(value, ACTION_EVIDENCE_FIELDS, at);
  if (value.schemaVersion !== 1) throw new Error(`${at}.schemaVersion is unsupported`);
  const action = value.action;
  strict(action, ACTION_FIELDS, `${at}.action`);
  if (!nonEmpty(action.id) || !nonEmpty(action.version)) {
    throw new Error(`${at}.action requires id and version`);
  }
  if (!isCheckEvidenceStatus(value.status)) throw new Error(`${at}.status is invalid`);
  for (const key of ['type', 'code', 'phase']) {
    if (!nonEmpty(value[key])) throw new Error(`${at}.${key} must be a non-empty string`);
  }
  if (value.summary !== null && typeof value.summary !== 'string') {
    throw new Error(`${at}.summary must be a string or null`);
  }
  structured(value.observation, `${at}.observation`);
  structured(value.expected, `${at}.expected`);
  if (typeof value.retryable !== 'boolean') throw new Error(`${at}.retryable must be boolean`);
  validateTiming(value.timing, `${at}.timing`, ACTION_TIMING_FIELDS);
  if (!Array.isArray(value.attachments)) throw new Error(`${at}.attachments must be an array`);
  structured(value.attachments, `${at}.attachments`);
  validateStringList(value.sensitivity, `${at}.sensitivity`);
}

export function validateCheckEvidence(
  value: unknown,
  { at = 'check evidence' }: { at?: string } = {},
): CheckEvidence {
  strict(value, FIELDS, at);
  if (value.schemaVersion !== CHECK_EVIDENCE_SCHEMA_VERSION) {
    throw new Error(`${at}.schemaVersion is unsupported`);
  }
  if (!isCheckEvidenceStatus(value.status)) throw new Error(`${at}.status is invalid`);
  if (!nonEmpty(value.code) || !CODE.test(value.code)) throw new Error(`${at}.code is invalid`);
  if (typeof value.phase !== 'string' || !PHASE.has(value.phase)) {
    throw new Error(`${at}.phase is invalid`);
  }
  if (value.actor !== null && !nonEmpty(value.actor)) {
    throw new Error(`${at}.actor must be a string or null`);
  }
  if (value.summary !== null && typeof value.summary !== 'string') {
    throw new Error(`${at}.summary must be a string or null`);
  }
  structured(value.observation, `${at}.observation`);
  structured(value.expected, `${at}.expected`);
  if (typeof value.retryable !== 'boolean') throw new Error(`${at}.retryable must be boolean`);
  validateTiming(value.timing, `${at}.timing`);
  if (!Array.isArray(value.actions)) throw new Error(`${at}.actions must be an array`);
  value.actions.forEach((entry, index) => {
    const entryAt = `${at}.actions[${index}]`;
    strict(entry, ACTION_ENTRY_FIELDS, entryAt);
    if (entry.actor !== null && !nonEmpty(entry.actor)) {
      throw new Error(`${entryAt}.actor must be a string or null`);
    }
    validateActionEvidence(entry.evidence, `${entryAt}.evidence`);
  });
  if (!Array.isArray(value.attachments)) throw new Error(`${at}.attachments must be an array`);
  value.attachments.forEach((attachment, index) => {
    const attachmentAt = `${at}.attachments[${index}]`;
    strict(attachment, ATTACHMENT_FIELDS, attachmentAt);
    if (!nonEmpty(attachment.kind) || !nonEmpty(attachment.ref)) {
      throw new Error(`${attachmentAt} requires kind and ref`);
    }
  });
  validateStringList(value.sensitivity, `${at}.sensitivity`);
  return value as unknown as CheckEvidence;
}

export function createCheckEvidence({
  status,
  code,
  phase,
  actor = null,
  summary = null,
  observation = null,
  expected = null,
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
