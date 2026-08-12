import { randomUUID } from 'node:crypto';
import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

import { hashDirectory } from './provenance.mjs';
import { validateCheckEvidence } from './check-evidence.mjs';

export const ARTIFACT_SCHEMA_VERSION = 2;

export const ARTIFACT_KINDS = Object.freeze([
  'action_check',
  'benchmark_run',
  'backend_lease_evidence',
  'bug_report_quality',
  'contract_lint',
  'grade',
  'grade_bundle',
  'mutation_control',
  'null_control',
  'performance_run',
  'preflight',
  'reference_build',
  'reference_qualification',
  'recovery',
]);

const KIND_SET = new Set(ARTIFACT_KINDS);
const IDENTITY_KEYS = Object.freeze([
  'engine', 'recipe', 'fixture', 'calibration', 'experiment', 'agentAdapter', 'stackAdapter',
]);
const SECRET_KEYS = new Set(['apikey', 'leasetoken', 'ownershiptoken', 'password', 'secret']);
const ENVELOPE_KEYS = new Set([
  'artifactSchemaVersion', 'kind', 'id', 'attempt', 'timestamps', 'identities', 'payload',
]);
const PAYLOAD_FIELDS = Object.freeze({
  action_check: new Set(['backend', 'results', 'missing']),
  backend_lease_evidence: new Set(['version', 'runId', 'backend', 'track', 'runIndex', 'ownerPid',
    'createdAt', 'stoppedAt', 'releasedAt', 'state', 'resources', 'ownership']),
  benchmark_run: new Set(['status', 'track', 'backend', 'model', 'guidance', 'stack',
    'setup', 'backendLease', 'backendDiagnostics', 'validation', 'levels', 'contaminated', 'contamination',
    'mutationControl', 'totals', 'outcome', 'selectionRequest']),
  bug_report_quality: new Set(['bugs', 'vague', 'vaguePct']),
  contract_lint: new Set(['label', 'url', 'level', 'pass', 'counts', 'results']),
  grade: new Set(['definitionSchemaVersion', 'recipeRelease', 'label', 'url', 'level', 'runId',
    'total', 'max', 'features', 'environment', 'inconclusive', 'selection']),
  grade_bundle: new Set(['definitionSchemaVersion', 'recipeRelease', 'calibration', 'label', 'track',
    'backend', 'url', 'app', 'level', 'suites', 'totals', 'code', 'error', 'outcome', 'provenance',
    'actions', 'selection']),
  mutation_control: new Set(['durationMs', 'app', 'mutations', 'manifestStatus', 'fixtureSha256',
    'spec', 'backend', 'track', 'ok', 'outcome', 'baseline', 'summary', 'results']),
  null_control: new Set(['durationMs', 'tracks', 'ok', 'summary', 'criteria']),
  performance_run: new Set(['label', 'backend', 'url', 'clients', 'rounds', 'warmupDiscarded',
    'seededBefore', 'sent', 'delivered', 'lost', 'elapsedMs', 'deliveryLatencyMs', 'server',
    'cpuSecondsPer1kDelivered']),
  preflight: new Set(['schemaVersion', 'generatedAt', 'request', 'ok', 'summary', 'checks']),
  reference_build: new Set(['isolation', 'image', 'fixtures', 'ok']),
  reference_qualification: new Set(['fixture', 'fixtureSha256', 'requiredRepetitions', 'isolation',
    'mutationControl', 'runs', 'stable', 'sameImage', 'sameHarness', 'harnessSha256', 'ok']),
  recovery: new Set(['schemaVersion', 'status', 'runId', 'backend', 'reason', 'cleanup',
    'resources', 'instructions']),
});
const HASH = /^[a-f0-9]{64}$/;
const ISO = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/;
const ROOT = dirname(fileURLToPath(import.meta.url));
let cachedEngineIdentity = null;

const isObject = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const fail = message => { throw new Error(`invalid artifact: ${message}`); };

function normalizeKind(kind) {
  if (!KIND_SET.has(kind)) fail(`unknown kind ${JSON.stringify(kind)}`);
  return kind;
}

function timestamp(value, at) {
  if (typeof value !== 'string' || !ISO.test(value) || Number.isNaN(Date.parse(value))) {
    fail(`${at} must be an ISO-8601 UTC timestamp`);
  }
  return value;
}

function identity(value, at) {
  if (value === null) return null;
  if (!isObject(value)) fail(`${at} must be an object or null`);
  const allowed = new Set(['id', 'version', 'sha256', 'state']);
  for (const key of Object.keys(value)) if (!allowed.has(key)) fail(`${at}.${key} is unknown`);
  if (typeof value.id !== 'string' || !value.id) fail(`${at}.id must be a non-empty string`);
  if (value.version !== null && value.version !== undefined
    && (typeof value.version !== 'string' || !value.version)) fail(`${at}.version is invalid`);
  if (value.sha256 !== null && value.sha256 !== undefined && !HASH.test(value.sha256)) {
    fail(`${at}.sha256 must be 64 lowercase hexadecimal characters`);
  }
  if (value.state !== null && value.state !== undefined
    && (typeof value.state !== 'string' || !value.state)) fail(`${at}.state is invalid`);
  return {
    id: value.id,
    version: value.version ?? null,
    sha256: value.sha256 ?? null,
    state: value.state ?? null,
  };
}

function validateIdentities(value, { requireEngine = false } = {}) {
  if (!isObject(value)) fail('identities must be an object');
  const allowed = new Set([...IDENTITY_KEYS, 'packs']);
  for (const key of Object.keys(value)) if (!allowed.has(key)) fail(`identities.${key} is unknown`);
  const normalized = Object.fromEntries(IDENTITY_KEYS.map(key => [key, identity(value[key] ?? null,
    `identities.${key}`)]));
  if (requireEngine && normalized.engine === null) fail('identities.engine is required');
  if (!Array.isArray(value.packs ?? [])) fail('identities.packs must be an array');
  normalized.packs = (value.packs ?? []).map((item, index) => identity(item, `identities.packs[${index}]`));
  const packIds = new Set();
  for (const pack of normalized.packs) {
    const key = `${pack.id}@${pack.version ?? ''}:${pack.sha256 ?? ''}`;
    if (packIds.has(key)) fail(`identities.packs duplicates ${key}`);
    packIds.add(key);
  }
  normalized.packs.sort((a, b) => `${a.id}@${a.version ?? ''}`.localeCompare(`${b.id}@${b.version ?? ''}`));
  return normalized;
}

function validatePayload(kind, payload) {
  if (!isObject(payload)) fail('payload must be an object');
  for (const key of Object.keys(payload)) {
    if (!PAYLOAD_FIELDS[kind].has(key)) fail(`${kind} payload.${key} is unknown`);
  }
  const arrayWhenPresent = field => {
    if (payload[field] !== undefined && !Array.isArray(payload[field])) {
      fail(`${kind} payload.${field} must be an array when present`);
    }
  };
  const objectWhenPresent = field => {
    if (payload[field] !== undefined && !isObject(payload[field])) {
      fail(`${kind} payload.${field} must be an object when present`);
    }
  };
  if (kind === 'benchmark_run') arrayWhenPresent('levels');
  const validateGradeFeatures = (features, at) => {
    if (!Array.isArray(features)) return;
    features.forEach((feature, featureIndex) => {
      if (!isObject(feature)) fail(`${at}[${featureIndex}] must be an object`);
      if (feature.setupEvidence === undefined) fail(`${at}[${featureIndex}].setupEvidence is required`);
      try { validateCheckEvidence(feature.setupEvidence,
        { at: `${at}[${featureIndex}].setupEvidence` }); }
      catch (error) { fail(error.message); }
      if (feature.setupEvidence.phase !== 'setup') {
        fail(`${at}[${featureIndex}].setupEvidence must use setup phase`);
      }
      if (feature.criteria !== undefined && !Array.isArray(feature.criteria)) {
        fail(`${at}[${featureIndex}].criteria must be an array when present`);
      }
      (feature.criteria ?? []).forEach((criterion, criterionIndex) => {
        const criterionAt = `${at}[${featureIndex}].criteria[${criterionIndex}]`;
        if (!isObject(criterion)) fail(`${criterionAt} must be an object`);
        for (const obsolete of ['passed', 'inconclusive', 'detail']) {
          if (Object.hasOwn(criterion, obsolete)) fail(`${criterionAt}.${obsolete} is obsolete; use evidence`);
        }
        if (criterion.evidence === undefined) fail(`${criterionAt}.evidence is required`);
        try { validateCheckEvidence(criterion.evidence, { at: `${criterionAt}.evidence` }); }
        catch (error) { fail(error.message); }
      });
    });
  };
  if (kind === 'grade') {
    arrayWhenPresent('features');
    validateGradeFeatures(payload.features, 'grade payload.features');
  }
  if (kind === 'grade_bundle') {
    objectWhenPresent('suites'); objectWhenPresent('totals');
    for (const [suiteId, suite] of Object.entries(payload.suites ?? {})) {
      if (isObject(suite)) validateGradeFeatures(suite.features, `grade_bundle payload.suites.${suiteId}.features`);
    }
  }
  if (kind === 'mutation_control') arrayWhenPresent('results');
  if (kind === 'null_control') arrayWhenPresent('criteria');
  if (kind === 'performance_run') { objectWhenPresent('deliveryLatencyMs'); objectWhenPresent('server'); }
  if (kind === 'reference_build') arrayWhenPresent('fixtures');
  if (kind === 'reference_qualification') arrayWhenPresent('runs');
  if (kind === 'recovery') { objectWhenPresent('cleanup'); objectWhenPresent('resources');
    arrayWhenPresent('instructions'); }
  if (kind === 'contract_lint') { arrayWhenPresent('results'); objectWhenPresent('counts'); }
  if (kind === 'action_check') { arrayWhenPresent('results'); arrayWhenPresent('missing'); }
  return payload;
}

function rejectSecrets(value, at = '$', seen = new Set()) {
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

export function emptyArtifactIdentities(overrides = {}) {
  return validateIdentities({ engine: currentEngineIdentity(), packs: [], ...overrides },
    { requireEngine: true });
}

export function currentEngineIdentity() {
  if (cachedEngineIdentity) return structuredClone(cachedEngineIdentity);
  const excludedRoots = new Set(['archive', 'node_modules', 'reference-apps', 'results', 'tests', 'tracks']);
  const executable = hashDirectory(ROOT, { exclude: (name, entry) => {
    const parts = name.split('/');
    if (parts[0].startsWith('.') || excludedRoots.has(parts[0])
      || parts.some(part => part.startsWith('.spacetime-data'))) return true;
    if (entry.isDirectory()) return false;
    return !(/\.(?:mjs|js|json|ya?ml|sh)$/.test(name) || /(?:^|\/)Dockerfile$/.test(name));
  } });
  cachedEngineIdentity = { id: 'stack-bench', version: null, sha256: executable.sha256, state: null };
  return structuredClone(cachedEngineIdentity);
}

export function recipeArtifactIdentities(recipeRelease, overrides = {}) {
  if (!recipeRelease) return emptyArtifactIdentities(overrides);
  return emptyArtifactIdentities({
    recipe: { id: recipeRelease.id, version: recipeRelease.version,
      sha256: recipeRelease.contentSha256, state: recipeRelease.state },
    fixture: recipeRelease.components?.fixture ? {
      id: recipeRelease.components.fixture.id,
      version: recipeRelease.components.fixture.version,
      sha256: recipeRelease.components.fixture.sha256 ?? null,
      state: recipeRelease.components.fixture.state,
    } : null,
    packs: (recipeRelease.components?.packs ?? []).map(pack => ({
      id: pack.id, version: pack.version, sha256: pack.sha256 ?? null, state: pack.state,
    })),
    ...overrides,
  });
}

export function createArtifact({ kind, id, attempt = null, timestamps = null,
  identities = null, payload = {} }) {
  const normalizedKind = normalizeKind(kind);
  if (typeof id !== 'string' || !id) fail('id must be a non-empty string');
  const now = new Date().toISOString();
  const startedAt = timestamp(timestamps?.startedAt ?? now, 'timestamps.startedAt');
  const completedAt = timestamps?.completedAt == null
    ? null : timestamp(timestamps.completedAt, 'timestamps.completedAt');
  if (completedAt && Date.parse(completedAt) < Date.parse(startedAt)) {
    fail('timestamps.completedAt precedes timestamps.startedAt');
  }
  const normalizedAttempt = attempt ?? { id, parentId: null };
  if (!isObject(normalizedAttempt) || typeof normalizedAttempt.id !== 'string' || !normalizedAttempt.id) {
    fail('attempt.id must be a non-empty string');
  }
  if (normalizedAttempt.parentId !== null && normalizedAttempt.parentId !== undefined
    && (typeof normalizedAttempt.parentId !== 'string' || !normalizedAttempt.parentId)) {
    fail('attempt.parentId must be a non-empty string or null');
  }
  const artifact = {
    artifactSchemaVersion: ARTIFACT_SCHEMA_VERSION,
    kind: normalizedKind,
    id,
    attempt: { id: normalizedAttempt.id, parentId: normalizedAttempt.parentId ?? null },
    timestamps: { startedAt, completedAt },
    identities: validateIdentities({ engine: currentEngineIdentity(), packs: [], ...(identities ?? {}) },
      { requireEngine: true }),
    payload: validatePayload(normalizedKind, structuredClone(payload)),
  };
  rejectSecrets(artifact);
  return artifact;
}

export function validateArtifact(input, { source = '<artifact>' } = {}) {
  if (!isObject(input)) fail(`${source} must be an object`);
  if (input.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION) {
    fail(`${source} uses unsupported schema ${input.artifactSchemaVersion ?? 'missing'}`);
  }
  for (const key of Object.keys(input)) {
    if (!ENVELOPE_KEYS.has(key)) fail(`${source}.${key} is unknown`);
  }
  for (const key of ENVELOPE_KEYS) {
    if (!Object.hasOwn(input, key)) fail(`${source}.${key} is required`);
  }
  if (!isObject(input.attempt) || !Object.hasOwn(input.attempt, 'id')
    || !Object.hasOwn(input.attempt, 'parentId')) fail(`${source}.attempt is incomplete`);
  if (!isObject(input.timestamps) || !Object.hasOwn(input.timestamps, 'startedAt')
    || !Object.hasOwn(input.timestamps, 'completedAt')) fail(`${source}.timestamps is incomplete`);
  if (!isObject(input.identities)) fail(`${source}.identities must be an object`);
  for (const key of [...IDENTITY_KEYS, 'packs']) {
    if (!Object.hasOwn(input.identities, key)) fail(`${source}.identities.${key} is required`);
  }
  return createArtifact(input);
}

export function writeArtifact(path, input) {
  if (input?.artifactSchemaVersion !== undefined
    && input.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION) {
    fail(`${path} uses unsupported schema ${input.artifactSchemaVersion}`);
  }
  const artifact = input?.artifactSchemaVersion === ARTIFACT_SCHEMA_VERSION
    ? validateArtifact(input, { source: path }) : createArtifact(input);
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.${process.pid}.${randomUUID()}.tmp`;
  const json = `${JSON.stringify(artifact, null, 2)}\n`;
  const parsed = JSON.parse(json);
  if (parsed.id !== artifact.id) fail('serialized artifact id changed');
  writeFileSync(temporary, json, { flag: 'wx' });
  renameSync(temporary, path);
  return artifact;
}

export function readArtifact(path, { expectedId = null, expectedKind = null } = {}) {
  const input = JSON.parse(readFileSync(path, 'utf8'));
  const artifact = validateArtifact(input, { source: path });
  if (expectedId !== null && artifact.id !== expectedId) {
    throw new Error(`artifact ${path} belongs to ${artifact.id ?? 'an unidentified run'}, not ${expectedId}`);
  }
  if (expectedKind !== null && artifact.kind !== normalizeKind(expectedKind)) {
    throw new Error(`artifact ${path} is ${artifact.kind}, not ${normalizeKind(expectedKind)}`);
  }
  return artifact;
}

export function artifactPayload(artifact) {
  return { ...artifact.payload, artifactSchemaVersion: artifact.artifactSchemaVersion,
    kind: artifact.kind, id: artifact.id, artifactEnvelope: {
      attempt: artifact.attempt,
      timestamps: artifact.timestamps,
      identities: artifact.identities,
    } };
}

export function readArtifactPayload(path, options = {}) {
  return artifactPayload(readArtifact(path, options));
}

// Convenience surface for the top-level run producer. New bytes are always
// written through the same strict schema-v2 envelope.
export function writeRunJson(path, run) {
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

export function readRunJson(path, expectedRunId) {
  return readArtifactPayload(path, { expectedId: expectedRunId });
}
