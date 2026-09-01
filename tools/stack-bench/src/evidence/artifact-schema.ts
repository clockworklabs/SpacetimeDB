import { validateCheckEvidence } from './check-evidence.js';
import type { CheckEvidence } from './check-evidence.js';
import { z } from 'zod';
import { runOutcomeKind } from './outcomes.js';
import { RUNNER_OBSERVATION_FIELDS } from '../runtime/runner-environment.js';
import { ARTIFACT_FILE } from './artifact-layout.js';
import {
  ARTIFACT_IDENTITY_KEYS as IDENTITY_KEYS,
  currentEngineIdentity,
  emptyArtifactIdentities,
  recipeArtifactIdentities,
  validateArtifactIdentities,
} from './artifact-identities.js';
import type {
  ArtifactIdentities,
  ArtifactIdentity,
  EngineArtifactIdentity,
} from './artifact-identities.js';
import { formatZodError } from '../zod-error.js';

export {
  currentEngineIdentity,
  emptyArtifactIdentities,
  recipeArtifactIdentities,
};
export type {
  ArtifactIdentities,
  ArtifactIdentity,
  EngineArtifactIdentity,
};

export const ARTIFACT_SCHEMA_VERSION = 2;

export const ARTIFACT_KINDS = Object.freeze([
  'action_check',
  'benchmark_run',
  'backend_lease_evidence',
  'bug_report_quality',
  'campaign_admission',
  'campaign_process',
  'campaign_plan',
  'campaign_report',
  'campaign_state',
  'contract_lint',
  'grade',
  'grade_bundle',
  'mutation_control',
  'null_control',
  'pack_budget_measurement',
  'performance_run',
  'preflight',
  'progression_state',
  'repair_continuation',
  'repair_process',
  'reference_build',
  'reference_qualification',
  'recovery',
  'source_checkpoint',
] as const);

export type ArtifactKind = typeof ARTIFACT_KINDS[number];
type UnknownRecord = Record<string, unknown>;

export interface Artifact<TPayload extends object = UnknownRecord> {
  artifactSchemaVersion: 2;
  kind: ArtifactKind;
  id: string;
  attempt: { id: string; parentId: string | null };
  timestamps: { startedAt: string; completedAt: string | null };
  identities: ArtifactIdentities;
  payload: TPayload;
}

export interface CreateArtifactInput {
  kind: unknown;
  id: string;
  attempt?: unknown;
  timestamps?: unknown;
  identities?: unknown;
  payload?: unknown;
}

const KIND_SET = new Set<string>(ARTIFACT_KINDS);
const SECRET_KEYS = new Set([
  'apikey', 'authorization', 'credential', 'credentials', 'leasetoken', 'ownershiptoken',
  'password', 'secret',
]);
const BENCHMARK_RUN_PAYLOAD_FIELDS = new Set(['status', 'mode', 'track', 'backend', 'model', 'guidance',
  'condition', 'stack', 'setup', 'backendLease', 'backendDiagnostics', 'validation', 'levels',
  'contaminated', 'contamination', 'mutationControl', 'totals', 'outcome', 'selectionRequest',
  'skills', 'runtime', 'pricing', 'featureCatalog', 'dependencyPolicy', 'progressionOwner', 'progressionStatus',
  'progressionResume']);
const PAYLOAD_FIELDS = Object.freeze({
  action_check: new Set(['backend', 'results', 'missing']),
  backend_lease_evidence: new Set(['version', 'runId', 'backend', 'track', 'runIndex', 'ownerPid',
    'createdAt', 'stoppedAt', 'releasedAt', 'state', 'resources', 'ownership']),
  benchmark_run: BENCHMARK_RUN_PAYLOAD_FIELDS,
  bug_report_quality: new Set(['bugs', 'vague', 'vaguePct']),
  campaign_admission: new Set(['schemaVersion', 'campaignId', 'campaignSha256', 'createdAt',
    'ok', 'runtime', 'agents', 'conditions', 'reports']),
  campaign_process: new Set(['schemaVersion', 'executionId', 'runIndex', 'exitCode', 'signal', 'timedOut',
    'streams']),
  campaign_plan: new Set(['campaignSchemaVersion', 'id', 'version', 'state', 'title', 'source',
    'contentSha256', 'definition', 'identities', 'bindings', 'stacks', 'agents', 'conditions',
    'attempts', 'summary', 'featureCatalog', 'dependencyPolicy']),
  campaign_report: new Set(['reportSchemaVersion', 'campaign', 'scope', 'policy', 'attempts',
    'conditions', 'summary', 'limitations', 'contentSha256']),
  campaign_state: new Set(['schemaVersion', 'campaignId', 'campaignSha256', 'status',
    'createdAt', 'updatedAt', 'maxParallel', 'attempts', 'summary']),
  contract_lint: new Set(['label', 'url', 'level', 'selectedHooks', 'pass', 'counts', 'results']),
  grade: new Set(['definitionSchemaVersion', 'recipeRelease', 'label', 'url', 'level', 'runId',
    'total', 'max', 'features', 'environment', 'inconclusive', 'selection', 'packRuntime']),
  grade_bundle: new Set(['definitionSchemaVersion', 'recipeRelease', 'calibration', 'label', 'track',
    'backend', 'url', 'app', 'level', 'suites', 'totals', 'code', 'error', 'outcome', 'provenance',
    'actions', 'selection', 'packRuntime', 'observation', 'source']),
  mutation_control: new Set(['durationMs', 'app', 'mutations', 'manifestStatus', 'fixtureSha256',
    'spec', 'backend', 'track', 'shard', 'ok', 'outcome', 'baseline', 'summary', 'results',
    'checkpoint']),
  null_control: new Set(['durationMs', 'runner', 'qualificationScope', 'tracks', 'ok', 'summary', 'criteria']),
  pack_budget_measurement: new Set(['schemaVersion', 'track', 'level', 'policy', 'evidence',
    'runner', 'samples', 'recommendations']),
  performance_run: new Set(['label', 'backend', 'url', 'clients', 'rounds', 'warmupDiscarded',
    'seededBefore', 'sent', 'delivered', 'lost', 'elapsedMs', 'deliveryLatencyMs', 'server',
    'cpuSecondsPer1kDelivered']),
  preflight: new Set(['schemaVersion', 'generatedAt', 'request', 'ok', 'summary', 'checks']),
  progression_state: new Set(['schemaVersion', 'owner', 'featureCatalog', 'dependencyPolicy',
    'events', 'resume', 'stateSha256']),
  repair_continuation: new Set([...BENCHMARK_RUN_PAYLOAD_FIELDS, 'continuation']),
  repair_process: new Set(['schemaVersion', 'parentRunId', 'level', 'roundsGranted',
    'exitCode', 'signal', 'timedOut', 'streams']),
  reference_build: new Set(['isolation', 'image', 'fixtures', 'ok']),
  reference_qualification: new Set(['fixture', 'fixtureSha256', 'requiredRepetitions', 'isolation',
    'runner', 'qualificationScope', 'mutationControl', 'runs', 'stable', 'sameImage', 'sameHarness',
    'harnessSha256', 'qualifiedCheckKeys', 'featureCatalog', 'diagnostic', 'ok']),
  recovery: new Set(['schemaVersion', 'status', 'runId', 'backend', 'reason', 'cleanup',
    'resources', 'instructions']),
  source_checkpoint: new Set(['schemaVersion', 'track', 'backend', 'level', 'source',
    'repair', 'outcome', 'selectionSha256']),
});
const PAYLOAD_SCHEMAS = Object.fromEntries(Object.entries(PAYLOAD_FIELDS).map(([kind, fields]) => [
  kind,
  z.strictObject(Object.fromEntries([...fields].map(field => [field, z.unknown().optional()]))),
])) as unknown as Record<ArtifactKind, z.ZodType<UnknownRecord>>;
const HASH = /^[a-f0-9]{64}$/;
const ISO = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/;
const artifactTimestampSchema = z.string().regex(ISO).refine(value => !Number.isNaN(Date.parse(value)));
const artifactEnvelopeSchema = z.strictObject({
  artifactSchemaVersion: z.literal(ARTIFACT_SCHEMA_VERSION),
  kind: z.enum(ARTIFACT_KINDS),
  id: z.string().min(1),
  attempt: z.strictObject({ id: z.string().min(1), parentId: z.string().min(1).nullable() }),
  timestamps: z.strictObject({
    startedAt: artifactTimestampSchema,
    completedAt: artifactTimestampSchema.nullable(),
  }),
  identities: z.record(z.string(), z.unknown()),
  payload: z.record(z.string(), z.unknown()),
});

const isObject = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const isSafeInteger = (value: unknown): value is number => Number.isSafeInteger(value);
const isInteger = (value: unknown): value is number => Number.isInteger(value);
const isFiniteNumber = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value);
const isHash = (value: unknown): value is string => typeof value === 'string' && HASH.test(value);
function fail(message: string): never {
  throw new Error(`invalid artifact: ${message}`);
}

function asObject(value: unknown, message: string): UnknownRecord {
  if (!isObject(value)) fail(message);
  return value;
}

function validateRunOutcome(value: unknown, at: string): void {
  const outcome = asObject(value, `${at} must be an object`);
  try { runOutcomeKind(outcome.kind); }
  catch { fail(`${at}.kind is invalid`); }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function normalizeKind(kind: unknown): ArtifactKind {
  if (typeof kind !== 'string' || !KIND_SET.has(kind)) {
    fail(`unknown kind ${JSON.stringify(kind)}`);
  }
  return kind as ArtifactKind;
}

function timestamp(value: unknown, at: string): string {
  if (typeof value !== 'string' || !ISO.test(value) || Number.isNaN(Date.parse(value))) {
    fail(`${at} must be an ISO-8601 UTC timestamp`);
  }
  return value;
}

function validatePayload(kind: ArtifactKind, input: unknown): UnknownRecord {
  const parsed = PAYLOAD_SCHEMAS[kind].safeParse(input);
  if (!parsed.success) {
    fail(formatZodError(parsed.error, `${kind} payload`));
  }
  const payload = parsed.data;
  const arrayWhenPresent = (field: string): unknown[] | undefined => {
    const value = payload[field];
    if (value !== undefined && !Array.isArray(value)) {
      fail(`${kind} payload.${field} must be an array when present`);
    }
    return value;
  };
  const objectWhenPresent = (field: string): UnknownRecord | undefined => {
    const value = payload[field];
    if (value !== undefined && !isObject(value)) {
      fail(`${kind} payload.${field} must be an object when present`);
    }
    return value;
  };
  const runnerWhenPresent = () => {
    const runner = objectWhenPresent('runner');
    if (runner === undefined) return;
    const allowed = new Set(['schemaVersion', 'mode', 'platform', 'architecture',
      ...RUNNER_OBSERVATION_FIELDS]);
    for (const key of Object.keys(runner)) {
      if (!allowed.has(key)) fail(`${kind} payload.runner.${key} is unknown`);
    }
    if (runner.schemaVersion !== 1) fail(`${kind} payload.runner.schemaVersion must be 1`);
    if (!['appliance', 'local-controller'].includes(String(runner.mode))) {
      fail(`${kind} payload.runner.mode is invalid`);
    }
    for (const field of ['platform', 'architecture']) {
      if (typeof runner[field] !== 'string' || !runner[field]) {
        fail(`${kind} payload.runner.${field} must be a non-empty string`);
      }
    }
    const observedFields = RUNNER_OBSERVATION_FIELDS.filter(field => runner[field] !== undefined);
    if (runner.mode === 'appliance' && observedFields.length > 0) {
      for (const field of ['dockerEngineVersion', 'dockerOs', 'dockerArchitecture', 'kernelVersion']) {
        if (typeof runner[field] !== 'string' || !runner[field]) {
          fail(`${kind} payload.runner.${field} must be a non-empty string for an appliance runner`);
        }
      }
      for (const field of ['cpuCount', 'memoryBytes']) {
        if (!isSafeInteger(runner[field]) || runner[field] < 1) {
          fail(`${kind} payload.runner.${field} must be a positive integer for an appliance runner`);
        }
      }
    } else if (runner.mode !== 'appliance') {
      for (const field of RUNNER_OBSERVATION_FIELDS) {
        if (runner[field] !== undefined) {
          fail(`${kind} payload.runner.${field} is only valid for an appliance runner`);
        }
      }
    }
  };
  if (['benchmark_run', 'repair_continuation'].includes(kind)) {
    for (const [index, levelValue] of (arrayWhenPresent('levels') ?? []).entries()) {
      const level = asObject(levelValue, `${kind} payload.levels[${index}] must be an object`);
      if (level.outcome !== undefined) {
        validateRunOutcome(level.outcome, `${kind} payload.levels[${index}].outcome`);
      }
    }
    if (payload.outcome !== undefined) validateRunOutcome(payload.outcome, `${kind} payload.outcome`);
  }
  if (['benchmark_run', 'repair_continuation'].includes(kind)
    && payload.progressionStatus !== undefined) {
    const progressionStatus = objectWhenPresent('progressionStatus');
    if (progressionStatus === undefined || !isObject(payload.featureCatalog)
      || !isObject(payload.dependencyPolicy)) {
      fail(`${kind} payload.progressionStatus requires featureCatalog and dependencyPolicy`);
    }
    const fields = new Set(['stateArtifact', 'phase', 'level', 'attempts', 'score']);
    for (const key of Object.keys(progressionStatus)) {
      if (!fields.has(key)) fail(`${kind} payload.progressionStatus.${key} is unknown`);
    }
    if (progressionStatus.stateArtifact !== ARTIFACT_FILE.progressionState) {
      fail(`${kind} payload.progressionStatus.stateArtifact is invalid`);
    }
    if (!['active', 'terminal'].includes(String(progressionStatus.phase))) {
      fail(`${kind} payload.progressionStatus.phase is invalid`);
    }
    const numericFields = [['level', 1], ['attempts', 0]] as const;
    for (const [field, minimum] of numericFields) {
      if (!isSafeInteger(progressionStatus[field]) || progressionStatus[field] < minimum) {
        fail(`${kind} payload.progressionStatus.${field} is invalid`);
      }
    }
    if (!isObject(progressionStatus.score)) {
      fail(`${kind} payload.progressionStatus.score must be an object`);
    }
  }
  if (kind === 'benchmark_run' && payload.progressionResume !== undefined) {
    const progressionResume = asObject(payload.progressionResume,
      'benchmark_run payload.progressionResume must be an object');
    const fields = new Set(['priorRunId', 'priorRunSha256', 'stateSha256',
      'action', 'inheritedLevels', 'priorTotals']);
    for (const key of Object.keys(progressionResume)) {
      if (!fields.has(key)) fail(`benchmark_run payload.progressionResume.${key} is unknown`);
    }
    if (typeof progressionResume.priorRunId !== 'string' || !progressionResume.priorRunId) {
      fail('benchmark_run payload.progressionResume.priorRunId is invalid');
    }
    for (const field of ['priorRunSha256', 'stateSha256']) {
      if (!isHash(progressionResume[field])) {
        fail(`benchmark_run payload.progressionResume.${field} is invalid`);
      }
    }
    const action = progressionResume.action;
    if (!isObject(action) || !['build', 'repair', 'terminal'].includes(String(action.type))
      || (action.type !== 'terminal' && (!isSafeInteger(action.level) || action.level < 1))) {
      fail('benchmark_run payload.progressionResume.action is invalid');
    }
    if (!Array.isArray(progressionResume.inheritedLevels)
      || progressionResume.inheritedLevels.some(level => !isSafeInteger(level) || level < 1)) {
      fail('benchmark_run payload.progressionResume.inheritedLevels is invalid');
    }
    if (progressionResume.priorTotals !== null && !isObject(progressionResume.priorTotals)) {
      fail('benchmark_run payload.progressionResume.priorTotals is invalid');
    }
  }
  if (kind === 'progression_state') {
    if (payload.schemaVersion !== 3) fail('progression_state payload.schemaVersion must be 3');
    objectWhenPresent('owner');
    objectWhenPresent('featureCatalog');
    objectWhenPresent('dependencyPolicy');
    arrayWhenPresent('events');
    if (!isHash(payload.stateSha256)) {
      fail('progression_state payload.stateSha256 is invalid');
    }
    if (payload.resume !== undefined) {
      const resume = asObject(payload.resume, 'progression_state payload.resume must be an object');
      const resumeFields = new Set(['actionSha256', 'source']);
      for (const key of Object.keys(resume)) {
        if (!resumeFields.has(key)) fail(`progression_state payload.resume.${key} is unknown`);
      }
      if (!isHash(resume.actionSha256)) {
        fail('progression_state payload.resume.actionSha256 is invalid');
      }
      const source = asObject(resume.source, 'progression_state payload.resume.source must be an object');
      const sourceFields = new Set(['directory', 'sha256', 'files']);
      for (const key of Object.keys(source)) {
        if (!sourceFields.has(key)) fail(`progression_state payload.resume.source.${key} is unknown`);
      }
      if (typeof source.directory !== 'string' || !source.directory) {
        fail('progression_state payload.resume.source.directory is invalid');
      }
      if (!isHash(source.sha256)) {
        fail('progression_state payload.resume.source.sha256 is invalid');
      }
      if (!isSafeInteger(source.files) || source.files < 0) {
        fail('progression_state payload.resume.source.files is invalid');
      }
    }
  }
  if (kind === 'repair_continuation') {
    const continuation = objectWhenPresent('continuation');
    if (continuation === undefined) fail('repair_continuation payload.continuation is required');
    const fields = new Set(['schemaVersion', 'rootRunId', 'parentRunId', 'level', 'grantIndex',
      'roundsGranted', 'cumulativeRoundsBefore', 'cumulativeRoundsAfter', 'parentCheckpointSha256',
      'baseline', 'resumeSetup', 'downstreamLevelsToRerun', 'cumulativeCostBeforeUsd',
      'cumulativeCostAfterUsd', 'cumulativeDurationBeforeSec', 'cumulativeDurationAfterSec']);
    for (const key of Object.keys(continuation)) {
      if (!fields.has(key)) fail(`repair_continuation payload.continuation.${key} is unknown`);
    }
    if (continuation.schemaVersion !== 1) {
      fail('repair_continuation payload.continuation.schemaVersion must be 1');
    }
    for (const field of ['rootRunId', 'parentRunId']) {
      if (typeof continuation[field] !== 'string' || !continuation[field]) {
        fail(`repair_continuation payload.continuation.${field} is required`);
      }
    }
    for (const field of ['level', 'grantIndex', 'roundsGranted', 'cumulativeRoundsBefore',
      'cumulativeRoundsAfter']) {
      if (!isSafeInteger(continuation[field]) || continuation[field] < 0) {
        fail(`repair_continuation payload.continuation.${field} must be a non-negative integer`);
      }
    }
    const level = continuation.level;
    const grantIndex = continuation.grantIndex;
    const roundsGranted = continuation.roundsGranted;
    const cumulativeRoundsBefore = continuation.cumulativeRoundsBefore;
    const cumulativeRoundsAfter = continuation.cumulativeRoundsAfter;
    if (!isSafeInteger(level) || !isSafeInteger(grantIndex) || !isSafeInteger(roundsGranted)
      || !isSafeInteger(cumulativeRoundsBefore) || !isSafeInteger(cumulativeRoundsAfter)) {
      fail('repair_continuation payload.continuation round accounting is invalid');
    }
    if (level < 1 || grantIndex < 1 || roundsGranted < 1
      || cumulativeRoundsAfter < cumulativeRoundsBefore
      || cumulativeRoundsAfter > cumulativeRoundsBefore + roundsGranted) {
      fail('repair_continuation payload.continuation round accounting is invalid');
    }
    if (!isHash(continuation.parentCheckpointSha256)) {
      fail('repair_continuation payload.continuation.parentCheckpointSha256 is invalid');
    }
    const cumulativeFields = [['cumulativeCostBeforeUsd', 'cumulativeCostAfterUsd'],
      ['cumulativeDurationBeforeSec', 'cumulativeDurationAfterSec']] as const;
    for (const [before, after] of cumulativeFields) {
      const beforeValue = continuation[before];
      const afterValue = continuation[after];
      if (!isFiniteNumber(beforeValue) || beforeValue < 0
        || !isFiniteNumber(afterValue) || afterValue < beforeValue) {
        fail(`repair_continuation payload.continuation.${after} is invalid`);
      }
    }
    const downstreamLevels = continuation.downstreamLevelsToRerun;
    if (!Array.isArray(downstreamLevels)
      || downstreamLevels.some(item => !isSafeInteger(item) || item <= level)
      || new Set(downstreamLevels).size !== downstreamLevels.length) {
      fail('repair_continuation payload.continuation.downstreamLevelsToRerun is invalid');
    }
    if (continuation.baseline !== null) {
      const baseline = asObject(continuation.baseline,
        'repair_continuation payload.continuation.baseline must be an object or null');
      const baselineFields = new Set(['score', 'max', 'selectionSha256', 'sourceSha256',
        'outcome', 'reproduced', 'mismatches']);
      for (const key of Object.keys(baseline)) {
        if (!baselineFields.has(key)) fail(`repair_continuation payload.continuation.baseline.${key} is unknown`);
      }
      const scoreValid = baseline.score === null && baseline.max === null
        || isSafeInteger(baseline.score) && isSafeInteger(baseline.max)
          && baseline.max >= 1 && baseline.score >= 0 && baseline.score <= baseline.max;
      const sourceValid = baseline.sourceSha256 === null
        || isHash(baseline.sourceSha256);
      if (!scoreValid
        || (baseline.selectionSha256 !== null && !isHash(baseline.selectionSha256))
        || !sourceValid || !isObject(baseline.outcome) || typeof baseline.reproduced !== 'boolean'
        || !Array.isArray(baseline.mismatches)
        || baseline.mismatches.some(item => typeof item !== 'string' || !item)
        || (baseline.reproduced && (baseline.sourceSha256 === null || baseline.score === null))) {
        fail('repair_continuation payload.continuation.baseline is invalid');
      }
      validateRunOutcome(baseline.outcome,
        'repair_continuation payload.continuation.baseline.outcome');
    }
    if (continuation.resumeSetup !== null) {
      const setup = asObject(continuation.resumeSetup,
        'repair_continuation payload.continuation.resumeSetup must be an object or null');
      const setupFields = new Set(['sessionId', 'costUsd', 'durationMs', 'sourceVerified']);
      for (const key of Object.keys(setup)) {
        if (!setupFields.has(key)) fail(`repair_continuation payload.continuation.resumeSetup.${key} is unknown`);
      }
      if (setup.sessionId !== null && (typeof setup.sessionId !== 'string' || !setup.sessionId)) {
        fail('repair_continuation payload.continuation.resumeSetup.sessionId is invalid');
      }
      if (!isFiniteNumber(setup.costUsd) || setup.costUsd < 0
        || !isFiniteNumber(setup.durationMs) || setup.durationMs < 0
        || typeof setup.sourceVerified !== 'boolean') {
        fail('repair_continuation payload.continuation.resumeSetup is invalid');
      }
    }
  }
  if (kind === 'campaign_process') {
    if (payload.schemaVersion !== 1) fail('campaign_process payload.schemaVersion must be 1');
    if (typeof payload.executionId !== 'string' || !payload.executionId) {
      fail('campaign_process payload.executionId is required');
    }
    if (payload.runIndex !== undefined
      && (!isInteger(payload.runIndex) || payload.runIndex < 0)) {
      fail('campaign_process payload.runIndex must be a non-negative integer');
    }
    if (payload.exitCode !== null && !isInteger(payload.exitCode)) {
      fail('campaign_process payload.exitCode must be an integer or null');
    }
    if (payload.signal !== null && typeof payload.signal !== 'string') {
      fail('campaign_process payload.signal must be a string or null');
    }
    if (typeof payload.timedOut !== 'boolean') fail('campaign_process payload.timedOut must be boolean');
    if (payload.streams !== null) {
      const streams = objectWhenPresent('streams');
      if (streams === undefined) fail('campaign_process payload.streams must be an object or null');
      for (const [name, stream] of Object.entries(streams)) {
        if (!['stdout', 'stderr'].includes(name) || !isObject(stream)) {
          fail(`campaign_process payload.streams.${name} is invalid`);
        }
        const allowed = new Set(['path', 'sha256', 'bytes', 'retainedBytes', 'truncated']);
        for (const key of Object.keys(stream)) {
          if (!allowed.has(key)) fail(`campaign_process payload.streams.${name}.${key} is unknown`);
        }
        if (stream.path !== `process.${name}.log`) fail(`campaign_process payload.streams.${name}.path is invalid`);
        if (!isHash(stream.sha256)) fail(`campaign_process payload.streams.${name}.sha256 is invalid`);
        const bytes = stream.bytes;
        const retainedBytes = stream.retainedBytes;
        if (!isSafeInteger(bytes) || bytes < 0 || !isSafeInteger(retainedBytes)
          || retainedBytes < 0 || retainedBytes > bytes) {
          fail(`campaign_process payload.streams.${name} byte counts are invalid`);
        }
        if (stream.truncated !== (bytes > retainedBytes)) {
          fail(`campaign_process payload.streams.${name}.truncated is inconsistent`);
        }
      }
    }
  }
  if (kind === 'repair_process') {
    if (payload.schemaVersion !== 1) fail('repair_process payload.schemaVersion must be 1');
    if (typeof payload.parentRunId !== 'string' || !payload.parentRunId) {
      fail('repair_process payload.parentRunId is required');
    }
    for (const field of ['level', 'roundsGranted']) {
      if (!isSafeInteger(payload[field]) || payload[field] < 1) {
        fail(`repair_process payload.${field} must be a positive integer`);
      }
    }
    if (payload.exitCode !== null && !isInteger(payload.exitCode)) {
      fail('repair_process payload.exitCode must be an integer or null');
    }
    if (payload.signal !== null && typeof payload.signal !== 'string') {
      fail('repair_process payload.signal must be a string or null');
    }
    if (typeof payload.timedOut !== 'boolean') fail('repair_process payload.timedOut must be boolean');
    if (payload.streams !== null && !isObject(payload.streams)) {
      fail('repair_process payload.streams must be an object or null');
    }
    const repairStreams = payload.streams === null ? {} : payload.streams;
    if (!isObject(repairStreams)) fail('repair_process payload.streams must be an object or null');
    for (const [name, stream] of Object.entries(repairStreams)) {
      if (!['stdout', 'stderr'].includes(name) || !isObject(stream)) {
        fail(`repair_process payload.streams.${name} is invalid`);
      }
      const allowed = new Set(['path', 'sha256', 'bytes', 'retainedBytes', 'truncated']);
      for (const key of Object.keys(stream)) {
        if (!allowed.has(key)) fail(`repair_process payload.streams.${name}.${key} is unknown`);
      }
      const bytes = stream.bytes;
      const retainedBytes = stream.retainedBytes;
      if (stream.path !== `process.${name}.log` || !isHash(stream.sha256)
        || !isSafeInteger(bytes) || bytes < 0
        || !isSafeInteger(retainedBytes) || retainedBytes < 0
        || retainedBytes > bytes || stream.truncated !== (bytes > retainedBytes)) {
        fail(`repair_process payload.streams.${name} is inconsistent`);
      }
    }
  }
  const validateGradeFeatures = (features: unknown, at: string): void => {
    if (!Array.isArray(features)) return;
    features.forEach((feature, featureIndex) => {
      if (!isObject(feature)) fail(`${at}[${featureIndex}] must be an object`);
      if (feature.setupEvidence === undefined) fail(`${at}[${featureIndex}].setupEvidence is required`);
      try { validateCheckEvidence(feature.setupEvidence,
        { at: `${at}[${featureIndex}].setupEvidence` }); }
      catch (error) { fail(errorMessage(error)); }
      const setupEvidence = asObject(feature.setupEvidence, `${at}[${featureIndex}].setupEvidence must be an object`);
      if (setupEvidence.phase !== 'setup') {
        fail(`${at}[${featureIndex}].setupEvidence must use setup phase`);
      }
      if (feature.criteria !== undefined && !Array.isArray(feature.criteria)) {
        fail(`${at}[${featureIndex}].criteria must be an array when present`);
      }
      const criteria = feature.criteria ?? [];
      if (!Array.isArray(criteria)) fail(`${at}[${featureIndex}].criteria must be an array when present`);
      criteria.forEach((criterion, criterionIndex) => {
        const criterionAt = `${at}[${featureIndex}].criteria[${criterionIndex}]`;
        if (!isObject(criterion)) fail(`${criterionAt} must be an object`);
        for (const obsolete of ['passed', 'inconclusive', 'detail']) {
          if (Object.hasOwn(criterion, obsolete)) fail(`${criterionAt}.${obsolete} is obsolete; use evidence`);
        }
        if (criterion.evidence === undefined) fail(`${criterionAt}.evidence is required`);
        try { validateCheckEvidence(criterion.evidence, { at: `${criterionAt}.evidence` }); }
        catch (error) { fail(errorMessage(error)); }
      });
    });
  };
  if (kind === 'grade') {
    return validateGradePayload(payload);
  }
  if (kind === 'grade_bundle') {
    objectWhenPresent('suites'); objectWhenPresent('totals');
    if (payload.outcome !== undefined) validateRunOutcome(payload.outcome,
      'grade_bundle payload.outcome');
    const observation = payload.observation ?? 'scored';
    if (!['scored', 'observed'].includes(String(observation))) {
      fail('grade_bundle payload.observation must be scored or observed');
    }
    if (payload.source !== undefined
      && (!isObject(payload.source) || Object.keys(payload.source).length !== 1
        || !isHash(payload.source.sha256))) {
      fail('grade_bundle payload.source must contain its application SHA-256');
    }
    if (observation === 'observed') {
      if (payload.source === undefined) {
        fail('grade_bundle observed payload.source must contain its first-build SHA-256');
      }
      const selection = asObject(payload.selection,
        'grade_bundle observed selection must be diagnostic and contribute zero score');
      if (selection.observation !== 'observed' || selection.scoredPoints !== 0
        || !isSafeInteger(selection.observedPoints) || selection.observedPoints < 0) {
        fail('grade_bundle observed selection must be diagnostic and contribute zero score');
      }
    }
    const suites = payload.suites ?? {};
    if (!isObject(suites)) fail('grade_bundle payload.suites must be an object when present');
    for (const [suiteId, suite] of Object.entries(suites)) {
      if (isObject(suite)) validateGradeFeatures(suite.features, `grade_bundle payload.suites.${suiteId}.features`);
    }
  }
  if (kind === 'mutation_control') {
    arrayWhenPresent('results');
    if (payload.outcome !== undefined) validateRunOutcome(payload.outcome,
      'mutation_control payload.outcome');
    if (payload.shard !== undefined) {
      if (!isObject(payload.shard)) fail('mutation_control payload.shard must be an object');
      const allowed = new Set(['index', 'count', 'mutationIds']);
      for (const key of Object.keys(payload.shard)) {
        if (!allowed.has(key)) fail(`mutation_control payload.shard.${key} is unknown`);
      }
      const { index, count, mutationIds } = payload.shard;
      if (!isSafeInteger(count) || count < 1
        || !isSafeInteger(index) || index < 0 || index >= count) {
        fail('mutation_control payload.shard coordinates are invalid');
      }
      if (!Array.isArray(mutationIds) || mutationIds.length === 0
        || mutationIds.some(id => typeof id !== 'string' || !id)
        || new Set(mutationIds).size !== mutationIds.length) {
        fail('mutation_control payload.shard.mutationIds must contain unique non-empty strings');
      }
      const results = payload.results ?? [];
      if (!Array.isArray(results)) fail('mutation_control payload.results must be an array when present');
      const resultIds = results.map(result => isObject(result) ? result.id : undefined);
      if (resultIds.some(id => typeof id !== 'string' || !id)
        || new Set(resultIds).size !== resultIds.length
        || resultIds.some(id => !mutationIds.includes(id))) {
        fail('mutation_control payload.results must be a unique subset of the assigned mutation IDs');
      }
      const checkpoint = payload.checkpoint;
      const checkpointStatus = isObject(checkpoint) ? checkpoint.status : undefined;
      const partial = checkpointStatus === 'running' || checkpointStatus === 'incomplete';
      if (!partial && resultIds.length !== mutationIds.length) {
        fail('complete mutation_control payload.shard.mutationIds must match the exact result set');
      }
    }
  }
  if (kind === 'null_control') {
    arrayWhenPresent('criteria'); objectWhenPresent('qualificationScope'); runnerWhenPresent();
  }
  if (kind === 'pack_budget_measurement') {
    objectWhenPresent('policy'); arrayWhenPresent('evidence'); arrayWhenPresent('samples');
    arrayWhenPresent('recommendations'); runnerWhenPresent();
  }
  if (kind === 'performance_run') { objectWhenPresent('deliveryLatencyMs'); objectWhenPresent('server'); }
  if (kind === 'reference_build') arrayWhenPresent('fixtures');
  if (kind === 'reference_qualification') {
    for (const field of ['fixture', 'requiredRepetitions', 'runs']) {
      if (!Object.hasOwn(payload, field)) fail(`reference_qualification payload.${field} is required`);
    }
    if (typeof payload.fixture !== 'string' || !payload.fixture) {
      fail('reference_qualification payload.fixture must be a non-empty string');
    }
    if (!isSafeInteger(payload.requiredRepetitions) || payload.requiredRepetitions < 1) {
      fail('reference_qualification payload.requiredRepetitions must be a positive integer');
    }
    arrayWhenPresent('runs'); objectWhenPresent('qualificationScope'); runnerWhenPresent();
  }
  if (kind === 'recovery') {
    if (payload.schemaVersion !== 1) fail('recovery payload.schemaVersion must be 1');
    if (!['clean', 'retained', 'quarantined'].includes(String(payload.status))) {
      fail('recovery payload.status is invalid');
    }
    for (const field of ['runId', 'backend']) {
      if (typeof payload[field] !== 'string' || !payload[field]) {
        fail(`recovery payload.${field} must be a non-empty string`);
      }
    }
    if (payload.reason !== null && typeof payload.reason !== 'string') {
      fail('recovery payload.reason must be a string or null');
    }
    const cleanup = objectWhenPresent('cleanup');
    if (!cleanup || typeof cleanup.succeeded !== 'boolean' || typeof cleanup.retained !== 'boolean') {
      fail('recovery payload.cleanup is invalid');
    }
    const resources = objectWhenPresent('resources');
    if (!resources || typeof resources.backendState !== 'string'
      || !Array.isArray(resources.listenerProcesses) || !Array.isArray(resources.locks)) {
      fail('recovery payload.resources is invalid');
    }
    for (const process of resources.listenerProcesses) {
      if (!isObject(process) || !isSafeInteger(process.pid) || process.pid <= 0
        || typeof process.startMarker !== 'string' || !/^\d+$/.test(process.startMarker)) {
        fail('recovery payload.resources.listenerProcesses is invalid');
      }
    }
    const instructions = arrayWhenPresent('instructions');
    if (!instructions || instructions.some(item => typeof item !== 'string')) {
      fail('recovery payload.instructions is invalid');
    }
    if (payload.status === 'clean' && (!cleanup.succeeded || cleanup.retained)) {
      fail('recovery clean status conflicts with cleanup');
    }
  }
  if (kind === 'source_checkpoint') {
    if (payload.schemaVersion !== 2) fail('source_checkpoint payload.schemaVersion must be 2');
    for (const field of ['track', 'backend']) {
      if (typeof payload[field] !== 'string' || !payload[field]) {
        fail(`source_checkpoint payload.${field} must be a non-empty string`);
      }
    }
    if (!isSafeInteger(payload.level) || payload.level < 1) {
      fail('source_checkpoint payload.level must be a positive integer');
    }
    const source = objectWhenPresent('source');
    if (source === undefined) fail('source_checkpoint payload.source is required');
    const sourceFields = new Set(['directory', 'sha256', 'files']);
    for (const key of Object.keys(source)) {
      if (!sourceFields.has(key)) fail(`source_checkpoint payload.source.${key} is unknown`);
    }
    if (source.directory !== `level-l${payload.level}-source`) {
      fail('source_checkpoint payload.source.directory does not match its level');
    }
    if (!isHash(source.sha256)) {
      fail('source_checkpoint payload.source.sha256 is invalid');
    }
    if (!isSafeInteger(source.files) || source.files < 0) {
      fail('source_checkpoint payload.source.files must be a non-negative integer');
    }
    const repair = objectWhenPresent('repair');
    if (repair === undefined) fail('source_checkpoint payload.repair is required');
    const repairFields = new Set(['status', 'budgetRounds', 'roundsUsed', 'stallLimitRounds',
      'stopReason', 'strikeScope', 'nodeStrikes']);
    for (const key of Object.keys(repair)) {
      if (!repairFields.has(key)) fail(`source_checkpoint payload.repair.${key} is unknown`);
    }
    if (!['ungraded', 'not-needed', 'corrected', 'budget-exhausted', 'incomplete']
      .includes(String(repair.status))) fail('source_checkpoint payload.repair.status is invalid');
    for (const field of ['budgetRounds', 'roundsUsed']) {
      if (!isSafeInteger(repair[field]) || repair[field] < 0) {
        fail(`source_checkpoint payload.repair.${field} must be a non-negative integer`);
      }
    }
    if (repair.stallLimitRounds !== undefined
      && (!isSafeInteger(repair.stallLimitRounds) || repair.stallLimitRounds < 0)) {
      fail('source_checkpoint payload.repair.stallLimitRounds must be a non-negative integer');
    }
    if (!isSafeInteger(repair.roundsUsed) || !isSafeInteger(repair.budgetRounds)) {
      fail('source_checkpoint payload.repair rounds must be non-negative integers');
    }
    if (repair.roundsUsed > repair.budgetRounds) {
      fail('source_checkpoint payload.repair.roundsUsed exceeds its budget');
    }
    if (repair.strikeScope !== undefined || repair.nodeStrikes !== undefined) {
      if (repair.strikeScope !== 'feature' || !Array.isArray(repair.nodeStrikes)) {
        fail('source_checkpoint payload.repair feature strikes are invalid');
      }
      const ids = new Set<string>();
      let prior: string | null = null;
      for (const [index, counter] of repair.nodeStrikes.entries()) {
        if (!isObject(counter)) fail(`source_checkpoint payload.repair.nodeStrikes[${index}] is invalid`);
        const fields = new Set(['nodeId', 'initialBudget', 'granted', 'budget', 'used',
          'remaining', 'exhaustionReason']);
        for (const key of Object.keys(counter)) {
          if (!fields.has(key)) fail(`source_checkpoint payload.repair.nodeStrikes[${index}].${key} is unknown`);
        }
        if (typeof counter.nodeId !== 'string' || !counter.nodeId || ids.has(counter.nodeId)
          || (prior !== null && prior.localeCompare(counter.nodeId) >= 0)
          || !isSafeInteger(counter.initialBudget) || counter.initialBudget < 1
          || !isSafeInteger(counter.granted) || counter.granted < 0
          || !isSafeInteger(counter.budget) || counter.budget < 1
          || counter.budget !== counter.initialBudget + counter.granted
          || !isSafeInteger(counter.used) || counter.used < 0
          || counter.used > counter.budget
          || counter.remaining !== counter.budget - counter.used
          || (counter.exhaustionReason !== null
            && (typeof counter.exhaustionReason !== 'string'
              || !['strikes-exhausted', 'repeated-findings'].includes(counter.exhaustionReason)))) {
          fail(`source_checkpoint payload.repair.nodeStrikes[${index}] is invalid`);
        }
        ids.add(counter.nodeId);
        prior = counter.nodeId;
      }
    }
    if (repair.stopReason !== null && typeof repair.stopReason !== 'string') {
      fail('source_checkpoint payload.repair.stopReason must be a string or null');
    }
    validateRunOutcome(payload.outcome, 'source_checkpoint payload.outcome');
    if (payload.selectionSha256 !== null && !isHash(payload.selectionSha256)) {
      fail('source_checkpoint payload.selectionSha256 is invalid');
    }
  }
  if (kind === 'contract_lint') { arrayWhenPresent('results'); objectWhenPresent('counts'); }
  if (kind === 'action_check') { arrayWhenPresent('results'); arrayWhenPresent('missing'); }
  return payload;
}

function rejectSecrets(value: unknown, at: string = '$', seen: Set<object> = new Set()): void {
  if (value === null || typeof value !== 'object') return;
  if (seen.has(value)) fail(`${at} contains a cycle`);
  seen.add(value);
  if (Array.isArray(value)) value.forEach((item, index) => rejectSecrets(item, `${at}[${index}]`, seen));
  else {
    for (const [key, item] of Object.entries(value)) {
      const normalizedKey = key.toLowerCase().replaceAll(/[^a-z0-9]/g, '');
      if (SECRET_KEYS.has(normalizedKey)) fail(`${at}.${key} is secret-bearing and cannot be public`);
      rejectSecrets(item, `${at}.${key}`, seen);
    }
  }
  seen.delete(value);
}

export function createArtifact<TPayload extends object>(
  input: CreateArtifactInput & { payload: TPayload },
): Artifact<TPayload>;
export function createArtifact(input: CreateArtifactInput): Artifact;
export function createArtifact(input: CreateArtifactInput): Artifact {
  const { kind, id, attempt = null, timestamps = null, identities = null, payload = {} } = input;
  const normalizedKind = normalizeKind(kind);
  if (typeof id !== 'string' || !id) fail('id must be a non-empty string');
  const now = new Date().toISOString();
  const normalizedTimestamps = timestamps === null
    ? {} : asObject(timestamps, 'timestamps must be an object or null');
  const startedAt = timestamp(normalizedTimestamps.startedAt ?? now, 'timestamps.startedAt');
  const completedAt = normalizedTimestamps.completedAt == null
    ? null : timestamp(normalizedTimestamps.completedAt, 'timestamps.completedAt');
  if (completedAt && Date.parse(completedAt) < Date.parse(startedAt)) {
    fail('timestamps.completedAt precedes timestamps.startedAt');
  }
  const normalizedAttempt = attempt === null
    ? { id, parentId: null } : asObject(attempt, 'attempt must be an object or null');
  if (typeof normalizedAttempt.id !== 'string' || !normalizedAttempt.id) {
    fail('attempt.id must be a non-empty string');
  }
  if (normalizedAttempt.parentId !== null && normalizedAttempt.parentId !== undefined
    && (typeof normalizedAttempt.parentId !== 'string' || !normalizedAttempt.parentId)) {
    fail('attempt.parentId must be a non-empty string or null');
  }
  const identityInput = identities === null
    ? {} : asObject(identities, 'identities must be an object or null');
  const identityDefaults: UnknownRecord = { packs: [], ...identityInput };
  if (!Object.hasOwn(identityInput, 'engine')) identityDefaults.engine = currentEngineIdentity();
  const artifact: Artifact = {
    artifactSchemaVersion: ARTIFACT_SCHEMA_VERSION,
    kind: normalizedKind,
    id,
    attempt: { id: normalizedAttempt.id, parentId: normalizedAttempt.parentId ?? null },
    timestamps: { startedAt, completedAt },
    identities: validateArtifactIdentities(identityDefaults,
      { requireEngine: true }),
    payload: validatePayload(normalizedKind, structuredClone(payload)),
  };
  rejectSecrets(artifact);
  return artifact;
}

export function validateArtifact<TPayload extends object = UnknownRecord>(
  input: unknown,
  options?: { source?: string },
): Artifact<TPayload>;
export function validateArtifact(
  input: unknown,
  { source = '<artifact>' }: { source?: string } = {},
): Artifact {
  if (!isObject(input)) fail(`${source} must be an object`);
  if (input.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION) {
    fail(`${source} uses unsupported schema ${input.artifactSchemaVersion ?? 'missing'}`);
  }
  if (!isObject(input.attempt) || !Object.hasOwn(input.attempt, 'id')
    || !Object.hasOwn(input.attempt, 'parentId')) fail(`${source}.attempt is incomplete`);
  if (!isObject(input.timestamps) || !Object.hasOwn(input.timestamps, 'startedAt')
    || !Object.hasOwn(input.timestamps, 'completedAt')) fail(`${source}.timestamps is incomplete`);
  if (!Object.hasOwn(input, 'identities')) fail(`${source}.identities is required`);
  if (!isObject(input.identities)) fail(`${source}.identities must be an object`);
  for (const key of [...IDENTITY_KEYS, 'packs']) {
    if (!Object.hasOwn(input.identities, key)) fail(`${source}.identities.${key} is required`);
  }
  const parsed = artifactEnvelopeSchema.safeParse(structuredClone(input));
  if (!parsed.success) {
    fail(formatZodError(parsed.error, source));
  }
  const candidate = parsed.data;
  const { startedAt, completedAt } = candidate.timestamps;
  if (completedAt && Date.parse(completedAt) < Date.parse(startedAt)) {
    fail(`${source}.timestamps.completedAt precedes timestamps.startedAt`);
  }
  const artifact: Artifact = {
    artifactSchemaVersion: ARTIFACT_SCHEMA_VERSION,
    kind: candidate.kind,
    id: candidate.id,
    attempt: candidate.attempt,
    timestamps: { startedAt, completedAt },
    identities: validateArtifactIdentities(candidate.identities,
      { requireEngine: true, requireComplete: true, sortPacks: false }),
    payload: validatePayload(candidate.kind, structuredClone(candidate.payload)),
  };
  rejectSecrets(artifact);
  return artifact;
}

export interface GradeArtifactCriterion {
  id: string;
  desc: string;
  points: number;
  stableKey: string | null;
  evidence: CheckEvidence;
  serverCheck?: string;
}

export interface GradeArtifactFeature {
  id: number;
  name: string;
  score: number;
  setupEvidence: CheckEvidence;
  criteria: GradeArtifactCriterion[];
  max: number;
}

export interface GradeArtifactSelection {
  sha256: string;
  checks: Array<{ stableKey: string; points: number }>;
}

export interface GradeArtifactPayload extends UnknownRecord {
  total: number;
  max: number;
  features: GradeArtifactFeature[];
  selection: GradeArtifactSelection | null;
  inconclusive: Array<{ points: number }>;
}

export function validateGradePayload(payload: UnknownRecord): GradeArtifactPayload {
  const number = (value: unknown, at: string): number => {
    if (!isFiniteNumber(value) || value < 0) fail(`${at} must be a non-negative number`);
    return value;
  };
  const selectionValue = payload.selection;
  const selection = selectionValue === undefined || selectionValue === null ? null : (() => {
    const value = asObject(selectionValue, 'grade payload.selection must be an object');
    if (typeof value.sha256 !== 'string' || !value.sha256 || !Array.isArray(value.checks)) {
      fail('grade payload.selection is invalid');
    }
    return { ...value, sha256: value.sha256, checks: value.checks.map((check, index) => {
      const entry = asObject(check, `grade payload.selection.checks[${index}] must be an object`);
      if (typeof entry.stableKey !== 'string' || !entry.stableKey) {
        fail(`grade payload.selection.checks[${index}].stableKey is invalid`);
      }
      return { ...entry, stableKey: entry.stableKey,
        points: number(entry.points, `grade payload.selection.checks[${index}].points`) };
    }) };
  })();
  if (!Array.isArray(payload.features)) fail('grade payload.features must be an array');
  const features = payload.features.map((feature, featureIndex) => {
    const value = asObject(feature, `grade payload.features[${featureIndex}] must be an object`);
    if (!Array.isArray(value.criteria)) fail(`grade payload.features[${featureIndex}].criteria must be an array`);
    const setupEvidence = validateCheckEvidence(value.setupEvidence,
      { at: `grade payload.features[${featureIndex}].setupEvidence` });
    if (setupEvidence.phase !== 'setup') {
      fail(`grade payload.features[${featureIndex}].setupEvidence must use setup phase`);
    }
    if (!Number.isSafeInteger(value.id) || Number(value.id) < 1
      || typeof value.name !== 'string' || !value.name) {
      fail(`grade payload.features[${featureIndex}] identity is invalid`);
    }
    return { ...value, id: Number(value.id), name: value.name,
    score: number(value.score, `grade payload.features[${featureIndex}].score`), setupEvidence,
    criteria: value.criteria.map((criterion, criterionIndex) => {
      const entry = asObject(criterion,
        `grade payload.features[${featureIndex}].criteria[${criterionIndex}] must be an object`);
      for (const obsolete of ['passed', 'inconclusive', 'detail']) {
        if (Object.hasOwn(entry, obsolete)) {
          fail(`grade payload.features[${featureIndex}].criteria[${criterionIndex}].${obsolete} is obsolete; use evidence`);
        }
      }
      if (entry.evidence === undefined) {
        fail(`grade payload.features[${featureIndex}].criteria[${criterionIndex}].evidence is required`);
      }
      if (entry.stableKey !== undefined && entry.stableKey !== null
        && (typeof entry.stableKey !== 'string' || !entry.stableKey)) {
        fail(`grade payload.features[${featureIndex}].criteria[${criterionIndex}].stableKey is invalid`);
      }
      if (typeof entry.id !== 'string' || !entry.id
        || typeof entry.desc !== 'string' || !entry.desc
        || (entry.serverCheck !== undefined && typeof entry.serverCheck !== 'string')) {
        fail(`grade payload.features[${featureIndex}].criteria[${criterionIndex}] identity is invalid`);
      }
      return { ...entry, id: entry.id, desc: entry.desc,
        points: number(entry.points,
          `grade payload.features[${featureIndex}].criteria[${criterionIndex}].points`),
        stableKey: entry.stableKey ?? null, evidence: validateCheckEvidence(entry.evidence,
          { at: `grade payload.features[${featureIndex}].criteria[${criterionIndex}].evidence` }),
        ...(entry.serverCheck === undefined ? {} : { serverCheck: entry.serverCheck }) };
    }), max: number(value.max, `grade payload.features[${featureIndex}].max`) };
  });
  const inconclusive = payload.inconclusive === undefined ? [] : (() => {
    if (!Array.isArray(payload.inconclusive)) fail('grade payload.inconclusive must be an array');
    return payload.inconclusive.map((entry, index) => {
      const value = asObject(entry, `grade payload.inconclusive[${index}] must be an object`);
      return { points: number(value.points, `grade payload.inconclusive[${index}].points`) };
    });
  })();
  return { ...payload, total: number(payload.total, 'grade payload.total'),
    max: number(payload.max, 'grade payload.max'), features, selection, inconclusive };
}
