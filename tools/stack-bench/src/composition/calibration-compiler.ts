import { existsSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

import { compileRecipeSelectionFile } from './composition-compiler.js';
import { canonicalDefinitionJson, canonicalizeDefinition, readDefinitionJson }
  from './definition-plan.js';
import { mutationTargetKeys, validateMutationDefinitions } from '../evidence/mutation-analysis.js';
import type { MutationDefinition } from '../evidence/mutation-analysis.js';
import { sha256 } from '../evidence/provenance.js';
import { loadReferenceRegistry, validateReferenceRegistry } from '../references/reference-fixtures.js';
import { readArtifact } from '../evidence/artifacts.js';
import { executionPlanForRelease } from './recipe-release.js';
import { missingRunnerObservation } from '../runtime/runner-environment.js';
import { qualificationScopeIdentity, validateQualificationScopeIdentity } from './qualification-scope.js';
import type { RecipeCheck, RecipeExecution, RecipeRelease } from './recipe-release.js';
import { resolveFeatureCatalog } from '../progression/feature-catalog-selection.js';
import { progressionLevels, selectFeatureCatalogLevels }
  from '../progression/progression-definition.js';

type UnknownRecord = Record<string, unknown>;

// Each compiler validates a cloned record field by field and returns that same
// record. The intersection is what lets the proven record be read back as the
// shape it was proven to be.
type Validated<T> = T & UnknownRecord;

const isOneOf = (value: unknown, allowed: readonly string[]): boolean =>
  typeof value === 'string' && allowed.includes(value);

export interface CalibrationReference {
  backend: string;
  id: string;
  sourceSha256: string;
  targetPath?: string;
}

export interface CalibrationMutation {
  backend: string;
  path: string;
  sha256: string;
  referenceId: string;
  executionSha256?: string;
  targets: Array<{ id: string; stableKeys: string[] }>;
}

export interface CalibrationEvidence {
  kind: 'reference' | 'mutation' | 'null';
  stack?: string;
  repetition: number;
  path: string;
  sha256: string;
}

export interface CalibrationControl {
  stableKey: string;
  role: string;
  promotionPolicy: string;
  mutationTargets: string[];
  reason?: string;
}

export interface CalibrationDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  title: string;
  track: string;
  recipe: {
    path: string;
    id: string;
    meaningSha256: string;
    executionSha256: string;
    contentSha256: string;
  };
  fixture: { id: string; sourceSha256: string };
  references: { registryPath: string; entries: CalibrationReference[] };
  mutations: CalibrationMutation[];
  nullControl: { pointBearing: string; zeroPoint: string; repetitions: number };
  controls: CalibrationControl[];
  qualification: {
    exactCombinationRequired: boolean;
    referenceRepetitions: number;
    mutationRepetitions: number;
    checks?: string[];
    featureCatalog?: { path: string; id: string; contentSha256: string };
    runner?: { schemaVersion: number; mode: string; platform: string; architecture: string };
    stacks: string[];
    evidence: CalibrationEvidence[];
    buildImage?: string;
  };
  equivalenceDecisions: Array<{
    fromExecutionSha256: string;
    toExecutionSha256: string;
    rationale: string;
    evidence: Array<{ path: string; sha256: string }>;
  }>;
  qualificationReuse?: {
    sourceRecipe: { id: string; contentSha256: string; executionSha256: string };
    sourceCalibration: { id: string; contentSha256: string };
    rationale: string;
    evidence: Array<{ path: string; sha256: string }>;
    scopes: Array<{
      kind: 'reference' | 'mutation' | 'null';
      stack?: string;
      fromExecutableSha256: string;
      toExecutableSha256: string;
    }>;
  };
  selection: {
    path: string;
    sha256: string;
    alias: string;
    coveredAliases?: string[];
  };
}

export interface QualificationStaleness {
  kind: string;
  stack: string | null;
  repetition: number;
  path: string;
  reason: string;
}

export interface CalibrationPlan extends Omit<CalibrationDefinition, 'schemaVersion' | 'kind'> {
  calibrationSchemaVersion: number;
  contentSha256: string;
  qualificationSha256: string;
  qualificationStaleness: QualificationStaleness[];
}

export interface CalibrationContext {
  // Evidence is verified while the calibration is still being compiled, before
  // it carries its plan hashes.
  calibration: CalibrationDefinition | CalibrationPlan;
  qualificationIdentity: CalibrationIdentity;
  release: RecipeRelease;
  references: CalibrationReference[];
  execution: RecipeExecution[];
  stackBenchRoot: string;
}

export interface CalibrationIdentity {
  id: string;
  contentSha256: string;
}

export const CALIBRATION_SCHEMA_VERSION = 2;

const HASH = /^[a-f0-9]{64}$/;
const ID = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
const CONTROL_POLICIES = new Map([
  ['promotion-gate', 'must-pass-reference-and-kill-declared-mutant'],
  ['precondition', 'must-pass-reference'],
  ['diagnostic', 'record-only-until-calibrated'],
]);

const isObject = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
function fail(at: string, message: string): never {
  throw new Error(`invalid calibration at ${at}: ${message}`);
}

function strictObject(value: unknown, at: string,
  fields: Set<string>): asserts value is UnknownRecord {
  if (!isObject(value)) fail(at, 'must be an object');
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`${at}.${key}`, 'unknown field');
}

function string(value: unknown, at: string): string {
  if (typeof value !== 'string' || !value.trim()) fail(at, 'must be a non-empty string');
  return value;
}

function exactHash(value: unknown, at: string): string {
  const text = string(value, at);
  if (!HASH.test(text)) fail(at, 'must be 64 lowercase hexadecimal characters');
  return text;
}

function exactId(value: unknown, at: string): string {
  const text = string(value, at);
  if (!ID.test(text)) fail(at, 'must be a stable lowercase id');
  return text;
}


function array(value: unknown, at: string,
  { nonEmpty = false }: { nonEmpty?: boolean } = {}): unknown[] {
  if (!Array.isArray(value) || (nonEmpty && value.length === 0)) {
    fail(at, `must be a${nonEmpty ? ' non-empty' : 'n'} array`);
  }
  return value;
}

function contained(root: string, from: string, path: unknown,
  at: string): { absolute: string; relative: string } {
  const text = string(path, at);
  const lexicalRoot = resolve(root);
  const candidate = resolve(from, text);
  const rel = relative(lexicalRoot, candidate);
  if (rel === '..' || rel.startsWith(`..${sep}`)) fail(at, `escapes ${lexicalRoot}`);
  if (!existsSync(candidate)) fail(at, `does not exist: ${text}`);
  const realRoot = realpathSync(lexicalRoot);
  const target = realpathSync(candidate);
  const realRel = relative(realRoot, target);
  if (realRel === '..' || realRel.startsWith(`..${sep}`)) fail(at, `escapes ${realRoot}`);
  return { absolute: target, relative: realRel.replaceAll('\\', '/') };
}

const ROOT_FIELDS = new Set([
  'schemaVersion', 'kind', 'id', 'title', 'track', 'recipe',
  'fixture', 'references', 'mutations', 'nullControl', 'controls', 'qualification',
  'equivalenceDecisions', 'qualificationReuse', 'selection',
]);
const RECIPE_FIELDS = new Set([
  'path', 'id', 'meaningSha256', 'executionSha256', 'contentSha256',
]);
const FIXTURE_FIELDS = new Set(['id', 'sourceSha256']);
const REFERENCES_FIELDS = new Set(['registryPath', 'entries']);
const REFERENCE_FIELDS = new Set(['backend', 'id', 'sourceSha256']);
const MUTATION_FIELDS = new Set(['backend', 'path', 'sha256', 'referenceId']);
const NULL_FIELDS = new Set(['pointBearing', 'zeroPoint', 'repetitions']);
const CONTROL_FIELDS = new Set(['stableKey', 'role', 'promotionPolicy', 'mutationTargets', 'reason']);
const QUALIFICATION_FIELDS = new Set([
  'exactCombinationRequired', 'referenceRepetitions', 'mutationRepetitions', 'checks',
  'featureCatalog', 'runner',
  'stacks', 'evidence',
]);
const FEATURE_CATALOG_FIELDS = new Set(['path', 'id', 'contentSha256']);
const RUNNER_FIELDS = new Set(['schemaVersion', 'mode', 'platform', 'architecture']);
const EVIDENCE_FIELDS = new Set(['kind', 'stack', 'repetition', 'path', 'sha256']);
const EQUIVALENCE_EVIDENCE_FIELDS = new Set(['path', 'sha256']);
const EQUIVALENCE_FIELDS = new Set([
  'fromExecutionSha256', 'toExecutionSha256', 'rationale', 'evidence',
]);
const QUALIFICATION_REUSE_FIELDS = new Set([
  'sourceRecipe', 'sourceCalibration', 'rationale', 'evidence', 'scopes',
]);
const QUALIFICATION_REUSE_RECIPE_FIELDS = new Set([
  'id', 'contentSha256', 'executionSha256',
]);
const QUALIFICATION_REUSE_CALIBRATION_FIELDS = new Set(['id', 'contentSha256']);
const QUALIFICATION_REUSE_SCOPE_FIELDS = new Set([
  'kind', 'stack', 'fromExecutableSha256', 'toExecutableSha256',
]);
const SELECTION_FIELDS = new Set([
  'path', 'sha256', 'alias', 'coveredAliases',
]);

export function compileCalibrationDefinition(input: unknown,
  { source = '<calibration>' }: { source?: string } = {}): CalibrationDefinition {
  const value = structuredClone(input);
  strictObject(value, source, ROOT_FIELDS);
  if (value.schemaVersion !== CALIBRATION_SCHEMA_VERSION) fail(`${source}.schemaVersion`, 'must be 2');
  if (value.kind !== 'calibration-manifest') fail(`${source}.kind`, 'must be "calibration-manifest"');
  exactId(value.id, `${source}.id`);
  string(value.title, `${source}.title`);
  exactId(value.track, `${source}.track`);

  const recipe = value.recipe;
  strictObject(recipe, `${source}.recipe`, RECIPE_FIELDS);
  string(recipe.path, `${source}.recipe.path`);
  exactId(recipe.id, `${source}.recipe.id`);
  for (const [field, hash] of [['meaningSha256', recipe.meaningSha256],
    ['executionSha256', recipe.executionSha256],
    ['contentSha256', recipe.contentSha256]] as const) {
    exactHash(hash, `${source}.recipe.${field}`);
  }

  const fixture = value.fixture;
  strictObject(fixture, `${source}.fixture`, FIXTURE_FIELDS);
  exactId(fixture.id, `${source}.fixture.id`);
  exactHash(fixture.sourceSha256, `${source}.fixture.sourceSha256`);

  const references = value.references;
  strictObject(references, `${source}.references`, REFERENCES_FIELDS);
  string(references.registryPath, `${source}.references.registryPath`);
  const referenceEntries = array(references.entries, `${source}.references.entries`,
    { nonEmpty: true });
  const referenceIds = new Set<unknown>();
  const backends = new Set<unknown>();
  referenceEntries.forEach((entry: unknown, index: number) => {
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

  const mutations = array(value.mutations, `${source}.mutations`, { nonEmpty: true });
  const mutationBackends = new Set<unknown>();
  mutations.forEach((entry: unknown, index: number) => {
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

  const nullControl = value.nullControl;
  strictObject(nullControl, `${source}.nullControl`, NULL_FIELDS);
  if (nullControl.pointBearing !== 'must-fail-conclusively') {
    fail(`${source}.nullControl.pointBearing`, 'must be must-fail-conclusively');
  }
  if (nullControl.zeroPoint !== 'typed-policy') {
    fail(`${source}.nullControl.zeroPoint`, 'must be typed-policy');
  }
  if (!Number.isInteger(nullControl.repetitions) || Number(nullControl.repetitions) < 1) {
    fail(`${source}.nullControl.repetitions`, 'must be a positive integer');
  }

  const controls = array(value.controls, `${source}.controls`);
  const controlKeys = new Set<unknown>();
  controls.forEach((control: unknown, index: number) => {
    const at = `${source}.controls[${index}]`;
    strictObject(control, at, CONTROL_FIELDS);
    string(control.stableKey, `${at}.stableKey`);
    const role = String(control.role);
    if (!CONTROL_POLICIES.has(role)) fail(`${at}.role`, 'has an unknown control role');
    if (control.promotionPolicy !== CONTROL_POLICIES.get(role)) {
      fail(`${at}.promotionPolicy`, `must be ${CONTROL_POLICIES.get(role)} for ${role}`);
    }
    const mutationTargets = array(control.mutationTargets ?? [], `${at}.mutationTargets`);
    control.mutationTargets = mutationTargets;
    mutationTargets.forEach((target, targetIndex) => string(target, `${at}.mutationTargets[${targetIndex}]`));
    if (new Set(mutationTargets).size !== mutationTargets.length) {
      fail(`${at}.mutationTargets`, 'must not contain duplicates');
    }
    if (role === 'promotion-gate' && mutationTargets.length === 0) {
      fail(`${at}.mutationTargets`, 'promotion-gate controls require a declared mutant');
    }
    if (role !== 'promotion-gate' && mutationTargets.length > 0) {
      fail(`${at}.mutationTargets`, 'is allowed only for promotion-gate controls');
    }
    if (control.reason !== undefined) string(control.reason, `${at}.reason`);
    if (role === 'diagnostic' && control.reason === undefined) {
      fail(`${at}.reason`, 'is required for diagnostic controls');
    }
    if (controlKeys.has(control.stableKey)) fail(`${at}.stableKey`, `duplicates ${control.stableKey}`);
    controlKeys.add(control.stableKey);
  });

  const qualification = value.qualification;
  strictObject(qualification, `${source}.qualification`, QUALIFICATION_FIELDS);
  if (qualification.exactCombinationRequired !== true) {
    fail(`${source}.qualification.exactCombinationRequired`, 'must be true');
  }
  for (const [field, repetitions] of [['referenceRepetitions', qualification.referenceRepetitions],
    ['mutationRepetitions', qualification.mutationRepetitions]] as const) {
    if (!Number.isInteger(repetitions) || Number(repetitions) < 1) {
      fail(`${source}.qualification.${field}`, 'must be a positive integer');
    }
  }
  if (qualification.checks !== undefined) {
    const checks = array(qualification.checks, `${source}.qualification.checks`, { nonEmpty: true });
    checks.forEach((key, index) => string(key, `${source}.qualification.checks[${index}]`));
    if (new Set(checks).size !== checks.length) {
      fail(`${source}.qualification.checks`, 'must not contain duplicates');
    }
  }
  const featureCatalog = qualification.featureCatalog;
  if (featureCatalog !== undefined) {
    const at = `${source}.qualification.featureCatalog`;
    strictObject(featureCatalog, at, FEATURE_CATALOG_FIELDS);
    string(featureCatalog.path, `${at}.path`);
    string(featureCatalog.id, `${at}.id`);
    exactHash(featureCatalog.contentSha256, `${at}.contentSha256`);
  }
  const runner = qualification.runner;
  if (runner !== undefined) {
    const at = `${source}.qualification.runner`;
    strictObject(runner, at, RUNNER_FIELDS);
    if (runner.schemaVersion !== 1) fail(`${at}.schemaVersion`, 'must be 1');
    if (runner.mode !== 'appliance') fail(`${at}.mode`, 'must be appliance');
    string(runner.platform, `${at}.platform`);
    string(runner.architecture, `${at}.architecture`);
  }
  const stacks = array(qualification.stacks, `${source}.qualification.stacks`, { nonEmpty: true });
  const stackIds = new Set<string>();
  stacks.forEach((stack: unknown, index: number) => {
    const stackId = string(stack, `${source}.qualification.stacks[${index}]`);
    if (stackIds.has(stackId)) fail(`${source}.qualification.stacks[${index}]`, `duplicates ${stackId}`);
    stackIds.add(stackId);
  });
  const evidence = array(qualification.evidence, `${source}.qualification.evidence`);
  evidence.forEach((entry: unknown, index: number) => {
    const at = `${source}.qualification.evidence[${index}]`;
    strictObject(entry, at, EVIDENCE_FIELDS);
    if (!isOneOf(entry.kind, ['reference', 'mutation', 'null'])) {
      fail(`${at}.kind`, 'must be reference, mutation, or null');
    }
    if (!Number.isInteger(entry.repetition) || Number(entry.repetition) < 1) {
      fail(`${at}.repetition`, 'must be a positive integer');
    }
    if (entry.kind === 'null') {
      if (entry.stack !== undefined) fail(`${at}.stack`, 'is not allowed for null evidence');
    } else string(entry.stack, `${at}.stack`);
    string(entry.path, `${at}.path`);
    exactHash(entry.sha256, `${at}.sha256`);
  });

  const equivalenceDecisions = array(value.equivalenceDecisions ?? [],
    `${source}.equivalenceDecisions`);
  value.equivalenceDecisions = equivalenceDecisions;
  equivalenceDecisions.forEach((decision: unknown, index: number) => {
    const at = `${source}.equivalenceDecisions[${index}]`;
    strictObject(decision, at, EQUIVALENCE_FIELDS);
    exactHash(decision.fromExecutionSha256, `${at}.fromExecutionSha256`);
    exactHash(decision.toExecutionSha256, `${at}.toExecutionSha256`);
    if (decision.fromExecutionSha256 === decision.toExecutionSha256) fail(at, 'must compare different execution hashes');
    string(decision.rationale, `${at}.rationale`);
    const decisionEvidence = array(decision.evidence, `${at}.evidence`, { nonEmpty: true });
    decisionEvidence.forEach((entry: unknown, evidenceIndex: number) => {
      strictObject(entry, `${at}.evidence[${evidenceIndex}]`, EQUIVALENCE_EVIDENCE_FIELDS);
      string(entry.path, `${at}.evidence[${evidenceIndex}].path`);
      exactHash(entry.sha256, `${at}.evidence[${evidenceIndex}].sha256`);
    });
  });

  if (value.qualificationReuse !== undefined) {
    const reuse = value.qualificationReuse;
    const at = `${source}.qualificationReuse`;
    strictObject(reuse, at, QUALIFICATION_REUSE_FIELDS);
    const sourceRecipe = reuse.sourceRecipe;
    strictObject(sourceRecipe, `${at}.sourceRecipe`, QUALIFICATION_REUSE_RECIPE_FIELDS);
    exactId(sourceRecipe.id, `${at}.sourceRecipe.id`);
    exactHash(sourceRecipe.contentSha256, `${at}.sourceRecipe.contentSha256`);
    exactHash(sourceRecipe.executionSha256, `${at}.sourceRecipe.executionSha256`);
    const sourceCalibration = reuse.sourceCalibration;
    strictObject(sourceCalibration, `${at}.sourceCalibration`,
      QUALIFICATION_REUSE_CALIBRATION_FIELDS);
    exactId(sourceCalibration.id, `${at}.sourceCalibration.id`);
    exactHash(sourceCalibration.contentSha256, `${at}.sourceCalibration.contentSha256`);
    string(reuse.rationale, `${at}.rationale`);
    const reuseEvidence = array(reuse.evidence, `${at}.evidence`, { nonEmpty: true });
    reuseEvidence.forEach((entry: unknown, index: number) => {
      const evidenceAt = `${at}.evidence[${index}]`;
      strictObject(entry, evidenceAt, EQUIVALENCE_EVIDENCE_FIELDS);
      string(entry.path, `${evidenceAt}.path`);
      exactHash(entry.sha256, `${evidenceAt}.sha256`);
    });
    const scopes = array(reuse.scopes, `${at}.scopes`, { nonEmpty: true });
    const scopeKeys = new Set<string>();
    scopes.forEach((scope: unknown, index: number) => {
      const scopeAt = `${at}.scopes[${index}]`;
      strictObject(scope, scopeAt, QUALIFICATION_REUSE_SCOPE_FIELDS);
      if (!isOneOf(scope.kind, ['reference', 'mutation', 'null'])) {
        fail(`${scopeAt}.kind`, 'must be reference, mutation, or null');
      }
      if (scope.kind === 'null') {
        if (scope.stack !== undefined) fail(`${scopeAt}.stack`, 'is not allowed for null reuse');
      } else string(scope.stack, `${scopeAt}.stack`);
      exactHash(scope.fromExecutableSha256, `${scopeAt}.fromExecutableSha256`);
      exactHash(scope.toExecutableSha256, `${scopeAt}.toExecutableSha256`);
      if (scope.fromExecutableSha256 === scope.toExecutableSha256) {
        fail(scopeAt, 'must compare different executable hashes');
      }
      const key = `${scope.kind}:${scope.stack ?? ''}`;
      if (scopeKeys.has(key)) fail(scopeAt, `duplicates ${key}`);
      scopeKeys.add(key);
    });
  }

  const selection = value.selection;
  strictObject(selection, `${source}.selection`, SELECTION_FIELDS);
  string(selection.path, `${source}.selection.path`);
  exactHash(selection.sha256, `${source}.selection.sha256`);
  const alias = String(selection.alias);
  if (!/^L[1-9]\d*$/.test(alias)) fail(`${source}.selection.alias`, 'must be an L1-style alias');
  if (selection.coveredAliases !== undefined) {
    const coveredAliases = array(selection.coveredAliases, `${source}.selection.coveredAliases`,
      { nonEmpty: true });
    const covered = new Set<unknown>();
    for (const [index, entry] of coveredAliases.entries()) {
      if (!/^L[1-9]\d*$/.test(String(entry))) {
        fail(`${source}.selection.coveredAliases[${index}]`, 'must be an L1-style alias');
      }
      if (covered.has(entry)) fail(`${source}.selection.coveredAliases`, `duplicates ${entry}`);
      covered.add(entry);
    }
    if (!covered.has(selection.alias)) {
      fail(`${source}.selection.coveredAliases`, `must include ${selection.alias}`);
    }
  }
  return value as Validated<CalibrationDefinition>;
}

function verifyEvidence(entries: unknown, stackBenchRoot: string,
  at: string): CalibrationEvidence[] {
  return array(entries, at).map((entry: unknown, index: number): CalibrationEvidence => {
    if (!isObject(entry)) fail(`${at}[${index}]`, 'must be an object');
    const ref = contained(stackBenchRoot, stackBenchRoot, entry.path, `${at}[${index}].path`);
    const digest = sha256(readFileSync(ref.absolute));
    if (digest !== entry.sha256) fail(`${at}[${index}].sha256`, `stale digest for ${entry.path}`);
    return { ...entry, path: ref.relative, sha256: digest } as Validated<CalibrationEvidence>;
  });
}

function evidenceFailure(at: string, message: string): never {
  fail(at, `qualification artifact ${message}`);
}

export class QualificationEvidenceStaleError extends Error {
  readonly code = 'qualification_evidence_stale';
  readonly at: string;
  readonly reason: string;

  constructor(at: string, reason: string) {
    super(`${at} qualification artifact is stale: ${reason}`);
    this.name = 'QualificationEvidenceStaleError';
    this.at = at;
    this.reason = reason;
  }
}

// Optional chaining over a value the type system knows nothing about: an
// artifact is JSON from disk, so every step may be absent.
const read = (value: unknown, ...path: readonly string[]): unknown => {
  let current = value;
  for (const key of path) {
    if (current === null || current === undefined) return undefined;
    current = Reflect.get(Object(current), key);
  }
  return current;
};

function exactEvidenceIdentity(actual: unknown, expected: unknown, at: string): void {
  if (!actual || read(actual, 'id') !== read(expected, 'id')
    || read(actual, 'contentSha256') !== read(expected, 'contentSha256')) {
    evidenceFailure(at, `has wrong ${at.split('.').at(-1)} identity`);
  }
}

function evidenceIdentityMatches(actual: unknown, expected: unknown): boolean {
  return read(actual, 'id') === read(expected, 'id')
    && read(actual, 'contentSha256') === read(expected, 'contentSha256');
}

function qualificationEvidenceOrigin(artifact: UnknownRecord,
  calibration: CalibrationDefinition | CalibrationPlan, qualificationIdentity: CalibrationIdentity,
  release: RecipeRelease, at: string): 'current' | 'reused' {
  const identities = artifact.identities;
  const currentRecipe = { id: release.id, contentSha256: release.contentSha256 };
  const current = evidenceIdentityMatches(read(identities, 'recipe'), currentRecipe)
    && evidenceIdentityMatches(read(identities, 'calibration'), qualificationIdentity);
  if (current) return 'current';
  const reuse = calibration.qualificationReuse;
  const sourceRecipe = reuse && { id: reuse.sourceRecipe.id,
    contentSha256: reuse.sourceRecipe.contentSha256 };
  if (reuse && evidenceIdentityMatches(read(identities, 'recipe'), sourceRecipe)
    && evidenceIdentityMatches(read(identities, 'calibration'), reuse.sourceCalibration)) {
    return 'reused';
  }
  evidenceFailure(at, 'has mismatched recipe or calibration identities');
}

type ReuseDecision = {
  sourceRecipe: { id: string; contentSha256: string; executionSha256: string };
  sourceCalibration: { id: string; contentSha256: string };
  scopes: Array<{ kind: string; stack?: string;
    fromExecutableSha256: string; toExecutableSha256: string }>;
};

type ReusableScope = {
  schemaVersion?: unknown;
  kind?: unknown;
  executableSha256?: unknown;
  recipe?: unknown;
  checksSha256?: unknown;
  stack?: unknown;
  mutationSha256?: unknown;
};

export function canReuseQualificationScope({ actual, expected, artifact, calibration, entry }: {
  actual: ReusableScope;
  expected: ReusableScope;
  artifact: { identities?: UnknownRecord };
  calibration: { qualificationReuse?: ReuseDecision };
  entry: { kind: string; stack?: string };
}): boolean {
  const reuse = calibration?.qualificationReuse;
  if (!reuse) return false;
  const decision = reuse.scopes.find(scope => scope.kind === entry.kind
    && (scope.stack ?? null) === (entry.stack ?? null));
  if (!decision) return false;
  const sourceScopeRecipe = { id: reuse.sourceRecipe.id,
    contentSha256: reuse.sourceRecipe.contentSha256 };
  const sourceArtifactRecipe = { id: reuse.sourceRecipe.id,
    contentSha256: reuse.sourceRecipe.contentSha256 };
  const stackInputs = (value: unknown) => isObject(value)
    ? { id: value.id, reference: value.reference } : value;
  return actual.schemaVersion === expected.schemaVersion
    && actual.kind === expected.kind
    && actual.executableSha256 === decision.fromExecutableSha256
    && expected.executableSha256 === decision.toExecutableSha256
    && evidenceIdentityMatches(read(artifact.identities, 'recipe'), sourceArtifactRecipe)
    && evidenceIdentityMatches(read(artifact.identities, 'calibration'), reuse.sourceCalibration)
    && canonicalDefinitionJson(actual.recipe) === canonicalDefinitionJson(sourceScopeRecipe)
    && actual.checksSha256 === expected.checksSha256
    && canonicalDefinitionJson(stackInputs(actual.stack))
      === canonicalDefinitionJson(stackInputs(expected.stack))
    && actual.mutationSha256 === expected.mutationSha256;
}

export function currentLevelPoints(
  release: { checkCatalog: Array<Pick<RecipeCheck, 'executionId' | 'points'>> },
  execution: RecipeExecution[]): number {
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

export function hasExactSelectedPackRuntime(
  packRuntime: unknown,
  release: { checkCatalog: Array<{ packId?: string }> }): boolean {
  const packs = read(packRuntime, 'packs');
  if (!Array.isArray(packs)) return false;
  const expected = new Set(release.checkCatalog.map(check => check.packId));
  const observed = new Set(packs.map(pack => read(pack, 'id')));
  return packs.length === observed.size && observed.size === expected.size
    && [...expected].every(id => observed.has(id))
    && packs.every(pack => read(pack, 'exceeded') === false);
}

export function validateQualificationEvidenceArtifact(artifact: unknown,
  entry: CalibrationEvidence,
  { calibration, qualificationIdentity, release, references, execution, stackBenchRoot,
    enforceQualificationScope = true }: CalibrationContext
    & { enforceQualificationScope?: boolean }): void {
  const at = `evidence.${entry.kind}:${entry.stack ?? ''}:${entry.repetition}`;
  if (!isObject(artifact)) evidenceFailure(at, 'is not an artifact');
  qualificationEvidenceOrigin(artifact, calibration, qualificationIdentity, release, at);
  if (!read(artifact, 'identities', 'engine', 'sha256')) {
    evidenceFailure(at, 'has no engine content identity');
  }
  if (calibration.qualification.runner !== undefined) {
    for (const [field, expected] of Object.entries(calibration.qualification.runner)) {
      if (read(artifact, 'payload', 'runner', field) !== expected) {
        evidenceFailure(at, 'has the wrong controller runner environment');
      }
    }
    const observed = read(artifact, 'payload', 'runner');
    const missing = missingRunnerObservation(isObject(observed) ? observed : null);
    if (missing.length) evidenceFailure(at,
      `has no complete appliance runner observation (missing ${missing.join(', ')})`);
  }

  const scoredChecks = release.checkCatalog.filter(check => check.points > 0);
  const zeroPointChecks = release.checkCatalog.filter(check => check.points === 0);
  if (entry.kind === 'null') {
    if (artifact.kind !== 'null_control') evidenceFailure(at, `is ${artifact.kind}, not null_control`);
    const payload = artifact.payload;
    if (read(payload, 'ok') !== true) evidenceFailure(at, 'did not pass');
    if (canonicalDefinitionJson(read(payload, 'tracks'))
      !== canonicalDefinitionJson([release.track])) {
      evidenceFailure(at, 'targets another track');
    }
    const summary = read(payload, 'summary');
    const expectedPoints = scoredChecks.reduce((sum, check) => sum + check.points, 0);
    if (read(summary, 'criteria') !== scoredChecks.length
      || read(summary, 'points') !== expectedPoints
      || read(summary, 'expectedFailures', 'criteria') !== scoredChecks.length
      || read(summary, 'expectedFailures', 'points') !== expectedPoints
      || read(summary, 'vacuousPasses', 'criteria') !== 0
      || read(summary, 'vacuousPasses', 'points') !== 0
      || read(summary, 'oracleGaps', 'criteria') !== 0
      || read(summary, 'oracleGaps', 'points') !== 0
      || read(summary, 'unscored', 'criteria') !== zeroPointChecks.length
      || read(summary, 'unscored', 'passed') !== 0
      || read(summary, 'unscored', 'failed') !== zeroPointChecks.length
      || read(summary, 'unscored', 'inconclusive') !== 0) {
      evidenceFailure(at, 'does not prove the complete null policy');
    }
    const expected = new Map(scoredChecks.map(check => [
      `${check.source}:${check.featureId}:${check.criterionId}`, check,
    ]));
    const qualificationLevel = Number(calibration.selection.alias.slice(1));
    const results = read(payload, 'criteria') ?? [];
    if (!Array.isArray(results)) evidenceFailure(at, 'has no criteria list');
    for (const result of results) {
      const key = `${read(result, 'scenario')}:${read(result, 'feature')}:${read(result, 'criterion')}`;
      const check = expected.get(key);
      if (!check || read(result, 'track') !== release.track
        || read(result, 'level') !== qualificationLevel
        || read(result, 'points') !== check.points
        || read(result, 'status') !== 'expected_fail'
        || read(result, 'evidenceStatus') !== 'failed') {
        evidenceFailure(at, `contains invalid null result ${key}`);
      }
      expected.delete(key);
    }
    if (expected.size !== 0) evidenceFailure(at, 'does not cover every selected check exactly once');
    if (enforceQualificationScope) validateCurrentQualificationScope(artifact, entry,
      { calibration, release, references, stackBenchRoot }, at);
    return;
  }

  if (artifact.kind !== 'reference_qualification') {
    evidenceFailure(at, `is ${artifact.kind}, not reference_qualification`);
  }
  const reference = references.find(candidate => candidate.backend === entry.stack);
  if (!reference) evidenceFailure(at, `targets undeclared stack ${entry.stack}`);
  exactEvidenceIdentity(read(artifact, 'identities', 'fixture'),
    { id: reference.id, contentSha256: reference.sourceSha256 }, `${at}.fixture`);
  if (read(artifact, 'identities', 'stackAdapter', 'id') !== entry.stack) {
    evidenceFailure(at, 'has wrong stack adapter');
  }
  const payload = artifact.payload;
  if (read(payload, 'diagnostic') === true) {
    evidenceFailure(at, 'is targeted diagnostic evidence, not qualification evidence');
  }
  if (calibration.qualification.checks !== undefined) {
    const qualifiedKeys = read(payload, 'qualifiedCheckKeys') ?? [];
    if (!Array.isArray(qualifiedKeys)) evidenceFailure(at, 'has no qualified check keys');
    const actualChecks = [...qualifiedKeys].sort();
    const expectedChecks = release.checkCatalog.map(check => check.stableKey).sort();
    if (canonicalDefinitionJson(actualChecks) !== canonicalDefinitionJson(expectedChecks)) {
      evidenceFailure(at, 'does not prove the exact selected checks');
    }
  }
  const mutationControl = entry.kind === 'mutation';
  const requiredRepetitions = mutationControl
    ? calibration.qualification.mutationRepetitions : calibration.qualification.referenceRepetitions;
  const harnessSha256 = read(payload, 'harnessSha256');
  if (read(payload, 'fixture') !== reference.id
    || read(payload, 'fixtureSha256') !== reference.sourceSha256
    || read(payload, 'requiredRepetitions') !== requiredRepetitions
    || read(payload, 'isolation') !== 'docker'
    || read(payload, 'mutationControl') !== mutationControl || read(payload, 'ok') !== true
    || read(payload, 'stable') !== true || read(payload, 'sameImage') !== true
    || read(payload, 'sameHarness') !== true
    || !harnessSha256 || read(payload, 'runs', 'length') !== requiredRepetitions) {
    evidenceFailure(at, 'does not satisfy the repeated Docker gate');
  }
  const repetitions = new Set<unknown>();
  const expectedRunScore = currentLevelPoints(release, execution);
  const runs = read(payload, 'runs');
  if (!Array.isArray(runs)) evidenceFailure(at, 'has no repetition list');
  for (const run of runs) {
    const repetition = read(run, 'repetition');
    repetitions.add(repetition);
    const mutations = read(run, 'mutations');
    if (read(run, 'ok') !== true || read(run, 'processError') !== null
      || read(run, 'outcome') !== 'passed'
      || read(run, 'score') !== `${expectedRunScore}/${expectedRunScore}`
      || read(run, 'criteria') !== release.scoring.checks
      || read(run, 'zeroPointCriteria') !== zeroPointChecks.length
      || !read(run, 'imageId') || read(run, 'harnessSha256Before') !== harnessSha256
      || read(run, 'harnessSha256After') !== harnessSha256
      || read(run, 'failures', 'length') !== 0
      || !hasExactSelectedPackRuntime(read(run, 'packRuntime'), release)) {
      evidenceFailure(at, `contains failed or incomplete repetition ${repetition}`);
    }
    if (mutationControl) {
      if (!mutations || Number(read(mutations, 'total')) < 1
        || read(mutations, 'caught') !== read(mutations, 'total')) {
        evidenceFailure(at, `did not catch every mutation in repetition ${repetition}`);
      }
    } else if (mutations !== null) {
      evidenceFailure(at, `reference repetition ${repetition} contains mutation results`);
    }
  }
  if (repetitions.size !== requiredRepetitions
    || !Array.from({ length: requiredRepetitions }, (_, index) => index + 1)
      .every(repetition => repetitions.has(repetition))) {
    evidenceFailure(at, 'does not contain the exact repetition set');
  }
  if (!repetitions.has(entry.repetition)) evidenceFailure(at, 'does not contain its declared repetition');
  if (enforceQualificationScope) validateCurrentQualificationScope(artifact, entry,
    { calibration, release, references, stackBenchRoot }, at);
}

function validateCurrentQualificationScope(artifact: UnknownRecord, entry: CalibrationEvidence,
  { calibration, release, references, stackBenchRoot }:
    Pick<CalibrationContext, 'calibration' | 'release' | 'references' | 'stackBenchRoot'>,
  at: string): void {
  const actual = read(artifact, 'payload', 'qualificationScope');
  if (!isObject(actual)) throw new QualificationEvidenceStaleError(at,
    'legacy broad-hash evidence has no scoped qualification identity');
  try { validateQualificationScopeIdentity(actual, `${at}.qualificationScope`); }
  catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    evidenceFailure(at, `has an invalid scoped identity: ${message}`);
  }
  const reference = entry.kind === 'null' ? null
    : references.find(candidate => candidate.backend === entry.stack);
  const mutation = entry.kind === 'mutation'
    ? calibration.mutations.find(candidate => candidate.backend === entry.stack) : null;
  const expected = qualificationScopeIdentity({
    kind: entry.kind,
    release,
    stack: entry.stack ?? null,
    reference,
    mutation,
    stackBenchRoot,
  });
  if (actual.sha256 !== expected.sha256) {
    if (canReuseQualificationScope({ actual, expected, artifact, calibration, entry })) return;
    const changed = ['executableSha256', 'checksSha256', 'mutationSha256']
      .filter(field => actual[field] !== read(expected, field));
    if (canonicalDefinitionJson(actual.recipe) !== canonicalDefinitionJson(expected.recipe)) {
      changed.push('recipe');
    }
    if (canonicalDefinitionJson(actual.stack) !== canonicalDefinitionJson(expected.stack)) {
      changed.push('stack');
    }
    throw new QualificationEvidenceStaleError(at,
      `scoped inputs changed (${changed.length ? changed.join(', ') : 'identity'})`);
  }
}

function verifyQualificationEvidence(entries: CalibrationEvidence[], stackBenchRoot: string,
  at: string, context: Omit<CalibrationContext, 'stackBenchRoot'>):
  { entries: CalibrationEvidence[]; staleness: QualificationStaleness[]; buildImage?: string } {
  const artifacts = new Map();
  const normalized = verifyEvidence(entries, stackBenchRoot, at);
  const images = new Set<string>();
  const runners = new Set();
  const staleness: QualificationStaleness[] = [];
  normalized.forEach((entry, index) => {
    let artifact = artifacts.get(entry.path);
    if (!artifact) {
      try { artifact = readArtifact(resolve(stackBenchRoot, entry.path)); }
      catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        fail(`${at}[${index}].path`, `is not a valid artifact: ${message}`);
      }
      artifacts.set(entry.path, artifact);
    }
    validateQualificationEvidenceArtifact(artifact, entry,
      { ...context, stackBenchRoot, enforceQualificationScope: false });
    try {
      validateCurrentQualificationScope(artifact, entry,
        { ...context, stackBenchRoot }, `${at}[${index}]`);
    } catch (error) {
      if (!(error instanceof QualificationEvidenceStaleError)) throw error;
      staleness.push({ kind: entry.kind, stack: entry.stack ?? null,
        repetition: entry.repetition, path: entry.path, reason: error.reason });
    }
    if (context.calibration.qualification.runner !== undefined) {
      runners.add(canonicalDefinitionJson(read(artifact, 'payload', 'runner')));
    }
    if (entry.kind !== 'null') {
      const runs = read(artifact, 'payload', 'runs');
      if (Array.isArray(runs)) for (const run of runs) {
        const imageId = read(run, 'imageId');
        if (typeof imageId === 'string') images.add(imageId);
      }
    }
  });
  if (images.size > 1) fail(at, 'qualification artifacts use different build images');
  if (runners.size > 1) fail(at, 'qualification artifacts use different appliance runner environments');
  const buildImage = [...images][0];
  return { entries: normalized, staleness, ...(buildImage ? { buildImage } : {}) };
}

export function mutationExecutionSha256(manifest: UnknownRecord,
  mutations: unknown = manifest.mutations): string {
  const { status: _status, mutations: _mutations, ...fields } = manifest;
  const execution = { ...fields, mutations };
  return sha256(canonicalDefinitionJson(canonicalizeDefinition(execution)));
}

type IdentifiableCalibration = Partial<CalibrationDefinition>
  & Pick<CalibrationDefinition, 'id'>;

export function calibrationQualificationIdentity(
  calibration: IdentifiableCalibration): CalibrationIdentity {
  if (!calibration || typeof calibration !== 'object') {
    throw new Error('calibration qualification identity requires a compiled calibration');
  }
  const document = canonicalizeDefinition({
    schemaVersion: CALIBRATION_SCHEMA_VERSION,
    id: calibration.id,
    track: calibration.track,
    recipe: {
      id: calibration.recipe?.id,
      meaningSha256: calibration.recipe?.meaningSha256,
      executionSha256: calibration.recipe?.executionSha256,
      contentSha256: calibration.recipe?.contentSha256,
    },
    fixture: { id: calibration.fixture?.id },
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
      ...(calibration.qualification?.checks
        ? { checks: [...calibration.qualification.checks].sort() } : {}),
      ...(calibration.qualification?.featureCatalog
        ? { featureCatalog: calibration.qualification.featureCatalog } : {}),
      ...(calibration.qualification?.runner ? { runner: calibration.qualification.runner } : {}),
      stacks: [...(calibration.qualification?.stacks ?? [])].sort(),
    },
    equivalenceDecisions: calibration.equivalenceDecisions,
    ...(calibration.qualificationReuse ? { qualificationReuse: calibration.qualificationReuse } : {}),
  });
  return { id: calibration.id,
    contentSha256: sha256(canonicalDefinitionJson(document)) };
}

export function calibrationQualificationRelease<
  T extends Pick<RecipeRelease, 'scoring' | 'checkCatalog'>>(
  calibration: { qualification: { checks?: string[] } },
  release: T,
  execution: RecipeExecution[],
  { source = '<calibration>' }: { source?: string } = {}):
  { release: T; execution: RecipeExecution[] } {
  const requested = new Set(calibration.qualification.checks
    ?? release.checkCatalog.map(check => check.stableKey));
  const checkCatalog = release.checkCatalog.filter(check => requested.delete(check.stableKey));
  if (requested.size) {
    fail(`${source}.qualification.checks`, `contains unknown checks: ${
      [...requested].sort().join(', ')}`);
  }
  if (checkCatalog.length === 0) fail(`${source}.qualification.checks`, 'selects no checks');
  const executionIds = new Set(checkCatalog.map(check => check.executionId));
  return {
    release: { ...release, checkCatalog, scoring: { ...release.scoring,
      checks: checkCatalog.length,
      points: checkCatalog.reduce((total, check) => total + check.points, 0) } },
    execution: execution.filter(entry => executionIds.has(entry.id)),
  };
}

export function compileCalibrationFile(calibrationPath: string,
  { trackRoot, stackBenchRoot, release }:
    { trackRoot: string; stackBenchRoot?: string; release: RecipeRelease }): CalibrationPlan {
  if (!release) throw new Error('calibration compilation requires the resolved recipe release');
  const root = realpathSync(resolve(trackRoot));
  const benchRoot = realpathSync(resolve(stackBenchRoot ?? resolve(root, '..', '..')));
  const calibrationRef = contained(root, root, calibrationPath, 'calibration path');
  const absolute = calibrationRef.absolute;
  const source = relative(root, absolute).replaceAll('\\', '/');
  const calibration = compileCalibrationDefinition(
    readDefinitionJson(absolute, 'calibration'), { source });
  if (calibration.track !== release.track) fail(`${source}.track`, `expected ${release.track}`);
  for (const [field, declared] of [['id', calibration.recipe.id],
    ['meaningSha256', calibration.recipe.meaningSha256],
    ['executionSha256', calibration.recipe.executionSha256],
    ['contentSha256', calibration.recipe.contentSha256]] as const) {
    if (declared !== release[field]) {
      fail(`${source}.recipe.${field}`, `does not match resolved recipe ${release.id}`);
    }
  }
  const recipeRef = contained(root, dirname(absolute), calibration.recipe.path, `${source}.recipe.path`);
  if (!release.sourceManifest.some(entry => entry.path === recipeRef.relative && entry.kinds.includes('recipe'))) {
    fail(`${source}.recipe.path`, 'does not name the resolved recipe source');
  }
  const execution = executionPlanForRelease(recipeRef.absolute, {
    trackRoot: root,
    level: Number(calibration.selection.alias.slice(1)),
  });
  if (calibration.qualification.featureCatalog) {
    const declared = calibration.qualification.featureCatalog;
    const fullCatalog = resolveFeatureCatalog(declared.path,
      { dir: root, name: release.track });
    const qualificationLevel = Number(calibration.selection.alias.slice(1));
    const levels = progressionLevels(fullCatalog).filter(level => level <= qualificationLevel);
    const catalog = selectFeatureCatalogLevels(fullCatalog, levels);
    if (catalog.identity.contentSha256 !== declared.contentSha256) {
      fail(`${source}.qualification.featureCatalog.contentSha256`, 'is stale');
    }
    const catalogChecks = catalog.definition.nodes
      .flatMap(node => node.gradingChecks.map(check => check.id)).sort();
    const selectedChecks = [...(calibration.qualification.checks ?? [])].sort();
    if (canonicalDefinitionJson(catalogChecks) !== canonicalDefinitionJson(selectedChecks)) {
      fail(`${source}.qualification.checks`, 'does not match the feature catalog level selection');
    }
  }
  const { release: qualificationRelease, execution: qualificationExecution }
    = calibrationQualificationRelease(calibration, release, execution, { source });
  if (calibration.fixture.id !== release.components.fixture.id) {
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
    if (read(entry, 'imported', 'sourceSha256') !== selection.sourceSha256) {
      fail(`${at}.sourceSha256`, 'is stale');
    }
    return { ...selection, ...(entry.targetPath ? { targetPath: entry.targetPath } : {}) };
  });

  const releaseStableKeys = new Set(release.checkCatalog.map(check => check.stableKey));
  const qualifiedStableKeys = new Set(qualificationRelease.checkCatalog.map(check => check.stableKey));
  const mutationTargetRefs = new Map();
  const mutationCoverage = new Map();
  const mutations = calibration.mutations.map((selection, index) => {
    const at = `${source}.mutations[${index}]`;
    const ref = contained(benchRoot, benchRoot, selection.path, `${at}.path`);
    const digest = sha256(readFileSync(ref.absolute));
    if (digest !== selection.sha256) fail(`${at}.sha256`, `stale digest for ${selection.path}`);
    const manifest = readDefinitionJson(ref.absolute, 'mutation manifest');
    if (!isObject(manifest)) fail(at, 'mutation manifest must be an object');
    if (manifest.schemaVersion !== 3 || manifest.level !== undefined) {
      fail(at, 'mutation manifest must use schema 3 and must not own a level');
    }
    const reference = references.find(candidate => candidate.id === selection.referenceId);
    if (!reference || reference.backend !== selection.backend) fail(`${at}.referenceId`, 'does not match mutation backend');
    if (manifest.backend !== selection.backend || manifest.track !== release.track) {
      fail(at, 'mutation manifest targets another benchmark');
    }
    if (manifest.fixtureSha256 !== reference.sourceSha256) fail(at, 'mutation fixture hash does not match its reference');
    const declared = manifest.mutations;
    if (!Array.isArray(declared)) fail(at, 'mutation manifest must declare an array of mutations');
    const definitions = validateMutationDefinitions(declared as MutationDefinition[],
      { defaultScenario: typeof manifest.scenario === 'string' ? manifest.scenario : null,
        requireScenario: true });
    if (!definitions.ok) fail(at, `invalid mutation definitions: ${definitions.issues.map(issue => issue.kind).join(', ')}`);
    const selectedMutations = [];
    const targets = [];
    const covered = mutationCoverage.get(selection.backend) ?? new Set();
    for (const mutation of declared) {
      const stableKeys = mutationTargetKeys(mutation);
      const scopedKeys = stableKeys.filter(stableKey => qualifiedStableKeys.has(stableKey));
      if (scopedKeys.length === 0) {
        if (calibration.qualification.checks === undefined) {
          fail(at, `mutation ${mutation.id} targets unknown recipe checks: ${stableKeys.join(', ')}`);
        }
        continue;
      }
      if (scopedKeys.length !== stableKeys.length) {
        fail(at, `mutation ${mutation.id} spans qualification scope and unrelated checks`);
      }
      for (const stableKey of scopedKeys) {
        if (!releaseStableKeys.has(stableKey)) {
          fail(at, `mutation ${mutation.id} targets unknown recipe check ${stableKey}`);
        }
      }
      selectedMutations.push(mutation);
      for (const stableKey of scopedKeys) covered.add(stableKey);
      const targetRef = `${selection.backend}:${mutation.id}`;
      mutationTargetRefs.set(targetRef, new Set(scopedKeys));
      targets.push({ id: mutation.id, stableKeys: scopedKeys });
    }
    mutationCoverage.set(selection.backend, covered);
    return { ...selection, path: ref.relative,
      executionSha256: mutationExecutionSha256(manifest, selectedMutations), targets };
  });

  const scoredKeys = qualificationRelease.checkCatalog.filter(check => check.points > 0)
    .map(check => check.stableKey);
  for (const stack of calibration.qualification.stacks) {
    const covered = mutationCoverage.get(stack) ?? new Set();
    const missing = scoredKeys.filter(key => !covered.has(key));
    if (missing.length) {
      fail(`${source}.mutations`, `${stack} does not cover scored checks: ${missing.join(', ')}`);
    }
  }

  const zeroPoint = new Set(qualificationRelease.checkCatalog.filter(check => check.points === 0)
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

  let qualificationReuse = calibration.qualificationReuse;
  if (qualificationReuse) {
    if (qualificationReuse.sourceRecipe.executionSha256 !== calibration.recipe.executionSha256) {
      fail(`${source}.qualificationReuse.sourceRecipe.executionSha256`,
        'must match the current recipe execution identity');
    }
    qualificationReuse.scopes.forEach((scope, index) => {
      const reference = scope.kind === 'null' ? null
        : references.find(candidate => candidate.backend === scope.stack) ?? null;
      const mutation = scope.kind === 'mutation'
        ? mutations.find(candidate => candidate.backend === scope.stack) ?? null : null;
      const expected = qualificationScopeIdentity({
        kind: scope.kind,
        release: qualificationRelease,
        stack: scope.stack ?? null,
        reference,
        mutation,
        stackBenchRoot: benchRoot,
      });
      if (scope.toExecutableSha256 !== expected.executableSha256) {
        fail(`${source}.qualificationReuse.scopes[${index}].toExecutableSha256`, 'is stale');
      }
    });
    qualificationReuse = { ...qualificationReuse,
      evidence: verifyEvidence(qualificationReuse.evidence, benchRoot,
        `${source}.qualificationReuse.evidence`) };
  }
  const calibrated = { ...calibration, qualificationReuse,
    references: { ...calibration.references, entries: references },
    mutations } as Validated<CalibrationDefinition>;
  const qualificationIdentity = calibrationQualificationIdentity(calibrated);
  const verifiedEvidence = verifyQualificationEvidence(calibration.qualification.evidence, benchRoot,
    `${source}.qualification.evidence`, {
      calibration: calibrated,
      qualificationIdentity,
      release: qualificationRelease,
      references,
      execution: qualificationExecution,
    });
  const evidence = verifiedEvidence.entries;
  const equivalenceDecisions = calibration.equivalenceDecisions.map((decision, index) => ({
    ...decision,
    evidence: verifyEvidence(decision.evidence, benchRoot, `${source}.equivalenceDecisions[${index}].evidence`),
  }));
  const selectionRef = contained(root, root, calibration.selection.path,
    `${source}.selection.path`);
  const selectionDigest = sha256(readFileSync(selectionRef.absolute));
  if (selectionDigest !== calibration.selection.sha256) fail(`${source}.selection.sha256`, 'selection digest is stale');
  const selectionCatalog = compileRecipeSelectionFile(selectionRef.absolute, { trackRoot: root });
  if (!selectionCatalog.entries.some(entry => entry.alias === calibration.selection.alias
    && entry.recipe.id === release.id)) {
    fail(`${source}.selection`, 'does not resolve to this recipe');
  }

  const supportedStacks = new Set(calibration.qualification.stacks);
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
  if (actualEvidence.length > 0 && (new Set(actualEvidence).size !== actualEvidence.length
    || canonicalDefinitionJson([...actualEvidence].sort()) !== canonicalDefinitionJson(expectedEvidence.sort()))) {
    fail(`${source}.qualification.evidence`, 'must contain exactly the declared reference, mutation, and null repetitions');
  }

  const planInput = {
    calibrationSchemaVersion: CALIBRATION_SCHEMA_VERSION,
    id: calibration.id,
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
      ...(verifiedEvidence.buildImage ? { buildImage: verifiedEvidence.buildImage } : {}),
      stacks: [...calibration.qualification.stacks].sort(),
      evidence: evidence.sort((a, b) => `${a.kind}:${a.stack ?? ''}:${a.repetition}`
        .localeCompare(`${b.kind}:${b.stack ?? ''}:${b.repetition}`)) },
    equivalenceDecisions: equivalenceDecisions.sort((a, b) =>
      `${a.fromExecutionSha256}:${a.toExecutionSha256}`
        .localeCompare(`${b.fromExecutionSha256}:${b.toExecutionSha256}`)),
    ...(qualificationReuse ? { qualificationReuse: { ...qualificationReuse,
      scopes: [...qualificationReuse.scopes].sort((a, b) => `${a.kind}:${a.stack ?? ''}`
        .localeCompare(`${b.kind}:${b.stack ?? ''}`)) } } : {}),
    selection: { ...calibration.selection, path: selectionRef.relative },
  };
  const plan = canonicalizeDefinition(planInput);
  if (!isObject(plan)) fail(source, 'could not be canonicalized');
  const qualificationSha256 = calibrationQualificationIdentity(planInput).contentSha256;
  return { ...plan, qualificationSha256,
    contentSha256: sha256(canonicalDefinitionJson({ ...plan, qualificationSha256 })),
    qualificationStaleness: verifiedEvidence.staleness } as CalibrationPlan;
}

export function calibrationCoversAlias(calibration: CalibrationPlan,
  release: RecipeRelease, alias: string,
  { selection, selectionPath, trackRoot }:
    { selection: unknown; selectionPath: string; trackRoot: string }): boolean {
  const exactAlias = calibration.selection.alias === alias;
  const coveredAliases = calibration.selection.coveredAliases ?? [calibration.selection.alias];
  if (!coveredAliases.includes(alias)) return false;
  if (!exactAlias && !calibration.qualification.featureCatalog) return false;
  const requestedLevel = Number(alias.slice(1));
  const qualifiedLevel = Number(calibration.selection.alias.slice(1));
  if (!Number.isInteger(requestedLevel) || requestedLevel < 1 || requestedLevel > qualifiedLevel) {
    return false;
  }
  const entries = read(selection, 'entries');
  if (!Array.isArray(entries)) return false;
  return entries.some(entry => read(entry, 'alias') === alias
    && read(entry, 'recipe', 'id') === release.id
    && calibration.recipe.contentSha256 === release.contentSha256
    && contained(trackRoot, dirname(selectionPath), read(entry, 'recipe', 'path'),
      `calibration alias ${alias}`).relative === calibration.recipe.path);
}

export function resolveCalibrationForRelease(release: RecipeRelease | null,
  { trackRoot, stackBenchRoot, alias = null }:
    { trackRoot: string; stackBenchRoot?: string; alias?: string | null }): CalibrationPlan | null {
  if (!release) return null;
  const root = realpathSync(resolve(trackRoot));
  const directory = join(root, 'composition', 'calibrations');
  if (!existsSync(directory)) return null;
  const matches = [];
  for (const name of readdirSync(directory).filter(file => file.endsWith('.json')).sort()) {
    const path = join(directory, name);
    const raw = readDefinitionJson(path, 'calibration');
    if (read(raw, 'recipe', 'id') !== release.id
      || read(raw, 'recipe', 'contentSha256') !== release.contentSha256) continue;
    matches.push(compileCalibrationFile(path, { trackRoot: root, stackBenchRoot, release }));
  }
  const selected = alias === null ? matches : matches.filter(calibration => {
    const selectionPath = resolve(root, calibration.selection.path);
    const selection = compileRecipeSelectionFile(selectionPath, { trackRoot: root });
    return calibrationCoversAlias(calibration, release, alias,
      { selection, selectionPath, trackRoot: root });
  });
  if (selected.length > 1) {
    const scope = alias === null ? '' : ` for ${alias}`;
    throw new Error(`multiple calibrations match ${release.id}${scope} (${release.contentSha256})`);
  }
  return selected[0] ?? null;
}
