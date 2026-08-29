import { randomUUID } from 'node:crypto';
import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';


import { hashDirectory } from './provenance.js';
import { validateCheckEvidence } from './check-evidence.js';
import { RUNNER_OBSERVATION_FIELDS } from '../runtime/runner-environment.mjs';
import type { CostRun } from './cost-proof.js';

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

export interface ArtifactIdentity {
  id: string;
  version: string | null;
  sha256: string | null;
  state: string | null;
}

export interface EngineArtifactIdentity extends ArtifactIdentity {
  id: 'stack-bench';
  version: null;
  sha256: string;
  state: null;
}

type ArtifactIdentityKey = typeof IDENTITY_KEYS[number];

export type ArtifactIdentities = Record<ArtifactIdentityKey, ArtifactIdentity | null> & {
  packs: ArtifactIdentity[];
};

export interface Artifact<TPayload extends object = UnknownRecord> {
  artifactSchemaVersion: 2;
  kind: ArtifactKind;
  id: string;
  attempt: { id: string; parentId: string | null };
  timestamps: { startedAt: string; completedAt: string | null };
  identities: ArtifactIdentities;
  payload: TPayload;
  [key: string]: unknown;
}

export interface CreateArtifactInput {
  kind: unknown;
  id: string;
  attempt?: unknown;
  timestamps?: unknown;
  identities?: unknown;
  payload?: unknown;
}

interface ArtifactReadOptions {
  expectedId?: string | null;
  expectedKind?: ArtifactKind | null;
}

const IDENTITY_KEYS = Object.freeze([
  'engine', 'recipe', 'fixture', 'calibration', 'experiment', 'agentAdapter', 'stackAdapter',
] as const);
const KIND_SET = new Set<string>(ARTIFACT_KINDS);
const SECRET_KEYS = new Set(['apikey', 'leasetoken', 'ownershiptoken', 'password', 'secret']);
const ENVELOPE_KEYS = new Set([
  'artifactSchemaVersion', 'kind', 'id', 'attempt', 'timestamps', 'identities', 'payload',
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
    'events', 'snapshot', 'resume', 'snapshotSha256']),
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
const HASH = /^[a-f0-9]{64}$/;
const ISO = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/;
import { STACK_BENCH_ROOT as ROOT } from '../package-root.js';
let cachedEngineIdentity: EngineArtifactIdentity | null = null;

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

function identity(value: unknown, at: string): ArtifactIdentity | null {
  if (value === null) return null;
  const candidate = asObject(value, `${at} must be an object or null`);
  const allowed = new Set(['id', 'version', 'sha256', 'state']);
  for (const key of Object.keys(candidate)) if (!allowed.has(key)) fail(`${at}.${key} is unknown`);
  if (typeof candidate.id !== 'string' || !candidate.id) fail(`${at}.id must be a non-empty string`);
  if (candidate.version !== null && candidate.version !== undefined
    && (typeof candidate.version !== 'string' || !candidate.version)) fail(`${at}.version is invalid`);
  if (candidate.sha256 !== null && candidate.sha256 !== undefined && !isHash(candidate.sha256)) {
    fail(`${at}.sha256 must be 64 lowercase hexadecimal characters`);
  }
  if (candidate.state !== null && candidate.state !== undefined
    && (typeof candidate.state !== 'string' || !candidate.state)) fail(`${at}.state is invalid`);
  return {
    id: candidate.id,
    version: candidate.version ?? null,
    sha256: candidate.sha256 ?? null,
    state: candidate.state ?? null,
  };
}

function validateIdentities(
  value: unknown,
  { requireEngine = false }: { requireEngine?: boolean } = {},
): ArtifactIdentities {
  const candidate = asObject(value, 'identities must be an object');
  const allowed = new Set([...IDENTITY_KEYS, 'packs']);
  for (const key of Object.keys(candidate)) if (!allowed.has(key)) fail(`identities.${key} is unknown`);
  const packsValue = candidate.packs ?? [];
  if (!Array.isArray(packsValue)) fail('identities.packs must be an array');
  const normalized: ArtifactIdentities = {
    engine: identity(candidate.engine ?? null, 'identities.engine'),
    recipe: identity(candidate.recipe ?? null, 'identities.recipe'),
    fixture: identity(candidate.fixture ?? null, 'identities.fixture'),
    calibration: identity(candidate.calibration ?? null, 'identities.calibration'),
    experiment: identity(candidate.experiment ?? null, 'identities.experiment'),
    agentAdapter: identity(candidate.agentAdapter ?? null, 'identities.agentAdapter'),
    stackAdapter: identity(candidate.stackAdapter ?? null, 'identities.stackAdapter'),
    packs: packsValue.map((item, index) => {
      const pack = identity(item, `identities.packs[${index}]`);
      if (pack === null) fail(`identities.packs[${index}] must not be null`);
      return pack;
    }),
  };
  if (requireEngine && normalized.engine === null) fail('identities.engine is required');
  const packIds = new Set<string>();
  for (const pack of normalized.packs) {
    const key = `${pack.id}@${pack.version ?? ''}:${pack.sha256 ?? ''}`;
    if (packIds.has(key)) fail(`identities.packs duplicates ${key}`);
    packIds.add(key);
  }
  normalized.packs.sort((a, b) => `${a.id}@${a.version ?? ''}`.localeCompare(`${b.id}@${b.version ?? ''}`));
  return normalized;
}

function validatePayload(kind: ArtifactKind, input: unknown): UnknownRecord {
  const payload = asObject(input, 'payload must be an object');
  for (const key of Object.keys(payload)) {
    if (!PAYLOAD_FIELDS[kind].has(key)) fail(`${kind} payload.${key} is unknown`);
  }
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
  if (['benchmark_run', 'repair_continuation'].includes(kind)) arrayWhenPresent('levels');
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
    if (progressionStatus.stateArtifact !== 'progression-state.json') {
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
    const fields = new Set(['priorRunId', 'priorRunSha256', 'stateSnapshotSha256',
      'action', 'inheritedLevels', 'priorTotals']);
    for (const key of Object.keys(progressionResume)) {
      if (!fields.has(key)) fail(`benchmark_run payload.progressionResume.${key} is unknown`);
    }
    if (typeof progressionResume.priorRunId !== 'string' || !progressionResume.priorRunId) {
      fail('benchmark_run payload.progressionResume.priorRunId is invalid');
    }
    for (const field of ['priorRunSha256', 'stateSnapshotSha256']) {
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
    if (payload.schemaVersion !== 2) fail('progression_state payload.schemaVersion must be 2');
    objectWhenPresent('owner');
    objectWhenPresent('featureCatalog');
    objectWhenPresent('dependencyPolicy');
    objectWhenPresent('snapshot');
    arrayWhenPresent('events');
    if (!isHash(payload.snapshotSha256)) {
      fail('progression_state payload.snapshotSha256 is invalid');
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
        || !sourceValid || !isObject(baseline.outcome)
        || typeof baseline.outcome.kind !== 'string' || typeof baseline.reproduced !== 'boolean'
        || !Array.isArray(baseline.mismatches)
        || baseline.mismatches.some(item => typeof item !== 'string' || !item)
        || (baseline.reproduced && (baseline.sourceSha256 === null || baseline.score === null))) {
        fail('repair_continuation payload.continuation.baseline is invalid');
      }
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
    arrayWhenPresent('features');
    validateGradeFeatures(payload.features, 'grade payload.features');
  }
  if (kind === 'grade_bundle') {
    objectWhenPresent('suites'); objectWhenPresent('totals');
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
    arrayWhenPresent('runs'); objectWhenPresent('qualificationScope'); runnerWhenPresent();
  }
  if (kind === 'recovery') { objectWhenPresent('cleanup'); objectWhenPresent('resources');
    arrayWhenPresent('instructions'); }
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
    objectWhenPresent('outcome');
    if (!isObject(payload.outcome) || typeof payload.outcome.kind !== 'string'
      || !payload.outcome.kind) fail('source_checkpoint payload.outcome.kind is required');
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
      if (SECRET_KEYS.has(key.toLowerCase())) fail(`${at}.${key} is secret-bearing and cannot be public`);
      rejectSecrets(item, `${at}.${key}`, seen);
    }
  }
  seen.delete(value);
}

export function emptyArtifactIdentities(overrides: UnknownRecord = {}): ArtifactIdentities {
  return validateIdentities({ engine: currentEngineIdentity(), packs: [], ...overrides },
    { requireEngine: true });
}

export function currentEngineIdentity(): EngineArtifactIdentity {
  if (cachedEngineIdentity) return structuredClone(cachedEngineIdentity);
  const excludedRoots = new Set([
    'archive',
    'local-notes',
    'media',
    'qualification-evidence',
    'reference-apps',
    'results',
    'tests',
    'tracks',
    'transcripts',
  ]);
  const executable = hashDirectory(ROOT, { exclude: (name, entry) => {
    const parts = name.split('/');
    if (parts.some(part => part.startsWith('.')) || excludedRoots.has(parts[0] ?? '')
      || parts.includes('node_modules')
    ) return true;
    if (entry.isDirectory()) return false;
    // Generated while assembling the controller image. It describes the packaged
    // dependency volume; it is not executable harness source and does not exist
    // in a source checkout.
    if (name === 'dependency-manifest.json') return true;
    return !(/\.(?:mjs|js|json|ya?ml|sh)$/.test(name) || /(?:^|\/)Dockerfile$/.test(name));
  } });
  cachedEngineIdentity = { id: 'stack-bench', version: null, sha256: executable.sha256, state: null };
  return structuredClone(cachedEngineIdentity);
}

export function recipeArtifactIdentities(
  recipeRelease: unknown,
  overrides: UnknownRecord = {},
): ArtifactIdentities {
  if (!recipeRelease) return emptyArtifactIdentities(overrides);
  const release = asObject(recipeRelease, 'recipe release must be an object');
  const components = release.components === undefined
    ? {} : asObject(release.components, 'recipe release components must be an object');
  const fixture = components.fixture == null
    ? null : asObject(components.fixture, 'recipe release fixture must be an object');
  const packsValue = components.packs ?? [];
  if (!Array.isArray(packsValue)) fail('recipe release packs must be an array');
  return emptyArtifactIdentities({
    recipe: { id: release.id, version: release.version,
      sha256: release.contentSha256, state: release.state },
    fixture: fixture ? {
      id: fixture.id,
      version: fixture.version,
      sha256: fixture.sha256 ?? null,
      state: fixture.state,
    } : null,
    packs: packsValue.map((value, index) => {
      const pack = asObject(value, `recipe release packs[${index}] must be an object`);
      return {
      id: pack.id, version: pack.version, sha256: pack.sha256 ?? null, state: pack.state,
      };
    }),
    ...overrides,
  });
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
  const artifact: Artifact = {
    artifactSchemaVersion: ARTIFACT_SCHEMA_VERSION,
    kind: normalizedKind,
    id,
    attempt: { id: normalizedAttempt.id, parentId: normalizedAttempt.parentId ?? null },
    timestamps: { startedAt, completedAt },
    identities: validateIdentities({ engine: currentEngineIdentity(), packs: [], ...identityInput },
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
  const candidate = asObject(input, `${source} must be an object`);
  if (candidate.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION) {
    fail(`${source} uses unsupported schema ${candidate.artifactSchemaVersion ?? 'missing'}`);
  }
  for (const key of Object.keys(candidate)) {
    if (!ENVELOPE_KEYS.has(key)) fail(`${source}.${key} is unknown`);
  }
  for (const key of ENVELOPE_KEYS) {
    if (!Object.hasOwn(candidate, key)) fail(`${source}.${key} is required`);
  }
  if (!isObject(candidate.attempt) || !Object.hasOwn(candidate.attempt, 'id')
    || !Object.hasOwn(candidate.attempt, 'parentId')) fail(`${source}.attempt is incomplete`);
  if (!isObject(candidate.timestamps) || !Object.hasOwn(candidate.timestamps, 'startedAt')
    || !Object.hasOwn(candidate.timestamps, 'completedAt')) fail(`${source}.timestamps is incomplete`);
  if (!isObject(candidate.identities)) fail(`${source}.identities must be an object`);
  for (const key of [...IDENTITY_KEYS, 'packs']) {
    if (!Object.hasOwn(candidate.identities, key)) fail(`${source}.identities.${key} is required`);
  }
  if (typeof candidate.id !== 'string') fail(`${source}.id must be a string`);
  return createArtifact({ kind: candidate.kind, id: candidate.id, attempt: candidate.attempt,
    timestamps: candidate.timestamps, identities: candidate.identities, payload: candidate.payload });
}

export function writeArtifact(path: string, input: unknown): Artifact {
  const candidate = asObject(input, `${path} must be an object`);
  if (candidate.artifactSchemaVersion !== undefined
    && candidate.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION) {
    fail(`${path} uses unsupported schema ${candidate.artifactSchemaVersion}`);
  }
  let artifact: Artifact;
  if (candidate.artifactSchemaVersion === ARTIFACT_SCHEMA_VERSION) {
    artifact = validateArtifact(candidate, { source: path });
  } else {
    if (typeof candidate.id !== 'string') fail(`${path}.id must be a string`);
    artifact = createArtifact({ kind: candidate.kind, id: candidate.id, attempt: candidate.attempt,
      timestamps: candidate.timestamps, identities: candidate.identities, payload: candidate.payload });
  }
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.${process.pid}.${randomUUID()}.tmp`;
  const json = `${JSON.stringify(artifact, null, 2)}\n`;
  const parsed = JSON.parse(json);
  if (parsed.id !== artifact.id) fail('serialized artifact id changed');
  writeFileSync(temporary, json, { flag: 'wx' });
  renameSync(temporary, path);
  return artifact;
}

export function readArtifact<TPayload extends object = UnknownRecord>(
  path: string,
  { expectedId = null, expectedKind = null }: ArtifactReadOptions = {},
): Artifact<TPayload> {
  const input = JSON.parse(readFileSync(path, 'utf8'));
  const artifact = validateArtifact(input, { source: path });
  if (expectedId !== null && artifact.id !== expectedId) {
    throw new Error(`artifact ${path} belongs to ${artifact.id ?? 'an unidentified run'}, not ${expectedId}`);
  }
  if (expectedKind !== null && artifact.kind !== normalizeKind(expectedKind)) {
    throw new Error(`artifact ${path} is ${artifact.kind}, not ${normalizeKind(expectedKind)}`);
  }
  return artifact as Artifact<TPayload>;
}

export function artifactPayload<TPayload extends object>(artifact: Artifact<TPayload>): TPayload & UnknownRecord {
  return { ...artifact.payload, artifactSchemaVersion: artifact.artifactSchemaVersion,
    kind: artifact.kind, id: artifact.id, artifactEnvelope: {
      attempt: artifact.attempt,
      timestamps: artifact.timestamps,
      identities: artifact.identities,
    } };
}

export function readArtifactPayload<TPayload extends object = UnknownRecord>(
  path: string,
  options: ArtifactReadOptions = {},
): TPayload & UnknownRecord {
  return artifactPayload(readArtifact<TPayload>(path, options));
}

// Convenience surface for the top-level run producer. New bytes are always
// written through the same strict schema-v2 envelope.
export function writeRunJson(path: string, run: unknown): Artifact {
  if (!isObject(run) || typeof run.id !== 'string' || !run.id) {
    throw new Error('run artifact requires a non-empty id');
  }
  if (run.artifactSchemaVersion !== undefined
    && run.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION) {
    throw new Error(`run artifact schema ${run.artifactSchemaVersion} is not supported`);
  }
  if (run.artifactSchemaVersion === ARTIFACT_SCHEMA_VERSION && run.payload) {
    return writeArtifact(path, run);
  }
  const { id, kind: sourceKind = 'benchmark_run', startedAt, completedAt, generatedAt,
    identities, attempt, parentAttemptId, artifactSchemaVersion: _schema, ...payload } = run;
  return writeArtifact(path, {
    kind: sourceKind,
    id,
    attempt: attempt ?? { id, parentId: parentAttemptId ?? null },
    timestamps: { startedAt: startedAt ?? generatedAt ?? new Date().toISOString(),
      completedAt: completedAt ?? null },
    identities: identities ?? recipeArtifactIdentities(payload.recipeRelease, {
      stackAdapter: payload.backend ? { id: payload.backend } : null,
    }),
    payload,
  });
}

export function readRunJson(path: string, expectedRunId?: string): CostRun & UnknownRecord {
  return readArtifactPayload(path, { expectedId: expectedRunId }) as CostRun & UnknownRecord;
}
