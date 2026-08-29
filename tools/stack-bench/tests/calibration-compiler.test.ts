import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { calibrationCoversAlias, calibrationQualificationIdentity,
  calibrationQualificationRelease, canReuseQualificationScope,
  compileCalibrationDefinition, compileCalibrationFile,
  currentLevelPoints, hasExactSelectedPackRuntime, resolveCalibrationForRelease,
  validateQualificationEvidenceArtifact } from '../src/composition/calibration-compiler.js';
import type { CalibrationContext, CalibrationDefinition, CalibrationEvidence,
  CalibrationPlan } from '../src/composition/calibration-compiler.js';
import { readArtifact } from '../src/evidence/artifacts.js';
import { checkCalibrations } from '../commands/check-calibration.js';
import { buildRecipeRelease, resolveRecipeRelease } from '../src/composition/recipe-release.js';
import type { RecipeExecution } from '../src/composition/recipe-release.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { loadTrack } from '../src/composition/tracks.js';
import { qualificationScopeIdentity } from '../src/composition/qualification-scope.js';

const ROOT = STACK_BENCH_ROOT;
const TRACK = loadTrack('ecommerce');
const CALIBRATION = join(TRACK.dir, 'composition', 'calibrations', 'l1-modular-2.5.0.json');
const PROGRESSION_CALIBRATION = join(TRACK.dir, 'composition', 'calibrations',
  'progression-l1-l3-1.1.0.json');

interface PromotionCatalog {
  entries: Array<{
    alias: string;
    status: string;
    recipe: { id: string; version: string; path: string };
  }>;
}

interface QualificationArtifactPayload {
  qualificationScope?: unknown;
  runs: Array<{ ok: boolean }>;
  runner: { mode: string; platform: string; [key: string]: unknown };
  summary: { vacuousPasses: { criteria: number } };
  [key: string]: unknown;
}

interface QualificationArtifact {
  id: string;
  kind: string;
  attempt: { parentId?: string | null; [key: string]: unknown };
  identities: {
    agentAdapter: unknown;
    stackAdapter: { id: string };
    calibration: { id: string; version: string; sha256: string };
    recipe: { id: string; version: string; sha256: string };
    [key: string]: unknown;
  };
  payload: QualificationArtifactPayload;
  [key: string]: unknown;
}

function required<T>(value: T | undefined, message: string): T {
  assert.ok(value !== undefined, message);
  return value;
}

function parseCalibration(path: string): CalibrationDefinition {
  return JSON.parse(readFileSync(path, 'utf8')) as CalibrationDefinition;
}

function readQualificationArtifact(path: string): QualificationArtifact {
  return readArtifact<QualificationArtifactPayload>(path) as QualificationArtifact;
}

test('qualification runtime must cover the exact selected pack set', () => {
  const release = { checkCatalog: [{ packId: 'accounts' }, { packId: 'accounts' },
    { packId: 'orders' }] };
  assert.equal(hasExactSelectedPackRuntime({ packs: [
    { id: 'accounts', exceeded: false }, { id: 'orders', exceeded: false },
  ] }, release), true);
  assert.equal(hasExactSelectedPackRuntime({ packs: [
    { id: 'accounts', exceeded: false }, { id: 'orders', exceeded: false },
    { id: 'future', exceeded: false },
  ] }, release), false);
  assert.equal(hasExactSelectedPackRuntime({ packs: [
    { id: 'accounts', exceeded: false }, { id: 'accounts', exceeded: false },
  ] }, release), false);
  assert.equal(hasExactSelectedPackRuntime({ packs: [
    { id: 'accounts', exceeded: false }, { id: 'orders', exceeded: true },
  ] }, release), false);
});

function current() {
  const binding = resolveRecipeRelease(TRACK, 1);
  return { binding, plan: compileCalibrationFile(CALIBRATION,
    { trackRoot: TRACK.dir, stackBenchRoot: ROOT, release: binding.release }) };
}

function qualifiedProgression() {
  const binding = resolveRecipeRelease(TRACK, 3, 'ecommerce.progression-l1-l3@1.1.0');
  return { binding, plan: compileCalibrationFile(PROGRESSION_CALIBRATION,
    { trackRoot: TRACK.dir, stackBenchRoot: ROOT, release: binding.release }) };
}

function withCurrentScope(
  artifact: QualificationArtifact,
  entry: CalibrationEvidence,
  context: CalibrationContext,
): QualificationArtifact {
  const scoped = structuredClone(artifact);
  const reference = entry.kind === 'null' ? null
    : context.references.find(candidate => candidate.backend === entry.stack);
  const mutation = entry.kind === 'mutation'
    ? context.calibration.mutations.find(candidate => candidate.backend === entry.stack) : null;
  scoped.payload.qualificationScope = qualificationScopeIdentity({
    kind: entry.kind, release: context.release, stack: entry.stack ?? null,
    reference, mutation, stackBenchRoot: ROOT,
  });
  return scoped;
}

test('runtime calibration resolution exposes the pending L1 candidate', () => {
  const l1 = resolveRecipeRelease(TRACK, 1).release;
  const resolved = resolveCalibrationForRelease(l1,
    { trackRoot: TRACK.dir, stackBenchRoot: ROOT, alias: 'L1' });
  assert.ok(resolved);
  assert.equal(resolved.id, 'ecommerce.l1-modular-calibration');
  assert.match(resolved.contentSha256, /^[a-f0-9]{64}$/);
  assert.equal(resolved.state, 'draft');
  assert.deepEqual(resolved.qualification.stacks.map(stack => stack.status),
    ['candidate', 'candidate', 'candidate']);
  assert.deepEqual(resolved.qualification.evidence, []);
  assert.deepEqual(resolved.qualification.runner, {
    schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64',
  });
  assert.throws(() => resolveRecipeRelease(TRACK, 2, 'ecommerce.l2-standard@1.6.0'),
    /requires exactly one promoted L1 base; found 0/);
});

test('runtime calibration resolution uses the requested level alias', () => {
  const release = resolveRecipeRelease(TRACK, 3,
    'ecommerce.progression-catalog@1.0.0').release;
  const options = { trackRoot: TRACK.dir, stackBenchRoot: ROOT };
  assert.equal(resolveCalibrationForRelease(release, { ...options, alias: 'L3' })?.id,
    'ecommerce.progression-calibration');
  assert.equal(resolveCalibrationForRelease(release, { ...options, alias: 'L2' }), null);
});

test('one cumulative calibration covers each promoted lower alias of the same recipe', () => {
  const options = { trackRoot: TRACK.dir, stackBenchRoot: ROOT };
  for (const level of [1, 2, 3]) {
    const release = resolveRecipeRelease(TRACK, level,
      'ecommerce.progression-l1-l3@1.1.0').release;
    assert.equal(release.id, 'ecommerce.progression-l1-l3');
    const calibration = resolveCalibrationForRelease(release,
      { ...options, alias: `L${level}` });
    assert.ok(calibration);
    assert.equal(calibration.id, 'ecommerce.progression-l1-l3-calibration');
    assert.equal(calibration.promotion.alias, 'L3');
  }
});

test('cumulative aliases must name the exact calibrated recipe source', () => {
  const release = resolveRecipeRelease(TRACK, 3,
    'ecommerce.progression-l1-l3@1.1.0').release;
  const calibration = resolveCalibrationForRelease(release,
    { trackRoot: TRACK.dir, stackBenchRoot: ROOT });
  assert.ok(calibration);
  const catalogPath = join(TRACK.dir, 'composition', 'candidates.json');
  const catalog = JSON.parse(readFileSync(catalogPath, 'utf8')) as PromotionCatalog;
  const l1Entry = catalog.entries.find(entry => entry.alias === 'L1'
    && entry.recipe.id === release.id && entry.recipe.version === release.version);
  assert.ok(l1Entry);
  l1Entry.recipe.path =
    'recipes/progression-l1-l3-1.0.0.json';
  assert.equal(calibrationCoversAlias(calibration, release, 'L1',
    { catalog, catalogPath, trackRoot: TRACK.dir }), false);
  const l3Entry = catalog.entries.find(entry => entry.alias === 'L3'
    && entry.recipe.id === release.id && entry.recipe.version === release.version);
  assert.ok(l3Entry);
  l3Entry.recipe.path =
    'recipes/progression-l1-l3-1.0.0.json';
  assert.equal(calibrationCoversAlias(calibration, release, 'L3',
    { catalog, catalogPath, trackRoot: TRACK.dir }), false);
});

function temporaryCalibration(change: (value: CalibrationDefinition) => void) {
  const directory = mkdtempSync(join(tmpdir(), 'stack-bench-calibration-'));
  const trackRoot = join(directory, 'ecommerce');
  cpSync(TRACK.dir, trackRoot, { recursive: true });
  const path = join(trackRoot, 'composition', 'calibrations', 'l1-modular-2.5.0.json');
  const value = parseCalibration(CALIBRATION);
  change(value);
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
  return { directory, path, trackRoot };
}

function compileChanged(change: (value: CalibrationDefinition) => void): CalibrationPlan {
  const temporary = temporaryCalibration(change);
  try {
    const release = resolveRecipeRelease({ ...TRACK, dir: temporary.trackRoot }, 1).release;
    return compileCalibrationFile(temporary.path,
      { trackRoot: temporary.trackRoot, stackBenchRoot: ROOT, release });
  } finally { rmSync(temporary.directory, { recursive: true, force: true }); }
}

test('calibration definitions bind an optional stable-check subset', () => {
  const value = parseCalibration(CALIBRATION);
  value.qualification.checks = ['check.b', 'check.a'];
  const compiled = compileCalibrationDefinition(value);
  assert.deepEqual(compiled.qualification.checks, ['check.b', 'check.a']);
  const identityInput = { ...compiled, mutations: compiled.mutations.map(mutation => ({
    ...mutation, executionSha256: 'a'.repeat(64),
  })) };
  const first = calibrationQualificationIdentity(identityInput);
  identityInput.qualification.checks = ['check.a'];
  assert.notEqual(calibrationQualificationIdentity(identityInput).sha256, first.sha256);
  value.qualification.checks = ['check.a', 'check.a'];
  assert.throws(() => compileCalibrationDefinition(value), /must not contain duplicates/);
});

test('calibration check subsets produce an exact qualification release', () => {
  const sourceRelease = { scoring: { mode: 'source-points', checks: 2, points: 3 },
    checkCatalog: [
      { stableKey: 'check.a', executionId: 'suite', points: 1 },
      { stableKey: 'check.b', executionId: 'unused', points: 2 },
    ] };
  const execution: RecipeExecution[] = [{ id: 'suite', ownership: { kind: 'current' } },
    { id: 'unused', ownership: { kind: 'current' } }];
  const scoped = calibrationQualificationRelease({ qualification: { checks: ['check.a'] } },
    sourceRelease, execution);
  assert.deepEqual(scoped.release.checkCatalog.map(check => check.stableKey), ['check.a']);
  assert.deepEqual(scoped.release.scoring, { mode: 'source-points', checks: 1, points: 1 });
  assert.deepEqual(scoped.execution.map(entry => entry.id), ['suite']);
  assert.throws(() => calibrationQualificationRelease(
    { qualification: { checks: ['missing'] } }, sourceRelease, execution), /unknown checks/);
});

test('the current L1 calibration deterministically binds recipe, fixture, references, mutations, and controls', () => {
  const first = current().plan;
  const second = current().plan;
  assert.deepEqual(first, second);
  assert.equal(first.state, 'draft');
  assert.equal(first.recipe.contentSha256, current().binding.release.contentSha256);
  assert.equal(first.fixture.sourceSha256, current().binding.release.components.fixture.sha256);
  assert.equal(first.references.entries.length, 3);
  assert.equal(first.mutations.length, 3);
  const scored = new Set(current().binding.release.checkCatalog.filter(check => check.points > 0)
    .map(check => check.stableKey));
  for (const mutation of first.mutations) {
    const covered = new Set(mutation.targets.flatMap(target => target.stableKeys));
    assert.deepEqual([...scored].filter(key => !covered.has(key)), [],
      `${mutation.backend} must cover every scored check`);
  }
  assert.equal(first.controls.length, 2);
  assert.equal(first.controls.every(control => control.role === 'precondition'), true);
  assert.equal(new Set(first.controls.map(control => control.stableKey)).size, 2);
  assert.match(first.contentSha256, /^[a-f0-9]{64}$/);
  assert.match(first.qualificationSha256, /^[a-f0-9]{64}$/);
  assert.equal(calibrationQualificationIdentity(first).sha256, first.qualificationSha256);
  assert.deepEqual(checkCalibrations({ trackName: 'ecommerce' })
    .filter(result => result.id === 'ecommerce.l1-modular-calibration'
      || result.id === 'ecommerce.l2-standard-calibration')
    .map(result => `${result.id}@${result.version}:${result.state}`), [
    'ecommerce.l1-modular-calibration@2.5.0:draft',
    'ecommerce.l2-standard-calibration@1.6.0:draft',
  ]);
});

test('qualification evidence is semantically bound and tampering fails closed', () => {
  const { binding, plan } = qualifiedProgression();
  const referenceEntry: CalibrationEvidence = {
    kind: 'reference', stack: 'mongodb', repetition: 1,
    path: 'qualification-evidence/ecommerce-progression-l1-l3-v1.0.0/mongodb-reference.json',
    sha256: 'historical-fixture',
  };
  const reference = readQualificationArtifact(join(ROOT, referenceEntry.path));
  const historicalRelease = buildRecipeRelease(join(TRACK.dir, 'composition', 'recipes',
    'progression-l1-l3-1.0.0.json'), { trackRoot: TRACK.dir });
  historicalRelease.contentSha256 = reference.identities.recipe.sha256;
  const qualified = calibrationQualificationRelease(plan, historicalRelease, binding.execution);
  const context = {
    calibration: plan,
    qualificationIdentity: reference.identities.calibration,
    release: qualified.release,
    references: plan.references.entries,
    execution: qualified.execution,
    stackBenchRoot: ROOT,
  };
  const scopedReference = withCurrentScope(reference, referenceEntry, context);
  assert.doesNotThrow(() => validateQualificationEvidenceArtifact(scopedReference, referenceEntry, context));
  const legacyReference = structuredClone(scopedReference);
  delete legacyReference.payload.qualificationScope;
  assert.throws(() => validateQualificationEvidenceArtifact(legacyReference, referenceEntry, context),
    /legacy broad-hash evidence has no scoped qualification identity/);
  const wrongStack = structuredClone(scopedReference);
  wrongStack.identities.stackAdapter.id = 'postgres';
  assert.throws(() => validateQualificationEvidenceArtifact(wrongStack, referenceEntry, context),
    /wrong stack adapter/);

  const hiddenFailure = structuredClone(scopedReference);
  required(hiddenFailure.payload.runs[0], 'qualification run').ok = false;
  assert.throws(() => validateQualificationEvidenceArtifact(hiddenFailure, referenceEntry, context),
    /failed or incomplete repetition/);

  const runnerBound = structuredClone(plan);
  runnerBound.qualification.runner = {
    schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64',
  };
  const runnerIdentity = calibrationQualificationIdentity(runnerBound);
  const runnerReference = structuredClone(scopedReference);
  runnerReference.identities.calibration = runnerIdentity;
  runnerReference.payload.runner = { ...runnerBound.qualification.runner,
    dockerEngineVersion: '29.1.2', dockerOs: 'linux', dockerArchitecture: 'x86_64',
    kernelVersion: '6.8.0-test', cpuCount: 8, memoryBytes: 16_000_000_000 };
  const runnerContext = { ...context, calibration: runnerBound, qualificationIdentity: runnerIdentity };
  assert.doesNotThrow(() => validateQualificationEvidenceArtifact(runnerReference, referenceEntry,
    runnerContext));
  const unobservedRunner = structuredClone(runnerReference);
  unobservedRunner.payload.runner = structuredClone(runnerBound.qualification.runner);
  assert.throws(() => validateQualificationEvidenceArtifact(unobservedRunner, referenceEntry,
    runnerContext), /no complete appliance runner observation/);
  runnerReference.payload.runner.mode = 'local-controller';
  runnerReference.payload.runner.platform = 'win32';
  assert.throws(() => validateQualificationEvidenceArtifact(runnerReference, referenceEntry,
    runnerContext), /wrong controller runner environment/);

  const nullEntry: CalibrationEvidence = {
    kind: 'null', repetition: 1,
    path: 'qualification-evidence/ecommerce-progression-l1-l3-v1.0.0/null.json',
    sha256: 'historical-fixture',
  };
  const nullArtifact = readQualificationArtifact(join(ROOT, nullEntry.path));
  const vacuous = withCurrentScope(nullArtifact, nullEntry, context);
  vacuous.payload.summary.vacuousPasses.criteria = 1;
  assert.throws(() => validateQualificationEvidenceArtifact(vacuous, nullEntry, context),
    /complete null policy/);
});

test('the L2 candidate keeps its score contract while qualification is pending', () => {
  const release = buildRecipeRelease(
    join(TRACK.dir, 'composition', 'recipes', 'l2-standard-1.6.0.json'),
    { trackRoot: TRACK.dir });
  const plan = compileCalibrationFile(join(TRACK.dir, 'composition', 'calibrations',
    'l2-standard-1.6.0.json'), {
    trackRoot: TRACK.dir, stackBenchRoot: ROOT, release,
  });
  assert.equal(release.scoring.points, 117);
  assert.equal(plan.state, 'draft');
  assert.equal(plan.promotion.status, 'candidate');
  assert.deepEqual(plan.qualification.evidence, []);
});

test('qualification uses typed ownership when inherited execution ids are renamed', () => {
  const release = { checkCatalog: [
    { stableKey: 'base', executionId: 'renamed-base', points: 5 },
    { stableKey: 'current', executionId: 'current', points: 3 },
  ] };
  const execution: RecipeExecution[] = [
    { id: 'renamed-base', ownership: { kind: 'inherited' } },
    { id: 'current', ownership: { kind: 'current' } },
  ];
  assert.equal(currentLevelPoints(release, execution), 3);
});

test('qualification identity excludes governance transitions but binds executable controls', () => {
  const plan = current().plan;
  const governance = structuredClone(plan);
  governance.state = 'qualified';
  governance.promotion.status = 'promoted';
  governance.promotion.catalogSha256 = '0'.repeat(64);
  governance.qualification.evidence = [{ kind: 'reference', stack: 'mongodb', repetition: 1,
    path: 'results/evidence.json', sha256: '1'.repeat(64) }];
  governance.qualification.stacks.forEach(stack => { stack.status = 'qualified'; });
  governance.references.entries.forEach(entry => { entry.status = 'active'; });
  governance.mutations.forEach(entry => { entry.status = 'active'; entry.sha256 = '3'.repeat(64); });
  assert.equal(calibrationQualificationIdentity(governance).sha256, plan.qualificationSha256);

  const changed = structuredClone(plan);
  changed.nullControl.repetitions += 1;
  assert.notEqual(calibrationQualificationIdentity(changed).sha256, plan.qualificationSha256);

  const changedRunner = structuredClone(plan);
  changedRunner.qualification.runner = {
    schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'arm64',
  };
  assert.notEqual(calibrationQualificationIdentity(changedRunner).sha256, plan.qualificationSha256);
});

test('set-like calibration source ordering does not change its identity', () => {
  const before = current().plan;
  const after = compileChanged(value => {
    value.references.entries.reverse();
    value.mutations.reverse();
    value.controls.reverse();
    value.qualification.stacks.reverse();
  });
  assert.equal(after.contentSha256, before.contentSha256);
});

test('stale recipe, fixture, reference, mutation, and promotion hashes fail compilation', () => {
  const changes: Array<(value: CalibrationDefinition) => void> = [
    value => { value.recipe.contentSha256 = '0'.repeat(64); },
    value => { value.fixture.sourceSha256 = '0'.repeat(64); },
    value => { required(value.references.entries[0], 'reference').sourceSha256 = '0'.repeat(64); },
    value => { required(value.mutations[0], 'mutation').sha256 = '0'.repeat(64); },
    value => { value.promotion.catalogSha256 = '0'.repeat(64); },
  ];
  for (const change of changes) assert.throws(() => compileChanged(change), /does not match|stale/);
});

test('every zero-point check requires one typed policy with valid mutation targets', () => {
  assert.throws(() => compileChanged(value => { value.controls.pop(); }), /missing zero-point checks/);
  assert.throws(() => compileChanged(value => {
    required(value.controls[0], 'control').mutationTargets = ['postgres:not-a-mutant'];
  }), /allowed only for promotion-gate|must not declare mutationTargets|unknown mutation/);
  assert.throws(() => compileChanged(value => {
    required(value.controls[0], 'control').promotionPolicy = 'record-only-until-calibrated';
  }), /must be must-pass-reference/);
  assert.throws(() => compileChanged(value => {
    required(value.controls[0], 'first control').stableKey =
      required(value.controls[1], 'second control').stableKey;
  }), /duplicates|not an unassigned/);
});

test('pending calibration cannot claim qualification or promotion', () => {
  assert.throws(() => compileChanged(value => { value.state = 'qualified'; }),
    /every supported stack must be qualified/);
  assert.throws(() => compileChanged(value => { value.promotion.status = 'promoted'; }),
    /does not resolve to this recipe at the declared status/);
});

test('equivalence decisions require two distinct executions and hash-bound evidence', () => {
  const definition = parseCalibration(CALIBRATION);
  definition.equivalenceDecisions = [{
    fromExecutionSha256: definition.recipe.executionSha256,
    toExecutionSha256: definition.recipe.executionSha256,
    rationale: 'No change',
    evidence: [{ path: 'evidence.json', sha256: '0'.repeat(64) }],
  }];
  assert.throws(() => compileCalibrationDefinition(definition), /must compare different execution hashes/);
  assert.throws(() => compileChanged(value => {
    value.qualification.evidence = [];
    value.equivalenceDecisions = [{
      fromExecutionSha256: '1'.repeat(64),
      toExecutionSha256: '2'.repeat(64),
      rationale: 'Calibrated execution-only correction',
      evidence: [{ path: 'missing-evidence.json', sha256: '3'.repeat(64) }],
    }];
  }), /does not exist/);
});

test('qualification reuse accepts only the declared source and unchanged grading inputs', () => {
  const sourceRecipe = { id: 'recipe', version: '1.0.0',
    contentSha256: '1'.repeat(64), executionSha256: '2'.repeat(64) };
  const sourceCalibration = { id: 'calibration', version: '1.0.0', sha256: '3'.repeat(64) };
  const calibration = { qualificationReuse: { sourceRecipe, sourceCalibration,
    rationale: 'Selector compatibility only',
    evidence: [{ path: 'proof.json', sha256: '6'.repeat(64) }], scopes: [{
    kind: 'mutation', stack: 'mongodb', fromExecutableSha256: '4'.repeat(64),
    toExecutableSha256: '5'.repeat(64),
  }] } };
  const artifact = { identities: {
    recipe: { id: sourceRecipe.id, version: sourceRecipe.version,
      sha256: sourceRecipe.contentSha256 },
    calibration: { ...sourceCalibration },
  } };
  const actual = { schemaVersion: 2, kind: 'mutation', executableSha256: '4'.repeat(64),
    recipe: { id: sourceRecipe.id, version: sourceRecipe.version,
      contentSha256: sourceRecipe.contentSha256 },
    checksSha256: '7'.repeat(64), stack: { id: 'mongodb', version: '1.0.0',
      reference: { id: 'fixture', sourceSha256: '8'.repeat(64) } },
    mutationSha256: '9'.repeat(64) };
  const expected = structuredClone(actual);
  expected.executableSha256 = '5'.repeat(64);
  expected.recipe = { id: 'recipe', version: '1.1.0', contentSha256: 'a'.repeat(64) };
  const input = { actual, expected, artifact, calibration,
    entry: { kind: 'mutation', stack: 'mongodb' } };
  assert.equal(canReuseQualificationScope(input), true);
  const changes: Array<(value: typeof input) => void> = [
    value => { value.actual.checksSha256 = 'b'.repeat(64); },
    value => { value.actual.mutationSha256 = 'b'.repeat(64); },
    value => { value.actual.stack.reference.sourceSha256 = 'b'.repeat(64); },
    value => { value.actual.schemaVersion += 1; },
    value => { value.actual.kind = 'reference'; },
    value => { value.expected.executableSha256 = 'b'.repeat(64); },
    value => { value.artifact.identities.calibration.sha256 = 'b'.repeat(64); },
  ];
  for (const change of changes) {
    const changed = structuredClone(input);
    change(changed);
    assert.equal(canReuseQualificationScope(changed), false);
  }
});

test('qualification reuse definitions are exact and hash-bound', () => {
  const definition = parseCalibration(CALIBRATION);
  definition.qualificationReuse = {
    sourceRecipe: { id: definition.recipe.id, version: '1.0.0',
      contentSha256: '1'.repeat(64), executionSha256: definition.recipe.executionSha256 },
    sourceCalibration: { id: definition.id, version: '1.0.0', sha256: '2'.repeat(64) },
    rationale: 'No scoring change',
    evidence: [{ path: 'proof.json', sha256: '5'.repeat(64) }],
    scopes: [{ kind: 'null', fromExecutableSha256: '3'.repeat(64),
      toExecutableSha256: '4'.repeat(64) }],
  };
  assert.doesNotThrow(() => compileCalibrationDefinition(definition));
  assert.ok(definition.qualificationReuse);
  required(definition.qualificationReuse.scopes[0], 'reuse scope').stack = 'mongodb';
  assert.throws(() => compileCalibrationDefinition(definition), /not allowed for null reuse/);
});
