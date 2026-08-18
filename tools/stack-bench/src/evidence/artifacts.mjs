import { randomUUID } from 'node:crypto';
import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';


import { hashDirectory } from './provenance.mjs';
import { validateCheckEvidence } from './check-evidence.mjs';
import { RUNNER_OBSERVATION_FIELDS } from '../runtime/runner-environment.mjs';

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
  'repair_continuation',
  'repair_process',
  'reference_build',
  'reference_qualification',
  'recovery',
  'source_checkpoint',
]);

const KIND_SET = new Set(ARTIFACT_KINDS);
const IDENTITY_KEYS = Object.freeze([
  'engine', 'recipe', 'fixture', 'calibration', 'experiment', 'agentAdapter', 'stackAdapter',
]);
const SECRET_KEYS = new Set(['apikey', 'leasetoken', 'ownershiptoken', 'password', 'secret']);
const ENVELOPE_KEYS = new Set([
  'artifactSchemaVersion', 'kind', 'id', 'attempt', 'timestamps', 'identities', 'payload',
]);
const BENCHMARK_RUN_PAYLOAD_FIELDS = new Set(['status', 'track', 'backend', 'model', 'guidance',
  'condition', 'stack', 'setup', 'backendLease', 'backendDiagnostics', 'validation', 'levels',
  'contaminated', 'contamination', 'mutationControl', 'totals', 'outcome', 'selectionRequest',
  'skills', 'runtime']);
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
    'attempts', 'summary']),
  campaign_report: new Set(['reportSchemaVersion', 'campaign', 'scope', 'policy', 'attempts',
    'conditions', 'summary', 'limitations', 'contentSha256']),
  campaign_state: new Set(['schemaVersion', 'campaignId', 'campaignSha256', 'status',
    'createdAt', 'updatedAt', 'maxParallel', 'attempts', 'summary']),
  contract_lint: new Set(['label', 'url', 'level', 'pass', 'counts', 'results']),
  grade: new Set(['definitionSchemaVersion', 'recipeRelease', 'label', 'url', 'level', 'runId',
    'total', 'max', 'features', 'environment', 'inconclusive', 'selection', 'packRuntime']),
  grade_bundle: new Set(['definitionSchemaVersion', 'recipeRelease', 'calibration', 'label', 'track',
    'backend', 'url', 'app', 'level', 'suites', 'totals', 'code', 'error', 'outcome', 'provenance',
    'actions', 'selection', 'packRuntime', 'observation', 'source']),
  mutation_control: new Set(['durationMs', 'app', 'mutations', 'manifestStatus', 'fixtureSha256',
    'spec', 'backend', 'track', 'ok', 'outcome', 'baseline', 'summary', 'results']),
  null_control: new Set(['durationMs', 'runner', 'tracks', 'ok', 'summary', 'criteria']),
  pack_budget_measurement: new Set(['schemaVersion', 'track', 'level', 'policy', 'evidence',
    'runner', 'samples', 'recommendations']),
  performance_run: new Set(['label', 'backend', 'url', 'clients', 'rounds', 'warmupDiscarded',
    'seededBefore', 'sent', 'delivered', 'lost', 'elapsedMs', 'deliveryLatencyMs', 'server',
    'cpuSecondsPer1kDelivered']),
  preflight: new Set(['schemaVersion', 'generatedAt', 'request', 'ok', 'summary', 'checks']),
  repair_continuation: new Set([...BENCHMARK_RUN_PAYLOAD_FIELDS, 'continuation']),
  repair_process: new Set(['schemaVersion', 'parentRunId', 'level', 'roundsGranted',
    'exitCode', 'signal', 'timedOut', 'streams']),
  reference_build: new Set(['isolation', 'image', 'fixtures', 'ok']),
  reference_qualification: new Set(['fixture', 'fixtureSha256', 'requiredRepetitions', 'isolation',
    'runner', 'mutationControl', 'runs', 'stable', 'sameImage', 'sameHarness', 'harnessSha256', 'ok']),
  recovery: new Set(['schemaVersion', 'status', 'runId', 'backend', 'reason', 'cleanup',
    'resources', 'instructions']),
  source_checkpoint: new Set(['schemaVersion', 'track', 'backend', 'level', 'source',
    'repair', 'outcome', 'selectionSha256']),
});
const HASH = /^[a-f0-9]{64}$/;
const ISO = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/;
import { STACK_BENCH_ROOT as ROOT } from '../project-paths.mjs';
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
  const runnerWhenPresent = () => {
    if (payload.runner === undefined) return;
    objectWhenPresent('runner');
    const allowed = new Set(['schemaVersion', 'mode', 'platform', 'architecture',
      ...RUNNER_OBSERVATION_FIELDS]);
    for (const key of Object.keys(payload.runner)) {
      if (!allowed.has(key)) fail(`${kind} payload.runner.${key} is unknown`);
    }
    if (payload.runner.schemaVersion !== 1) fail(`${kind} payload.runner.schemaVersion must be 1`);
    if (!['appliance', 'local-controller'].includes(payload.runner.mode)) {
      fail(`${kind} payload.runner.mode is invalid`);
    }
    for (const field of ['platform', 'architecture']) {
      if (typeof payload.runner[field] !== 'string' || !payload.runner[field]) {
        fail(`${kind} payload.runner.${field} must be a non-empty string`);
      }
    }
    const observedFields = RUNNER_OBSERVATION_FIELDS.filter(field => payload.runner[field] !== undefined);
    if (payload.runner.mode === 'appliance' && observedFields.length > 0) {
      for (const field of ['dockerEngineVersion', 'dockerOs', 'dockerArchitecture', 'kernelVersion']) {
        if (typeof payload.runner[field] !== 'string' || !payload.runner[field]) {
          fail(`${kind} payload.runner.${field} must be a non-empty string for an appliance runner`);
        }
      }
      for (const field of ['cpuCount', 'memoryBytes']) {
        if (!Number.isSafeInteger(payload.runner[field]) || payload.runner[field] < 1) {
          fail(`${kind} payload.runner.${field} must be a positive integer for an appliance runner`);
        }
      }
    } else if (payload.runner.mode !== 'appliance') {
      for (const field of RUNNER_OBSERVATION_FIELDS) {
        if (payload.runner[field] !== undefined) {
          fail(`${kind} payload.runner.${field} is only valid for an appliance runner`);
        }
      }
    }
  };
  if (['benchmark_run', 'repair_continuation'].includes(kind)) arrayWhenPresent('levels');
  if (kind === 'repair_continuation') {
    objectWhenPresent('continuation');
    if (!isObject(payload.continuation)) fail('repair_continuation payload.continuation is required');
    const fields = new Set(['schemaVersion', 'rootRunId', 'parentRunId', 'level', 'grantIndex',
      'roundsGranted', 'cumulativeRoundsBefore', 'cumulativeRoundsAfter', 'parentCheckpointSha256',
      'baseline', 'resumeSetup', 'downstreamLevelsToRerun', 'cumulativeCostBeforeUsd',
      'cumulativeCostAfterUsd', 'cumulativeDurationBeforeSec', 'cumulativeDurationAfterSec']);
    for (const key of Object.keys(payload.continuation)) {
      if (!fields.has(key)) fail(`repair_continuation payload.continuation.${key} is unknown`);
    }
    if (payload.continuation.schemaVersion !== 1) {
      fail('repair_continuation payload.continuation.schemaVersion must be 1');
    }
    for (const field of ['rootRunId', 'parentRunId']) {
      if (typeof payload.continuation[field] !== 'string' || !payload.continuation[field]) {
        fail(`repair_continuation payload.continuation.${field} is required`);
      }
    }
    for (const field of ['level', 'grantIndex', 'roundsGranted', 'cumulativeRoundsBefore',
      'cumulativeRoundsAfter']) {
      if (!Number.isSafeInteger(payload.continuation[field]) || payload.continuation[field] < 0) {
        fail(`repair_continuation payload.continuation.${field} must be a non-negative integer`);
      }
    }
    if (payload.continuation.level < 1 || payload.continuation.grantIndex < 1
      || payload.continuation.roundsGranted < 1
      || payload.continuation.cumulativeRoundsAfter < payload.continuation.cumulativeRoundsBefore
      || payload.continuation.cumulativeRoundsAfter
        > payload.continuation.cumulativeRoundsBefore + payload.continuation.roundsGranted) {
      fail('repair_continuation payload.continuation round accounting is invalid');
    }
    if (!HASH.test(payload.continuation.parentCheckpointSha256 ?? '')) {
      fail('repair_continuation payload.continuation.parentCheckpointSha256 is invalid');
    }
    for (const [before, after] of [['cumulativeCostBeforeUsd', 'cumulativeCostAfterUsd'],
      ['cumulativeDurationBeforeSec', 'cumulativeDurationAfterSec']]) {
      if (!Number.isFinite(payload.continuation[before]) || payload.continuation[before] < 0
        || !Number.isFinite(payload.continuation[after])
        || payload.continuation[after] < payload.continuation[before]) {
        fail(`repair_continuation payload.continuation.${after} is invalid`);
      }
    }
    if (!Array.isArray(payload.continuation.downstreamLevelsToRerun)
      || payload.continuation.downstreamLevelsToRerun.some(level => !Number.isSafeInteger(level)
        || level <= payload.continuation.level)
      || new Set(payload.continuation.downstreamLevelsToRerun).size
        !== payload.continuation.downstreamLevelsToRerun.length) {
      fail('repair_continuation payload.continuation.downstreamLevelsToRerun is invalid');
    }
    if (payload.continuation.baseline !== null) {
      if (!isObject(payload.continuation.baseline)) {
        fail('repair_continuation payload.continuation.baseline must be an object or null');
      }
      const baselineFields = new Set(['score', 'max', 'selectionSha256', 'sourceSha256',
        'outcome', 'reproduced', 'mismatches']);
      for (const key of Object.keys(payload.continuation.baseline)) {
        if (!baselineFields.has(key)) fail(`repair_continuation payload.continuation.baseline.${key} is unknown`);
      }
      const baseline = payload.continuation.baseline;
      const scoreValid = baseline.score === null && baseline.max === null
        || Number.isSafeInteger(baseline.score) && Number.isSafeInteger(baseline.max)
          && baseline.max >= 1 && baseline.score >= 0 && baseline.score <= baseline.max;
      const sourceValid = baseline.sourceSha256 === null
        || HASH.test(baseline.sourceSha256 ?? '');
      if (!scoreValid
        || (baseline.selectionSha256 !== null && !HASH.test(baseline.selectionSha256 ?? ''))
        || !sourceValid || !isObject(baseline.outcome)
        || typeof baseline.outcome.kind !== 'string' || typeof baseline.reproduced !== 'boolean'
        || !Array.isArray(baseline.mismatches)
        || baseline.mismatches.some(item => typeof item !== 'string' || !item)
        || (baseline.reproduced && (baseline.sourceSha256 === null || baseline.score === null))) {
        fail('repair_continuation payload.continuation.baseline is invalid');
      }
    }
    if (payload.continuation.resumeSetup !== null) {
      if (!isObject(payload.continuation.resumeSetup)) {
        fail('repair_continuation payload.continuation.resumeSetup must be an object or null');
      }
      const setupFields = new Set(['sessionId', 'costUsd', 'durationMs', 'sourceVerified']);
      for (const key of Object.keys(payload.continuation.resumeSetup)) {
        if (!setupFields.has(key)) fail(`repair_continuation payload.continuation.resumeSetup.${key} is unknown`);
      }
      const setup = payload.continuation.resumeSetup;
      if (setup.sessionId !== null && (typeof setup.sessionId !== 'string' || !setup.sessionId)) {
        fail('repair_continuation payload.continuation.resumeSetup.sessionId is invalid');
      }
      if (!Number.isFinite(setup.costUsd) || setup.costUsd < 0
        || !Number.isFinite(setup.durationMs) || setup.durationMs < 0
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
      && (!Number.isInteger(payload.runIndex) || payload.runIndex < 0)) {
      fail('campaign_process payload.runIndex must be a non-negative integer');
    }
    if (payload.exitCode !== null && !Number.isInteger(payload.exitCode)) {
      fail('campaign_process payload.exitCode must be an integer or null');
    }
    if (payload.signal !== null && typeof payload.signal !== 'string') {
      fail('campaign_process payload.signal must be a string or null');
    }
    if (typeof payload.timedOut !== 'boolean') fail('campaign_process payload.timedOut must be boolean');
    if (payload.streams !== null) {
      objectWhenPresent('streams');
      for (const [name, stream] of Object.entries(payload.streams)) {
        if (!['stdout', 'stderr'].includes(name) || !isObject(stream)) {
          fail(`campaign_process payload.streams.${name} is invalid`);
        }
        const allowed = new Set(['path', 'sha256', 'bytes', 'retainedBytes', 'truncated']);
        for (const key of Object.keys(stream)) {
          if (!allowed.has(key)) fail(`campaign_process payload.streams.${name}.${key} is unknown`);
        }
        if (stream.path !== `process.${name}.log`) fail(`campaign_process payload.streams.${name}.path is invalid`);
        if (!HASH.test(stream.sha256)) fail(`campaign_process payload.streams.${name}.sha256 is invalid`);
        if (!Number.isSafeInteger(stream.bytes) || stream.bytes < 0
          || !Number.isSafeInteger(stream.retainedBytes) || stream.retainedBytes < 0
          || stream.retainedBytes > stream.bytes) fail(`campaign_process payload.streams.${name} byte counts are invalid`);
        if (stream.truncated !== (stream.bytes > stream.retainedBytes)) {
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
      if (!Number.isSafeInteger(payload[field]) || payload[field] < 1) {
        fail(`repair_process payload.${field} must be a positive integer`);
      }
    }
    if (payload.exitCode !== null && !Number.isInteger(payload.exitCode)) {
      fail('repair_process payload.exitCode must be an integer or null');
    }
    if (payload.signal !== null && typeof payload.signal !== 'string') {
      fail('repair_process payload.signal must be a string or null');
    }
    if (typeof payload.timedOut !== 'boolean') fail('repair_process payload.timedOut must be boolean');
    if (payload.streams !== null && !isObject(payload.streams)) {
      fail('repair_process payload.streams must be an object or null');
    }
    for (const [name, stream] of Object.entries(payload.streams ?? {})) {
      if (!['stdout', 'stderr'].includes(name) || !isObject(stream)) {
        fail(`repair_process payload.streams.${name} is invalid`);
      }
      const allowed = new Set(['path', 'sha256', 'bytes', 'retainedBytes', 'truncated']);
      for (const key of Object.keys(stream)) {
        if (!allowed.has(key)) fail(`repair_process payload.streams.${name}.${key} is unknown`);
      }
      if (stream.path !== `process.${name}.log` || !HASH.test(stream.sha256)
        || !Number.isSafeInteger(stream.bytes) || stream.bytes < 0
        || !Number.isSafeInteger(stream.retainedBytes) || stream.retainedBytes < 0
        || stream.retainedBytes > stream.bytes
        || stream.truncated !== (stream.bytes > stream.retainedBytes)) {
        fail(`repair_process payload.streams.${name} is inconsistent`);
      }
    }
  }
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
    const observation = payload.observation ?? 'scored';
    if (!['scored', 'observed'].includes(observation)) {
      fail('grade_bundle payload.observation must be scored or observed');
    }
    if (observation === 'observed') {
      if (!isObject(payload.source) || Object.keys(payload.source).length !== 1
        || !HASH.test(payload.source.sha256 ?? '')) {
        fail('grade_bundle observed payload.source must contain its first-build SHA-256');
      }
      if (payload.selection?.observation !== 'observed'
        || payload.selection?.scoredPoints !== 0
        || !Number.isSafeInteger(payload.selection?.observedPoints)
        || payload.selection.observedPoints < 0) {
        fail('grade_bundle observed selection must be diagnostic and contribute zero score');
      }
    } else if (payload.source !== undefined) {
      fail('grade_bundle scored observation cannot claim an observed source');
    }
    for (const [suiteId, suite] of Object.entries(payload.suites ?? {})) {
      if (isObject(suite)) validateGradeFeatures(suite.features, `grade_bundle payload.suites.${suiteId}.features`);
    }
  }
  if (kind === 'mutation_control') arrayWhenPresent('results');
  if (kind === 'null_control') { arrayWhenPresent('criteria'); runnerWhenPresent(); }
  if (kind === 'pack_budget_measurement') {
    objectWhenPresent('policy'); arrayWhenPresent('evidence'); arrayWhenPresent('samples');
    arrayWhenPresent('recommendations'); runnerWhenPresent();
  }
  if (kind === 'performance_run') { objectWhenPresent('deliveryLatencyMs'); objectWhenPresent('server'); }
  if (kind === 'reference_build') arrayWhenPresent('fixtures');
  if (kind === 'reference_qualification') {
    arrayWhenPresent('runs'); runnerWhenPresent();
  }
  if (kind === 'recovery') { objectWhenPresent('cleanup'); objectWhenPresent('resources');
    arrayWhenPresent('instructions'); }
  if (kind === 'source_checkpoint') {
    if (payload.schemaVersion !== 1) fail('source_checkpoint payload.schemaVersion must be 1');
    for (const field of ['track', 'backend']) {
      if (typeof payload[field] !== 'string' || !payload[field]) {
        fail(`source_checkpoint payload.${field} must be a non-empty string`);
      }
    }
    if (!Number.isSafeInteger(payload.level) || payload.level < 1) {
      fail('source_checkpoint payload.level must be a positive integer');
    }
    objectWhenPresent('source');
    if (!isObject(payload.source)) fail('source_checkpoint payload.source is required');
    const sourceFields = new Set(['directory', 'sha256', 'files']);
    for (const key of Object.keys(payload.source)) {
      if (!sourceFields.has(key)) fail(`source_checkpoint payload.source.${key} is unknown`);
    }
    if (payload.source.directory !== `level-l${payload.level}-source`) {
      fail('source_checkpoint payload.source.directory does not match its level');
    }
    if (!HASH.test(payload.source.sha256 ?? '')) {
      fail('source_checkpoint payload.source.sha256 is invalid');
    }
    if (!Number.isSafeInteger(payload.source.files) || payload.source.files < 0) {
      fail('source_checkpoint payload.source.files must be a non-negative integer');
    }
    objectWhenPresent('repair');
    if (!isObject(payload.repair)) fail('source_checkpoint payload.repair is required');
    const repairFields = new Set(['status', 'budgetRounds', 'roundsUsed', 'stopReason']);
    for (const key of Object.keys(payload.repair)) {
      if (!repairFields.has(key)) fail(`source_checkpoint payload.repair.${key} is unknown`);
    }
    if (!['ungraded', 'not-needed', 'corrected', 'budget-exhausted', 'incomplete']
      .includes(payload.repair.status)) fail('source_checkpoint payload.repair.status is invalid');
    for (const field of ['budgetRounds', 'roundsUsed']) {
      if (!Number.isSafeInteger(payload.repair[field]) || payload.repair[field] < 0) {
        fail(`source_checkpoint payload.repair.${field} must be a non-negative integer`);
      }
    }
    if (payload.repair.roundsUsed > payload.repair.budgetRounds) {
      fail('source_checkpoint payload.repair.roundsUsed exceeds its budget');
    }
    if (payload.repair.stopReason !== null && typeof payload.repair.stopReason !== 'string') {
      fail('source_checkpoint payload.repair.stopReason must be a string or null');
    }
    objectWhenPresent('outcome');
    if (!isObject(payload.outcome) || typeof payload.outcome.kind !== 'string'
      || !payload.outcome.kind) fail('source_checkpoint payload.outcome.kind is required');
    if (payload.selectionSha256 !== null && !HASH.test(payload.selectionSha256 ?? '')) {
      fail('source_checkpoint payload.selectionSha256 is invalid');
    }
  }
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
  const excludedRoots = new Set(['archive', 'reference-apps', 'results', 'tests', 'tracks']);
  const executable = hashDirectory(ROOT, { exclude: (name, entry) => {
    const parts = name.split('/');
    if (parts[0].startsWith('.') || excludedRoots.has(parts[0])
      || parts.includes('node_modules')
      || parts.some(part => part.startsWith('.spacetime-data'))) return true;
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
