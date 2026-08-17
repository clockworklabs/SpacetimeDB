import { existsSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

import { compilePromotionFile } from './composition-compiler.mjs';
import { canonicalDefinitionJson, canonicalizeDefinition } from './definition-plan.mjs';
import { mutationScenario, mutationTargetKeys, validateMutationDefinitions } from './mutation-analysis.mjs';
import { sha256 } from './provenance.mjs';
import { loadReferenceRegistry, validateReferenceRegistry } from './reference-fixtures.mjs';
import { readArtifact } from './artifacts.mjs';
import { executionPlanForRelease } from './recipe-release.mjs';
import { missingRunnerObservation } from './runner-environment.mjs';

export const CALIBRATION_SCHEMA_VERSION = 1;

const HASH = /^[a-f0-9]{64}$/;
const ID = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/;
const STATES = new Set(['draft', 'qualified', 'retired']);
const STACK_STATES = new Set(['candidate', 'qualified', 'unsupported']);
const CONTROL_POLICIES = new Map([
  ['promotion-gate', 'must-pass-reference-and-kill-declared-mutant'],
  ['precondition', 'must-pass-reference'],
  ['diagnostic', 'record-only-until-calibrated'],
]);

const isObject = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const fail = (at, message) => { throw new Error(`invalid calibration at ${at}: ${message}`); };

function strictObject(value, at, fields) {
  if (!isObject(value)) fail(at, 'must be an object');
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`${at}.${key}`, 'unknown field');
}

function string(value, at) {
  if (typeof value !== 'string' || !value.trim()) fail(at, 'must be a non-empty string');
  return value;
}

function exactHash(value, at) {
  string(value, at);
  if (!HASH.test(value)) fail(at, 'must be 64 lowercase hexadecimal characters');
  return value;
}

function exactId(value, at) {
  string(value, at);
  if (!ID.test(value)) fail(at, 'must be a stable lowercase id');
  return value;
}

function exactVersion(value, at) {
  string(value, at);
  if (!VERSION.test(value)) fail(at, 'must be an exact semantic version');
  return value;
}

function array(value, at, { nonEmpty = false } = {}) {
  if (!Array.isArray(value) || (nonEmpty && value.length === 0)) {
    fail(at, `must be a${nonEmpty ? ' non-empty' : 'n'} array`);
  }
  return value;
}

function readJson(path, label) {
  try { return JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) { throw new Error(`cannot read ${label} ${path}: ${error.message}`, { cause: error }); }
}

function contained(root, from, path, at) {
  string(path, at);
  const lexicalRoot = resolve(root);
  const candidate = resolve(from, path);
  const rel = relative(lexicalRoot, candidate);
  if (rel === '..' || rel.startsWith(`..${sep}`)) fail(at, `escapes ${lexicalRoot}`);
  if (!existsSync(candidate)) fail(at, `does not exist: ${path}`);
  const realRoot = realpathSync(lexicalRoot);
  const target = realpathSync(candidate);
  const realRel = relative(realRoot, target);
  if (realRel === '..' || realRel.startsWith(`..${sep}`)) fail(at, `escapes ${realRoot}`);
  return { absolute: target, relative: realRel.replaceAll('\\', '/') };
}

const ROOT_FIELDS = new Set([
  'schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'track', 'recipe',
  'fixture', 'references', 'mutations', 'nullControl', 'controls', 'qualification',
  'equivalenceDecisions', 'promotion',
]);
const RECIPE_FIELDS = new Set([
  'path', 'id', 'version', 'meaningSha256', 'executionSha256', 'contentSha256',
]);
const FIXTURE_FIELDS = new Set(['id', 'version', 'sourceSha256']);
const REFERENCES_FIELDS = new Set(['registryPath', 'entries']);
const REFERENCE_FIELDS = new Set(['backend', 'id', 'sourceSha256']);
const MUTATION_FIELDS = new Set(['backend', 'path', 'sha256', 'referenceId']);
const NULL_FIELDS = new Set(['pointBearing', 'zeroPoint', 'repetitions']);
const CONTROL_FIELDS = new Set(['stableKey', 'role', 'promotionPolicy', 'mutationTargets', 'reason']);
const QUALIFICATION_FIELDS = new Set([
  'exactCombinationRequired', 'referenceRepetitions', 'mutationRepetitions', 'runner', 'stacks', 'evidence',
]);
const RUNNER_FIELDS = new Set(['schemaVersion', 'mode', 'platform', 'architecture']);
const STACK_FIELDS = new Set(['id', 'status']);
const EVIDENCE_FIELDS = new Set(['kind', 'stack', 'repetition', 'path', 'sha256']);
const EQUIVALENCE_EVIDENCE_FIELDS = new Set(['path', 'sha256']);
const EQUIVALENCE_FIELDS = new Set([
  'fromExecutionSha256', 'toExecutionSha256', 'rationale', 'evidence',
]);
const PROMOTION_FIELDS = new Set(['catalogPath', 'catalogSha256', 'alias', 'status']);

export function compileCalibrationDefinition(input, { source = '<calibration>' } = {}) {
  const value = structuredClone(input);
  strictObject(value, source, ROOT_FIELDS);
  if (value.schemaVersion !== CALIBRATION_SCHEMA_VERSION) fail(`${source}.schemaVersion`, 'must be 1');
  if (value.kind !== 'calibration-manifest') fail(`${source}.kind`, 'must be "calibration-manifest"');
  exactId(value.id, `${source}.id`);
  exactVersion(value.version, `${source}.version`);
  if (!STATES.has(value.state)) fail(`${source}.state`, 'must be draft, qualified, or retired');
  string(value.title, `${source}.title`);
  exactId(value.track, `${source}.track`);

  strictObject(value.recipe, `${source}.recipe`, RECIPE_FIELDS);
  string(value.recipe.path, `${source}.recipe.path`);
  exactId(value.recipe.id, `${source}.recipe.id`);
  exactVersion(value.recipe.version, `${source}.recipe.version`);
  for (const field of ['meaningSha256', 'executionSha256', 'contentSha256']) {
    exactHash(value.recipe[field], `${source}.recipe.${field}`);
  }

  strictObject(value.fixture, `${source}.fixture`, FIXTURE_FIELDS);
  exactId(value.fixture.id, `${source}.fixture.id`);
  exactVersion(value.fixture.version, `${source}.fixture.version`);
  exactHash(value.fixture.sourceSha256, `${source}.fixture.sourceSha256`);

  strictObject(value.references, `${source}.references`, REFERENCES_FIELDS);
  string(value.references.registryPath, `${source}.references.registryPath`);
  array(value.references.entries, `${source}.references.entries`, { nonEmpty: true });
  const referenceIds = new Set();
  const backends = new Set();
  value.references.entries.forEach((entry, index) => {
    const at = `${source}.references.entries[${index}]`;
    strictObject(entry, at, REFERENCE_FIELDS);
    string(entry.backend, `${at}.backend`);
    exactId(entry.id, `${at}.id`);
    exactHash(entry.sourceSha256, `${at}.sourceSha256`);
    if (referenceIds.has(entry.id)) fail(`${at}.id`, `duplicates ${entry.id}`);
    if (backends.has(entry.backend)) fail(`${at}.backend`, `duplicates ${entry.backend}`);
    referenceIds.add(entry.id);
    backends.add(entry.backend);
  });

  array(value.mutations, `${source}.mutations`, { nonEmpty: true });
  const mutationBackends = new Set();
  value.mutations.forEach((entry, index) => {
    const at = `${source}.mutations[${index}]`;
    strictObject(entry, at, MUTATION_FIELDS);
    string(entry.backend, `${at}.backend`);
    string(entry.path, `${at}.path`);
    exactHash(entry.sha256, `${at}.sha256`);
    exactId(entry.referenceId, `${at}.referenceId`);
    if (!referenceIds.has(entry.referenceId)) fail(`${at}.referenceId`, 'does not name a selected reference');
    if (mutationBackends.has(entry.backend)) fail(`${at}.backend`, `duplicates ${entry.backend}`);
    mutationBackends.add(entry.backend);
  });

  strictObject(value.nullControl, `${source}.nullControl`, NULL_FIELDS);
  if (value.nullControl.pointBearing !== 'must-fail-conclusively') {
    fail(`${source}.nullControl.pointBearing`, 'must be must-fail-conclusively');
  }
  if (value.nullControl.zeroPoint !== 'typed-policy') {
    fail(`${source}.nullControl.zeroPoint`, 'must be typed-policy');
  }
  if (!Number.isInteger(value.nullControl.repetitions) || value.nullControl.repetitions < 1) {
    fail(`${source}.nullControl.repetitions`, 'must be a positive integer');
  }

  array(value.controls, `${source}.controls`);
  const controlKeys = new Set();
  value.controls.forEach((control, index) => {
    const at = `${source}.controls[${index}]`;
    strictObject(control, at, CONTROL_FIELDS);
    string(control.stableKey, `${at}.stableKey`);
    if (!CONTROL_POLICIES.has(control.role)) fail(`${at}.role`, 'has an unknown control role');
    if (control.promotionPolicy !== CONTROL_POLICIES.get(control.role)) {
      fail(`${at}.promotionPolicy`, `must be ${CONTROL_POLICIES.get(control.role)} for ${control.role}`);
    }
    control.mutationTargets = array(control.mutationTargets ?? [], `${at}.mutationTargets`);
    control.mutationTargets.forEach((target, targetIndex) => string(target, `${at}.mutationTargets[${targetIndex}]`));
    if (new Set(control.mutationTargets).size !== control.mutationTargets.length) {
      fail(`${at}.mutationTargets`, 'must not contain duplicates');
    }
    if (control.role === 'promotion-gate' && control.mutationTargets.length === 0) {
      fail(`${at}.mutationTargets`, 'promotion-gate controls require a declared mutant');
    }
    if (control.role !== 'promotion-gate' && control.mutationTargets.length > 0) {
      fail(`${at}.mutationTargets`, 'is allowed only for promotion-gate controls');
    }
    if (control.reason !== undefined) string(control.reason, `${at}.reason`);
    if (control.role === 'diagnostic' && control.reason === undefined) {
      fail(`${at}.reason`, 'is required for diagnostic controls');
    }
    if (controlKeys.has(control.stableKey)) fail(`${at}.stableKey`, `duplicates ${control.stableKey}`);
    controlKeys.add(control.stableKey);
  });

  strictObject(value.qualification, `${source}.qualification`, QUALIFICATION_FIELDS);
  if (value.qualification.exactCombinationRequired !== true) {
    fail(`${source}.qualification.exactCombinationRequired`, 'must be true');
  }
  for (const field of ['referenceRepetitions', 'mutationRepetitions']) {
    if (!Number.isInteger(value.qualification[field]) || value.qualification[field] < 2) {
      fail(`${source}.qualification.${field}`, 'must be an integer of at least 2');
    }
  }
  if (value.qualification.runner !== undefined) {
    const at = `${source}.qualification.runner`;
    strictObject(value.qualification.runner, at, RUNNER_FIELDS);
    if (value.qualification.runner.schemaVersion !== 1) fail(`${at}.schemaVersion`, 'must be 1');
    if (value.qualification.runner.mode !== 'appliance') fail(`${at}.mode`, 'must be appliance');
    string(value.qualification.runner.platform, `${at}.platform`);
    string(value.qualification.runner.architecture, `${at}.architecture`);
  }
  array(value.qualification.stacks, `${source}.qualification.stacks`, { nonEmpty: true });
  const stackIds = new Set();
  value.qualification.stacks.forEach((stack, index) => {
    const at = `${source}.qualification.stacks[${index}]`;
    strictObject(stack, at, STACK_FIELDS);
    string(stack.id, `${at}.id`);
    if (!STACK_STATES.has(stack.status)) fail(`${at}.status`, 'must be candidate, qualified, or unsupported');
    if (stackIds.has(stack.id)) fail(`${at}.id`, `duplicates ${stack.id}`);
    stackIds.add(stack.id);
  });
  array(value.qualification.evidence, `${source}.qualification.evidence`);
  value.qualification.evidence.forEach((entry, index) => {
    const at = `${source}.qualification.evidence[${index}]`;
    strictObject(entry, at, EVIDENCE_FIELDS);
    if (!['reference', 'mutation', 'null'].includes(entry.kind)) {
      fail(`${at}.kind`, 'must be reference, mutation, or null');
    }
    if (!Number.isInteger(entry.repetition) || entry.repetition < 1) {
      fail(`${at}.repetition`, 'must be a positive integer');
    }
    if (entry.kind === 'null') {
      if (entry.stack !== undefined) fail(`${at}.stack`, 'is not allowed for null evidence');
    } else string(entry.stack, `${at}.stack`);
    string(entry.path, `${at}.path`);
    exactHash(entry.sha256, `${at}.sha256`);
  });

  value.equivalenceDecisions = array(value.equivalenceDecisions ?? [], `${source}.equivalenceDecisions`);
  value.equivalenceDecisions.forEach((decision, index) => {
    const at = `${source}.equivalenceDecisions[${index}]`;
    strictObject(decision, at, EQUIVALENCE_FIELDS);
    exactHash(decision.fromExecutionSha256, `${at}.fromExecutionSha256`);
    exactHash(decision.toExecutionSha256, `${at}.toExecutionSha256`);
    if (decision.fromExecutionSha256 === decision.toExecutionSha256) fail(at, 'must compare different execution hashes');
    string(decision.rationale, `${at}.rationale`);
    array(decision.evidence, `${at}.evidence`, { nonEmpty: true });
    decision.evidence.forEach((entry, evidenceIndex) => {
      strictObject(entry, `${at}.evidence[${evidenceIndex}]`, EQUIVALENCE_EVIDENCE_FIELDS);
      string(entry.path, `${at}.evidence[${evidenceIndex}].path`);
      exactHash(entry.sha256, `${at}.evidence[${evidenceIndex}].sha256`);
    });
  });

  strictObject(value.promotion, `${source}.promotion`, PROMOTION_FIELDS);
  string(value.promotion.catalogPath, `${source}.promotion.catalogPath`);
  exactHash(value.promotion.catalogSha256, `${source}.promotion.catalogSha256`);
  if (!/^L[1-9]\d*$/.test(value.promotion.alias)) fail(`${source}.promotion.alias`, 'must be an L1-style alias');
  if (!['candidate', 'promoted', 'retired'].includes(value.promotion.status)) {
    fail(`${source}.promotion.status`, 'must be candidate, promoted, or retired');
  }
  return value;
}

function verifyEvidence(entries, stackBenchRoot, at) {
  return entries.map((entry, index) => {
    const ref = contained(stackBenchRoot, stackBenchRoot, entry.path, `${at}[${index}].path`);
    const digest = sha256(readFileSync(ref.absolute));
    if (digest !== entry.sha256) fail(`${at}[${index}].sha256`, `stale digest for ${entry.path}`);
    return { ...entry, path: ref.relative, sha256: digest };
  });
}

function evidenceFailure(at, message) {
  fail(at, `qualification artifact ${message}`);
}

function exactEvidenceIdentity(actual, expected, at) {
  if (!actual || actual.id !== expected.id || actual.version !== expected.version
    || actual.sha256 !== expected.sha256) {
    evidenceFailure(at, `has wrong ${at.split('.').at(-1)} identity`);
  }
}

export function currentLevelPoints(release, execution) {
  if (!Array.isArray(execution) || execution.length === 0) {
    throw new Error('qualification requires a typed execution plan');
  }
  const ownership = new Map();
  for (const entry of execution) {
    if (typeof entry?.id !== 'string' || !entry.id || ownership.has(entry.id)
      || !['current', 'inherited'].includes(entry.ownership?.kind)) {
      throw new Error('qualification received an invalid typed execution plan');
    }
    ownership.set(entry.id, entry.ownership.kind);
  }
  const selected = new Set();
  let points = 0;
  for (const check of release.checkCatalog) {
    const kind = ownership.get(check.executionId);
    if (!kind) throw new Error(`typed execution plan is missing ${check.executionId}`);
    selected.add(check.executionId);
    if (kind === 'current') points += check.points;
  }
  const empty = [...ownership.keys()].filter(id => !selected.has(id));
  if (empty.length) throw new Error(`typed execution plan has no checks for ${empty.join(', ')}`);
  return points;
}

export function validateQualificationEvidenceArtifact(artifact, entry,
  { calibration, qualificationIdentity, release, references, execution }) {
  const at = `evidence.${entry.kind}:${entry.stack ?? ''}:${entry.repetition}`;
  if (!artifact || typeof artifact !== 'object') evidenceFailure(at, 'is not an artifact');
  exactEvidenceIdentity(artifact.identities?.recipe,
    { id: release.id, version: release.version, sha256: release.contentSha256 }, `${at}.recipe`);
  exactEvidenceIdentity(artifact.identities?.calibration, qualificationIdentity,
    `${at}.calibration`);
  if (!artifact.identities?.engine?.sha256) evidenceFailure(at, 'has no engine content identity');
  if (calibration.qualification.runner !== undefined) {
    for (const [field, expected] of Object.entries(calibration.qualification.runner)) {
      if (artifact.payload?.runner?.[field] !== expected) {
        evidenceFailure(at, 'has the wrong controller runner environment');
      }
    }
    const missing = missingRunnerObservation(artifact.payload?.runner);
    if (missing.length) evidenceFailure(at,
      `has no complete appliance runner observation (missing ${missing.join(', ')})`);
  }

  const scoredChecks = release.checkCatalog.filter(check => check.points > 0);
  const zeroPointChecks = release.checkCatalog.filter(check => check.points === 0);
  if (entry.kind === 'null') {
    if (artifact.kind !== 'null_control') evidenceFailure(at, `is ${artifact.kind}, not null_control`);
    const payload = artifact.payload;
    if (payload.ok !== true) evidenceFailure(at, 'did not pass');
    if (canonicalDefinitionJson(payload.tracks) !== canonicalDefinitionJson([release.track])) {
      evidenceFailure(at, 'targets another track');
    }
    const summary = payload.summary;
    const expectedPoints = scoredChecks.reduce((sum, check) => sum + check.points, 0);
    if (summary?.criteria !== scoredChecks.length || summary?.points !== expectedPoints
      || summary.expectedFailures?.criteria !== scoredChecks.length
      || summary.expectedFailures?.points !== expectedPoints
      || summary.vacuousPasses?.criteria !== 0 || summary.vacuousPasses?.points !== 0
      || summary.oracleGaps?.criteria !== 0 || summary.oracleGaps?.points !== 0
      || summary.unscored?.criteria !== zeroPointChecks.length
      || summary.unscored?.passed !== 0 || summary.unscored?.failed !== zeroPointChecks.length
      || summary.unscored?.inconclusive !== 0) {
      evidenceFailure(at, 'does not prove the complete null policy');
    }
    const expected = new Map(scoredChecks.map(check => [
      `${check.source}:${check.featureId}:${check.criterionId}`, check,
    ]));
    const qualificationLevel = Number(calibration.promotion.alias.slice(1));
    for (const result of payload.criteria ?? []) {
      const key = `${result.scenario}:${result.feature}:${result.criterion}`;
      const check = expected.get(key);
      if (!check || result.track !== release.track || result.level !== qualificationLevel
        || result.points !== check.points || result.status !== 'expected_fail'
        || result.evidenceStatus !== 'failed') {
        evidenceFailure(at, `contains invalid null result ${key}`);
      }
      expected.delete(key);
    }
    if (expected.size !== 0) evidenceFailure(at, 'does not cover every selected check exactly once');
    return;
  }

  if (artifact.kind !== 'reference_qualification') {
    evidenceFailure(at, `is ${artifact.kind}, not reference_qualification`);
  }
  const reference = references.find(candidate => candidate.backend === entry.stack);
  if (!reference) evidenceFailure(at, `targets undeclared stack ${entry.stack}`);
  exactEvidenceIdentity(artifact.identities?.fixture,
    { id: reference.id, version: null, sha256: reference.sourceSha256 }, `${at}.fixture`);
  if (artifact.identities?.stackAdapter?.id !== entry.stack) evidenceFailure(at, 'has wrong stack adapter');
  const payload = artifact.payload;
  const mutationControl = entry.kind === 'mutation';
  const requiredRepetitions = mutationControl
    ? calibration.qualification.mutationRepetitions : calibration.qualification.referenceRepetitions;
  if (payload.fixture !== reference.id || payload.fixtureSha256 !== reference.sourceSha256
    || payload.requiredRepetitions !== requiredRepetitions || payload.isolation !== 'docker'
    || payload.mutationControl !== mutationControl || payload.ok !== true
    || payload.stable !== true || payload.sameImage !== true || payload.sameHarness !== true
    || !payload.harnessSha256 || payload.runs?.length !== requiredRepetitions) {
    evidenceFailure(at, 'does not satisfy the repeated Docker gate');
  }
  const repetitions = new Set();
  const expectedRunScore = currentLevelPoints(release, execution);
  for (const run of payload.runs) {
    repetitions.add(run.repetition);
    if (run.ok !== true || run.processError !== null || run.outcome !== 'passed'
      || run.score !== `${expectedRunScore}/${expectedRunScore}`
      || run.criteria !== release.scoring.checks || run.zeroPointCriteria !== zeroPointChecks.length
      || !run.imageId || run.harnessSha256Before !== payload.harnessSha256
      || run.harnessSha256After !== payload.harnessSha256 || run.failures?.length !== 0
      || !Array.isArray(run.packRuntime?.packs)
      || run.packRuntime.packs.length !== release.components.packs.length
      || run.packRuntime.packs.some(pack => pack.exceeded !== false)) {
      evidenceFailure(at, `contains failed or incomplete repetition ${run.repetition}`);
    }
    if (mutationControl) {
      if (!run.mutations || run.mutations.total < 1 || run.mutations.caught !== run.mutations.total) {
        evidenceFailure(at, `did not catch every mutation in repetition ${run.repetition}`);
      }
    } else if (run.mutations !== null) {
      evidenceFailure(at, `reference repetition ${run.repetition} contains mutation results`);
    }
  }
  if (repetitions.size !== requiredRepetitions
    || !Array.from({ length: requiredRepetitions }, (_, index) => index + 1)
      .every(repetition => repetitions.has(repetition))) {
    evidenceFailure(at, 'does not contain the exact repetition set');
  }
  if (!repetitions.has(entry.repetition)) evidenceFailure(at, 'does not contain its declared repetition');
}

function verifyQualificationEvidence(entries, stackBenchRoot, at, context) {
  const artifacts = new Map();
  const normalized = verifyEvidence(entries, stackBenchRoot, at);
  const engines = new Set();
  const harnesses = new Set();
  const images = new Set();
  const runners = new Set();
  normalized.forEach((entry, index) => {
    let artifact = artifacts.get(entry.path);
    if (!artifact) {
      try { artifact = readArtifact(resolve(stackBenchRoot, entry.path)); }
      catch (error) { fail(`${at}[${index}].path`, `is not a valid artifact: ${error.message}`); }
      artifacts.set(entry.path, artifact);
    }
    validateQualificationEvidenceArtifact(artifact, entry, context);
    engines.add(artifact.identities.engine.sha256);
    if (context.calibration.qualification.runner !== undefined) {
      runners.add(canonicalDefinitionJson(artifact.payload.runner));
    }
    if (entry.kind !== 'null') {
      harnesses.add(artifact.payload.harnessSha256);
      for (const run of artifact.payload.runs) images.add(run.imageId);
    }
  });
  if (engines.size > 1) fail(at, 'qualification artifacts were produced by different engines');
  if (harnesses.size > 1) fail(at, 'qualification artifacts use different harnesses');
  if (images.size > 1) fail(at, 'qualification artifacts use different build images');
  if (runners.size > 1) fail(at, 'qualification artifacts use different appliance runner environments');
  return normalized;
}

function mutationExecutionSha256(manifest) {
  const { status: _status, ...execution } = manifest;
  return sha256(canonicalDefinitionJson(canonicalizeDefinition(execution)));
}

export function calibrationQualificationIdentity(calibration) {
  if (!calibration || typeof calibration !== 'object') {
    throw new Error('calibration qualification identity requires a compiled calibration');
  }
  const document = canonicalizeDefinition({
    schemaVersion: CALIBRATION_SCHEMA_VERSION,
    id: calibration.id,
    version: calibration.version,
    track: calibration.track,
    recipe: {
      id: calibration.recipe?.id,
      version: calibration.recipe?.version,
      meaningSha256: calibration.recipe?.meaningSha256,
      executionSha256: calibration.recipe?.executionSha256,
      contentSha256: calibration.recipe?.contentSha256,
    },
    fixture: { id: calibration.fixture?.id, version: calibration.fixture?.version },
    references: (calibration.references?.entries ?? []).map(entry => ({
      backend: entry.backend, id: entry.id, sourceSha256: entry.sourceSha256,
    })).sort((a, b) => a.backend.localeCompare(b.backend)),
    mutations: (calibration.mutations ?? []).map(entry => ({
      backend: entry.backend, referenceId: entry.referenceId,
      executionSha256: entry.executionSha256,
    })).sort((a, b) => a.backend.localeCompare(b.backend)),
    nullControl: calibration.nullControl,
    controls: (calibration.controls ?? []).map(control => ({ ...control,
      mutationTargets: [...control.mutationTargets].sort(),
    })).sort((a, b) => a.stableKey.localeCompare(b.stableKey)),
    qualification: {
      exactCombinationRequired: calibration.qualification?.exactCombinationRequired,
      referenceRepetitions: calibration.qualification?.referenceRepetitions,
      mutationRepetitions: calibration.qualification?.mutationRepetitions,
      ...(calibration.qualification?.runner ? { runner: calibration.qualification.runner } : {}),
      stacks: (calibration.qualification?.stacks ?? []).map(stack => ({
        id: stack.id, supported: stack.status !== 'unsupported',
      })).sort((a, b) => a.id.localeCompare(b.id)),
    },
    equivalenceDecisions: calibration.equivalenceDecisions,
  });
  return { id: calibration.id, version: calibration.version,
    sha256: sha256(canonicalDefinitionJson(document)) };
}

export function compileCalibrationFile(calibrationPath, { trackRoot, stackBenchRoot, release } = {}) {
  if (!release) throw new Error('calibration compilation requires the resolved recipe release');
  const root = realpathSync(resolve(trackRoot));
  const benchRoot = realpathSync(resolve(stackBenchRoot ?? resolve(root, '..', '..')));
  const calibrationRef = contained(root, root, calibrationPath, 'calibration path');
  const absolute = calibrationRef.absolute;
  const source = relative(root, absolute).replaceAll('\\', '/');
  const calibration = compileCalibrationDefinition(readJson(absolute, 'calibration'), { source });
  if (calibration.track !== release.track) fail(`${source}.track`, `expected ${release.track}`);
  for (const field of ['id', 'version', 'meaningSha256', 'executionSha256', 'contentSha256']) {
    if (calibration.recipe[field] !== release[field]) {
      fail(`${source}.recipe.${field}`, `does not match resolved recipe ${release.id}@${release.version}`);
    }
  }
  const recipeRef = contained(root, dirname(absolute), calibration.recipe.path, `${source}.recipe.path`);
  if (!release.sourceManifest.some(entry => entry.path === recipeRef.relative && entry.kinds.includes('recipe'))) {
    fail(`${source}.recipe.path`, 'does not name the resolved recipe source');
  }
  const execution = executionPlanForRelease(recipeRef.absolute, {
    trackRoot: root,
    level: Number(calibration.promotion.alias.slice(1)),
  });
  if (calibration.fixture.id !== release.components.fixture.id
    || calibration.fixture.version !== release.components.fixture.version) {
    fail(`${source}.fixture`, 'does not match the resolved fixture identity');
  }
  if (release.components.fixture.sha256 !== calibration.fixture.sourceSha256) {
    fail(`${source}.fixture.sourceSha256`, 'does not match the resolved fixture source');
  }

  const registryRef = contained(benchRoot, benchRoot, calibration.references.registryPath,
    `${source}.references.registryPath`);
  const registry = loadReferenceRegistry(registryRef.absolute);
  const registryValidation = validateReferenceRegistry(registry, { root: benchRoot });
  if (!registryValidation.ok) fail(`${source}.references`, registryValidation.issues.join('; '));
  const references = calibration.references.entries.map((selection, index) => {
    const entry = registry.fixtures.find(candidate => candidate.id === selection.id);
    const at = `${source}.references.entries[${index}]`;
    if (!entry) fail(`${at}.id`, 'is absent from the reference registry');
    if (entry.backend !== selection.backend || entry.track !== release.track) {
      fail(at, 'targets a different backend or track');
    }
    if (entry.imported?.sourceSha256 !== selection.sourceSha256) fail(`${at}.sourceSha256`, 'is stale');
    return {
      ...selection,
      status: entry.status,
      ...(entry.targetPath ? { targetPath: entry.targetPath } : {}),
    };
  });

  const stableByLegacyKey = new Map(release.checkCatalog.map(check => [
    `${check.source}:${check.featureId}:${check.criterionId}`, check.stableKey,
  ]));
  const mutationTargetRefs = new Map();
  const mutations = calibration.mutations.map((selection, index) => {
    const at = `${source}.mutations[${index}]`;
    const ref = contained(benchRoot, benchRoot, selection.path, `${at}.path`);
    const digest = sha256(readFileSync(ref.absolute));
    if (digest !== selection.sha256) fail(`${at}.sha256`, `stale digest for ${selection.path}`);
    const manifest = readJson(ref.absolute, 'mutation manifest');
    const reference = references.find(candidate => candidate.id === selection.referenceId);
    if (!reference || reference.backend !== selection.backend) fail(`${at}.referenceId`, 'does not match mutation backend');
    if (manifest.backend !== selection.backend || manifest.track !== release.track) {
      fail(at, 'mutation manifest targets another benchmark');
    }
    if (manifest.fixtureSha256 !== reference.sourceSha256) fail(at, 'mutation fixture hash does not match its reference');
    const definitions = validateMutationDefinitions(manifest.mutations,
      { defaultScenario: manifest.scenario, requireScenario: true });
    if (!definitions.ok) fail(at, `invalid mutation definitions: ${definitions.issues.map(issue => issue.kind).join(', ')}`);
    const targets = [];
    for (const mutation of manifest.mutations) {
      const scenario = mutationScenario(manifest, mutation).replaceAll('\\', '/')
        .replace(new RegExp(`^tracks/${release.track}/`), '');
      const stableKeys = mutationTargetKeys(mutation).map(key => {
        const split = key.indexOf(':');
        const stable = stableByLegacyKey.get(`${scenario}:${key.slice(0, split)}:${key.slice(split + 1)}`);
        if (!stable) fail(at, `mutation ${mutation.id} targets unknown recipe check ${key}`);
        return stable;
      });
      const targetRef = `${selection.backend}:${mutation.id}`;
      mutationTargetRefs.set(targetRef, new Set(stableKeys));
      targets.push({ id: mutation.id, stableKeys });
    }
    return { ...selection, path: ref.relative, status: manifest.status,
      executionSha256: mutationExecutionSha256(manifest), targets };
  });

  const zeroPoint = new Set(release.checkCatalog.filter(check => check.points === 0)
    .map(check => check.stableKey));
  const controls = calibration.controls.map((control, index) => {
    const at = `${source}.controls[${index}]`;
    if (!zeroPoint.delete(control.stableKey)) fail(`${at}.stableKey`, 'is not an unassigned zero-point check');
    for (const target of control.mutationTargets) {
      const stableKeys = mutationTargetRefs.get(target);
      if (!stableKeys) fail(`${at}.mutationTargets`, `unknown mutation ${target}`);
      if (!stableKeys.has(control.stableKey)) fail(`${at}.mutationTargets`, `${target} does not target this check`);
    }
    return control;
  });
  if (zeroPoint.size) fail(`${source}.controls`, `missing zero-point checks: ${[...zeroPoint].sort().join(', ')}`);

  const qualificationIdentity = calibrationQualificationIdentity({
    ...calibration,
    references: { ...calibration.references, entries: references },
    mutations,
  });
  const evidence = verifyQualificationEvidence(calibration.qualification.evidence, benchRoot,
    `${source}.qualification.evidence`, {
      calibration,
      qualificationIdentity,
      release,
      references,
      execution,
    });
  const equivalenceDecisions = calibration.equivalenceDecisions.map((decision, index) => ({
    ...decision,
    evidence: verifyEvidence(decision.evidence, benchRoot, `${source}.equivalenceDecisions[${index}].evidence`),
  }));
  const catalogRef = contained(root, root, calibration.promotion.catalogPath,
    `${source}.promotion.catalogPath`);
  const catalogDigest = sha256(readFileSync(catalogRef.absolute));
  if (catalogDigest !== calibration.promotion.catalogSha256) fail(`${source}.promotion.catalogSha256`, 'catalog digest is stale');
  const catalog = compilePromotionFile(catalogRef.absolute, { trackRoot: root });
  const aliasEntries = catalog.entries.filter(entry => entry.alias === calibration.promotion.alias
    && entry.status === calibration.promotion.status);
  if (!aliasEntries.some(entry => entry.recipe.id === release.id && entry.recipe.version === release.version)) {
    fail(`${source}.promotion`, 'does not resolve to this recipe at the declared status');
  }
  if (calibration.promotion.status === 'promoted' && calibration.state !== 'qualified') {
    fail(`${source}.promotion.status`, 'a promoted alias requires a qualified calibration');
  }

  if (calibration.state === 'qualified') {
    if (release.state !== 'qualified') fail(`${source}.state`, 'cannot qualify a draft or retired recipe');
    if (references.some(entry => entry.status !== 'active')) fail(`${source}.references`, 'qualified calibration requires active references');
    if (mutations.some(entry => entry.status !== 'active')) fail(`${source}.mutations`, 'qualified calibration requires active mutations');
    if (calibration.qualification.stacks.some(stack => stack.status !== 'qualified'
      && stack.status !== 'unsupported')) {
      fail(`${source}.qualification.stacks`, 'every supported stack must be qualified');
    }
    if (evidence.length === 0) fail(`${source}.qualification.evidence`, 'qualified calibration requires evidence');
  }

  const supportedStacks = new Set(calibration.qualification.stacks
    .filter(stack => stack.status !== 'unsupported').map(stack => stack.id));
  const referenceStacks = new Set(references.map(entry => entry.backend));
  const mutationStacks = new Set(mutations.map(entry => entry.backend));
  for (const stack of supportedStacks) {
    if (!referenceStacks.has(stack)) fail(`${source}.references`, `missing supported stack ${stack}`);
    if (!mutationStacks.has(stack)) fail(`${source}.mutations`, `missing supported stack ${stack}`);
  }
  for (const stack of referenceStacks) {
    if (!supportedStacks.has(stack)) fail(`${source}.references`, `undeclared or unsupported stack ${stack}`);
  }
  for (const stack of mutationStacks) {
    if (!supportedStacks.has(stack)) fail(`${source}.mutations`, `undeclared or unsupported stack ${stack}`);
  }
  if (calibration.state === 'qualified') {
    const expectedEvidence = [];
    for (const stack of supportedStacks) {
      for (let repetition = 1; repetition <= calibration.qualification.referenceRepetitions; repetition += 1) {
        expectedEvidence.push(`reference:${stack}:${repetition}`);
      }
      for (let repetition = 1; repetition <= calibration.qualification.mutationRepetitions; repetition += 1) {
        expectedEvidence.push(`mutation:${stack}:${repetition}`);
      }
    }
    for (let repetition = 1; repetition <= calibration.nullControl.repetitions; repetition += 1) {
      expectedEvidence.push(`null::${repetition}`);
    }
    const actualEvidence = evidence.map(entry => `${entry.kind}:${entry.stack ?? ''}:${entry.repetition}`);
    if (new Set(actualEvidence).size !== actualEvidence.length
      || canonicalDefinitionJson([...actualEvidence].sort()) !== canonicalDefinitionJson(expectedEvidence.sort())) {
      fail(`${source}.qualification.evidence`, 'must contain exactly the declared reference, mutation, and null repetitions');
    }
  }

  const plan = canonicalizeDefinition({
    calibrationSchemaVersion: CALIBRATION_SCHEMA_VERSION,
    id: calibration.id,
    version: calibration.version,
    state: calibration.state,
    title: calibration.title,
    track: calibration.track,
    recipe: { ...calibration.recipe, path: recipeRef.relative },
    fixture: calibration.fixture,
    references: { registryPath: registryRef.relative,
      entries: references.sort((a, b) => a.backend.localeCompare(b.backend)) },
    mutations: mutations.map(entry => ({ ...entry,
      targets: entry.targets.map(target => ({ ...target, stableKeys: [...target.stableKeys].sort() }))
        .sort((a, b) => a.id.localeCompare(b.id)),
    })).sort((a, b) => a.backend.localeCompare(b.backend)),
    nullControl: calibration.nullControl,
    controls: controls.map(control => ({ ...control,
      mutationTargets: [...control.mutationTargets].sort() }))
      .sort((a, b) => a.stableKey.localeCompare(b.stableKey)),
    qualification: { ...calibration.qualification,
      stacks: [...calibration.qualification.stacks].sort((a, b) => a.id.localeCompare(b.id)),
      evidence: evidence.sort((a, b) => `${a.kind}:${a.stack ?? ''}:${a.repetition}`
        .localeCompare(`${b.kind}:${b.stack ?? ''}:${b.repetition}`)) },
    equivalenceDecisions: equivalenceDecisions.sort((a, b) =>
      `${a.fromExecutionSha256}:${a.toExecutionSha256}`
        .localeCompare(`${b.fromExecutionSha256}:${b.toExecutionSha256}`)),
    promotion: { ...calibration.promotion, catalogPath: catalogRef.relative },
  });
  const qualificationSha256 = calibrationQualificationIdentity(plan).sha256;
  return { ...plan, qualificationSha256,
    contentSha256: sha256(canonicalDefinitionJson({ ...plan, qualificationSha256 })) };
}

export function resolveCalibrationForRelease(release, { trackRoot, stackBenchRoot } = {}) {
  if (!release) return null;
  const root = realpathSync(resolve(trackRoot));
  const directory = join(root, 'composition', 'calibrations');
  if (!existsSync(directory)) return null;
  const matches = [];
  for (const name of readdirSync(directory).filter(file => file.endsWith('.json')).sort()) {
    const path = join(directory, name);
    const raw = readJson(path, 'calibration');
    if (raw.recipe?.id !== release.id || raw.recipe?.version !== release.version
      || raw.recipe?.contentSha256 !== release.contentSha256) continue;
    matches.push(compileCalibrationFile(path, { trackRoot: root, stackBenchRoot, release }));
  }
  if (matches.length > 1) {
    throw new Error(`multiple calibrations match ${release.id}@${release.version} (${release.contentSha256})`);
  }
  return matches[0] ?? null;
}
