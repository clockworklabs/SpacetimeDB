import assert from 'node:assert/strict';
import { join, relative } from 'node:path';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import test from 'node:test';
import { buildRecipeQualificationDocuments } from '../src/composition/recipe-release.js';
import { assertQualificationSliceCoverage, unchangedQualificationChecks,
  validateQualificationDocuments } from '../src/composition/qualification-slices.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { calibrationQualificationIdentity, calibrationQualificationRelease, compileCalibrationDefinition, mutationExecutionSha256,
  validateQualificationSlice } from '../src/composition/calibration-compiler.js';
import { mutationForRecipe, mutationTargetKeys } from '../src/evidence/mutation-analysis.js';
import type { MutationDefinition } from '../src/evidence/mutation-analysis.js';
import type { CalibrationEvidence, CalibrationPlan } from '../src/composition/calibration-compiler.js';
import { qualificationScopeIdentity } from '../src/composition/qualification-scope.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.js';
import { sha256 } from '../src/evidence/provenance.js';

const root = join(STACK_BENCH_ROOT, 'tracks/ecommerce');
const documents = validateQualificationDocuments(buildRecipeQualificationDocuments(
  join(root, 'composition/recipes/progression-catalog.json'), { trackRoot: root }));

test('saved definition proof binds the catalog and actual hash inputs', () => {
  assert.deepEqual(validateQualificationDocuments(documents), documents);
  for (const change of [
    (d: typeof documents) => { d.release.checkCatalog[0]!.points += 1; },
    (d: typeof documents) => { d.release.checkCatalog[0]!.executionId = 'wrong'; },
    (d: typeof documents) => { d.release.checkCatalog.push(d.release.checkCatalog[0]!); },
    (d: typeof documents) => { d.execution.runtime = {}; },
    (d: typeof documents) => { d.meaning.task = {}; },
  ]) {
    const changed = structuredClone(documents);
    change(changed);
    assert.throws(() => validateQualificationDocuments(changed), /qualification slice/);
  }
});

test('reuse invalidates the whole changed scenario and shared inputs', () => {
  const all = documents.release.checkCatalog.map(check => check.stableKey);
  assert.deepEqual([...unchangedQualificationChecks(documents, documents)], all);
  const changed = structuredClone(documents);
  const execution = changed.execution.execution as Array<Record<string, unknown>>;
  execution[0]!.checkGroups = [];
  const affected = documents.release.checkCatalog.filter(check => check.executionId === execution[0]!.id);
  const reused = unchangedQualificationChecks(documents, changed);
  assert.equal(reused.size, all.length - affected.length);
  assert(affected.every(check => !reused.has(check.stableKey)));
  for (const field of ['fixture', 'runtime', 'capabilities']) {
    const modified = structuredClone(documents);
    modified.execution[field] = {};
    assert.equal(unchangedQualificationChecks(documents, modified).size, 0, field);
  }
  const budget = structuredClone(documents);
  const packs = budget.execution.packs as Array<Record<string, unknown>>;
  packs[0]!.budget = {};
  const budgetReuse = unchangedQualificationChecks(documents, budget);
  assert.equal(budgetReuse.size, all.length - documents.release.checkCatalog
    .filter(check => check.packId === packs[0]!.id).length);
  const prompt = structuredClone(documents);
  prompt.meaning.task = {};
  assert.equal(unchangedQualificationChecks(documents, prompt).size, 0);
  const order = structuredClone(documents);
  (order.execution.execution as unknown[]).reverse();
  assert.equal(unchangedQualificationChecks(documents, order).size, 0);
});

test('slices require each check exactly once in every required evidence population', () => {
  const checks = documents.release.checkCatalog.slice(0, 2);
  const [a, b] = checks.map(check => check.stableKey);
  const first = { kind: 'reference', stack: 'postgres', repetition: 1, checks: [a!] };
  const second = { ...first, checks: [b!] };
  const required = ['reference:postgres:1'];
  assert.doesNotThrow(() => assertQualificationSliceCoverage([first, second], required, checks));
  assert.throws(() => assertQualificationSliceCoverage([first], required, checks), /missing coverage/);
  assert.throws(() => assertQualificationSliceCoverage([first, first, second], required, checks), /duplicate coverage/);
  assert.throws(() => assertQualificationSliceCoverage([{ ...first, checks: ['unknown'] }], required, checks), /unknown check/);
  assert.throws(() => assertQualificationSliceCoverage([first, second], [...required, 'mutation:postgres:1'], checks), /missing coverage/);
  assert.throws(() => assertQualificationSliceCoverage([{ ...first, repetition: 2 }], required, checks), /unexpected/);
});

test('saved slices validate real artifacts and reject incomplete or mismatched evidence', t => {
  const path = join(root, 'composition/calibrations/dependency-l3.json');
  const entry: CalibrationEvidence = { kind: 'mutation', stack: 'postgres', repetition: 1,
    path: 'tests/fixtures/qualification-evidence/ecommerce-l3-e804c1302/postgres-targeted.json',
    sha256: 'efc4a7f4df5f664fca9457c5746508f53db02b52f51a9ad5281c337a44c29bed',
    slice: { checks: ['ecommerce.progression.review-access-specifications.review-eligibility-direct.618a'],
      snapshot: { path: 'tests/fixtures/qualification-evidence/ecommerce-l3-e804c1302/current-inputs.json',
        sha256: '85f21bfeb1160f3889cb10bd3c3819a12dba46428d82e055e25af268f82b3f45' } } };
  const saved = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, entry.slice!.snapshot.path), 'utf8'));
  const savedDocuments = validateQualificationDocuments(saved.documents);
  const plan: CalibrationPlan = saved.calibration;
  // Exercise this retained receipt against its retained controls, not today's
  // expanded defect set. Changed controls remain a separate rejection below.
  const temporary = mkdtempSync(join(STACK_BENCH_ROOT, 'tests', '.slice-controls-'));
  t.after(() => rmSync(temporary, { recursive: true, force: true }));
  const mutationPath = join(temporary, 'postgres.json');
  writeFileSync(mutationPath, JSON.stringify(saved.mutations.postgres));
  plan.mutations.find(item => item.backend === 'postgres')!.path = mutationPath;
  const artifact = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, entry.path), 'utf8'));
  const executable = qualificationScopeIdentity({ kind: 'mutation', release: savedDocuments.release,
    stack: 'postgres', reference: plan.references.entries.find(item => item.backend === 'postgres'),
    mutation: plan.mutations.find(item => item.backend === 'postgres'), stackBenchRoot: STACK_BENCH_ROOT });
  // Simulate an explicit review only inside this test. This does not qualify the current release.
  plan.qualificationReuse = { rationale: 'test-only executable equivalence', evidence: [], scopes: [{
    kind: 'mutation', stack: 'postgres',
    fromExecutableSha256: artifact.payload.qualificationScope.executableSha256,
    toExecutableSha256: executable.executableSha256,
  }] };
  const selected = calibrationQualificationRelease(plan, savedDocuments.release, []);
  const context = { calibration: plan, qualificationIdentity: calibrationQualificationIdentity(plan),
    ...selected, stackBenchRoot: STACK_BENCH_ROOT, references: plan.references.entries,
    qualificationDocuments: savedDocuments };
  assert.doesNotThrow(() => validateQualificationSlice(artifact, entry, context));
  // An omitted source selection means all recipe checks. Build a test-only
  // snapshot and receipt for that policy; never modify the retained fixture.
  const implicitSource = structuredClone(saved);
  delete implicitSource.calibration.qualification.checks;
  delete implicitSource.calibration.qualification.featureCatalog;
  const allKeys = new Set(savedDocuments.release.checkCatalog.map(check => check.stableKey));
  const allMutations = implicitSource.mutations.postgres.mutations
    .map((mutation: MutationDefinition) => mutationForRecipe(mutation, savedDocuments.release))
    .filter((mutation: MutationDefinition) => mutationTargetKeys(mutation).some(key => allKeys.has(key)));
  implicitSource.calibration.mutations.find((m: { backend: string }) => m.backend === 'postgres').executionSha256
    = mutationExecutionSha256({ ...implicitSource.mutations.postgres, mutations: allMutations });
  const implicitSourcePlan = structuredClone(implicitSource.calibration) as CalibrationPlan;
  implicitSourcePlan.mutations.find(m => m.backend === 'postgres')!.path = mutationPath;
  implicitSourcePlan.qualificationReuse = structuredClone(plan.qualificationReuse);
  const implicitArtifact = structuredClone(artifact);
  const implicitIdentity = calibrationQualificationIdentity(implicitSource.calibration);
  implicitArtifact.identities.calibration = { id: implicitIdentity.id, sha256: implicitIdentity.contentSha256 };
  const implicitSnapshotPath = join(temporary, 'implicit-inputs.json');
  const implicitArtifactPath = join(temporary, 'implicit-artifact.json');
  writeFileSync(implicitSnapshotPath, JSON.stringify(implicitSource));
  writeFileSync(implicitArtifactPath, JSON.stringify(implicitArtifact));
  const implicitEntry: CalibrationEvidence = { ...entry, path: relative(STACK_BENCH_ROOT, implicitArtifactPath),
    sha256: sha256(readFileSync(implicitArtifactPath)), slice: { checks: [...entry.slice!.checks],
      snapshot: { path: relative(STACK_BENCH_ROOT, implicitSnapshotPath), sha256: sha256(readFileSync(implicitSnapshotPath)) } } };
  assert.doesNotThrow(() => validateQualificationSlice(implicitArtifact, implicitEntry,
    { ...context, calibration: implicitSourcePlan }));
  const wrongImplicitArtifact = structuredClone(implicitArtifact);
  wrongImplicitArtifact.identities.calibration.sha256 = 'f'.repeat(64);
  assert.throws(() => validateQualificationSlice(wrongImplicitArtifact, implicitEntry,
    { ...context, calibration: implicitSourcePlan }), /mismatched recipe or calibration identities/);
  // A fixed reference does not rebuild from graph prompts. Rekeying a different
  // independent scenario must not discard this unchanged, measured slice.
  const rekeyed = structuredClone(savedDocuments);
  const oldKey = 'ecommerce.feature.accounts.accounts.1c';
  const newKey = 'ecommerce.spec.access-control.account-password.1c';
  const password = rekeyed.release.checkCatalog.find(check => check.stableKey === oldKey)!;
  Object.assign(password, { stableKey: newKey, stablePackId: 'ecommerce.spec.access-control',
    checkGroupId: 'account-password' });
  Object.assign((rekeyed.meaning.checks as Array<Record<string, unknown>>)
    .find(check => check.stableKey === oldKey)!, password);
  const passwordExecution = (rekeyed.execution.execution as Array<{ id: string;
    checkGroups: Array<Record<string, unknown>> }>).find(item => item.id === password.executionId)!;
  const passwordGroup = passwordExecution.checkGroups.find(group => group.checkGroupId === 'accounts')!;
  Object.assign(passwordGroup, { stablePackId: 'ecommerce.spec.access-control', checkGroupId: 'account-password' });
  rekeyed.release.meaningSha256 = sha256(canonicalDefinitionJson(rekeyed.meaning));
  rekeyed.release.executionSha256 = sha256(canonicalDefinitionJson(rekeyed.execution));
  rekeyed.release.contentSha256 = sha256(canonicalDefinitionJson({ schemaVersion: 3,
    meaningSha256: rekeyed.release.meaningSha256, executionSha256: rekeyed.release.executionSha256 }));
  validateQualificationDocuments(rekeyed);
  const rekeyedPlan = structuredClone(plan);
  rekeyedPlan.qualification.checks = rekeyedPlan.qualification.checks!.map(key => key === oldKey ? newKey : key);
  rekeyedPlan.qualification.featureCatalog!.contentSha256 = 'a'.repeat(64);
  const rekeyedContext = { ...context, calibration: rekeyedPlan,
    release: rekeyed.release, qualificationDocuments: rekeyed };
  assert.doesNotThrow(() => validateQualificationSlice(artifact, entry, rekeyedContext));
  const implicitSelection = structuredClone(rekeyedPlan);
  delete implicitSelection.qualification.checks;
  assert.doesNotThrow(() => validateQualificationSlice(artifact, entry,
    { ...rekeyedContext, calibration: implicitSelection }));
  const absentCurrent = structuredClone(rekeyedPlan);
  absentCurrent.qualification.checks = absentCurrent.qualification.checks!
    .filter(key => !entry.slice!.checks.includes(key));
  assert.throws(() => validateQualificationSlice(artifact, entry,
    { ...rekeyedContext, calibration: absentCurrent }), /absent from source or current qualification/);
  const absentSource = structuredClone(saved);
  absentSource.calibration.qualification.checks = absentSource.calibration.qualification.checks
    .filter((key: string) => !entry.slice!.checks.includes(key));
  const absentSourcePath = join(temporary, 'absent-source.json');
  writeFileSync(absentSourcePath, JSON.stringify(absentSource));
  const absentSourceEntry = structuredClone(entry);
  absentSourceEntry.slice!.snapshot = { path: relative(STACK_BENCH_ROOT, absentSourcePath),
    sha256: sha256(readFileSync(absentSourcePath)) };
  assert.throws(() => validateQualificationSlice(artifact, absentSourceEntry, rekeyedContext),
    /absent from source or current qualification/);
  const changedDependency = structuredClone(rekeyed);
  (changedDependency.meaning.checks as Array<Record<string, unknown>>)
    .find(check => check.stableKey === entry.slice!.checks[0])!.requiresFeatures = ['different-feature'];
  assert.throws(() => validateQualificationSlice(artifact, entry,
    { ...rekeyedContext, qualificationDocuments: changedDependency }), /changed check/);
  for (const change of [
    (p: CalibrationPlan) => { p.qualification.featureCatalog!.id = 'other-catalog'; },
    (p: CalibrationPlan) => { p.qualification.featureCatalog!.path = 'other.json'; },
    (p: CalibrationPlan) => { delete p.qualification.featureCatalog; },
    (p: CalibrationPlan) => { p.qualification.runner!.platform = 'other'; },
    (p: CalibrationPlan) => { p.qualification.exactCombinationRequired = false; },
  ]) {
    const bad = structuredClone(rekeyedPlan); change(bad);
    assert.throws(() => validateQualificationSlice(artifact, entry,
      { ...rekeyedContext, calibration: bad }), /source qualification policy differs/);
  }
  for (const field of ['fixture', 'nullControl', 'controls'] as const) {
    const bad = structuredClone(rekeyedPlan);
    if (field === 'controls') bad.controls.push({ stableKey: entry.slice!.checks[0]!,
      role: 'control', promotionPolicy: 'required', mutationTargets: [] });
    else Object.assign(bad[field], { changed: true });
    assert.throws(() => validateQualificationSlice(artifact, entry,
      { ...rekeyedContext, calibration: bad }), new RegExp(`source ${field} differs`));
  }
  // Failure cases specified before implementation: contract wording alone may
  // be reviewed, but wrong task hashes, absent or changed evidence, requirements,
  // ownership, and changed scenario steps must never receive that exemption.
  const revised = structuredClone(savedDocuments);
  const task = revised.meaning.task as { contracts: Array<{ text: string; owners: string[] }>;
    requirements: unknown[] };
  task.contracts[0]!.text += '\nAn optional link can open the public catalog.\n';
  const taskHash = (value: unknown) => sha256(canonicalDefinitionJson(value));
  const reviewPath = join(temporary, 'contract-review.json');
  writeFileSync(reviewPath, JSON.stringify({ rationale: 'Catalog navigation is optional; fixed reference source is unchanged.' }));
  const reviewed = structuredClone(plan);
  reviewed.qualificationReuse!.rationale = 'Review only changed catalog interface wording.';
  reviewed.qualificationReuse!.evidence = [{ path: relative(STACK_BENCH_ROOT, reviewPath), sha256: sha256(readFileSync(reviewPath)) }];
  reviewed.qualificationReuse!.contractTextEquivalences = [{
    fromTaskSha256: taskHash(savedDocuments.meaning.task), toTaskSha256: taskHash(revised.meaning.task),
  }];
  const reviewContext = { ...context, calibration: reviewed, qualificationDocuments: revised };
  assert.throws(() => validateQualificationSlice(artifact, entry,
    { ...reviewContext, calibration: plan }), /changed check/);
  assert.doesNotThrow(() => validateQualificationSlice(artifact, entry, reviewContext));
  const observations: Array<{ case: string; outcome: string }> = [{ case: 'reviewed contract text', outcome: 'accepted' }];
  for (const [name, change] of [
    ['wrong source task', (p: CalibrationPlan) => { p.qualificationReuse!.contractTextEquivalences![0]!.fromTaskSha256 = 'f'.repeat(64); }],
    ['wrong target task', (p: CalibrationPlan) => { p.qualificationReuse!.contractTextEquivalences![0]!.toTaskSha256 = 'f'.repeat(64); }],
    ['no evidence', (p: CalibrationPlan) => { p.qualificationReuse!.evidence = []; }],
    ['changed evidence', (p: CalibrationPlan) => { p.qualificationReuse!.evidence[0]!.sha256 = 'f'.repeat(64); }],
    ['empty rationale', (p: CalibrationPlan) => { p.qualificationReuse!.rationale = ''; }],
  ] as const) {
    const bad = structuredClone(reviewed); change(bad);
    assert.throws(() => validateQualificationSlice(artifact, entry, { ...reviewContext, calibration: bad }));
    observations.push({ case: name, outcome: 'rejected' });
  }
  for (const change of [
    (d: typeof revised) => { (d.meaning.task as typeof task).requirements = []; },
    (d: typeof revised) => { (d.meaning.task as typeof task).contracts[0]!.owners = ['different-owner']; },
  ]) {
    const bad = structuredClone(revised); change(bad);
    const exactHashes = structuredClone(reviewed);
    exactHashes.qualificationReuse!.contractTextEquivalences![0]!.toTaskSha256 = taskHash(bad.meaning.task);
    assert.throws(() => validateQualificationSlice(artifact, entry,
      { ...reviewContext, calibration: exactHashes, qualificationDocuments: bad }), /changed check/);
  }
  const changedScenario = structuredClone(revised);
  const catalogKey = 'ecommerce.feature.catalog.catalog-values.2a';
  const catalog = changedScenario.release.checkCatalog.find(check => check.stableKey === catalogKey)!;
  const execution = (changedScenario.execution.execution as Array<Record<string, unknown>>)
    .find(item => item.id === catalog.executionId)!;
  execution.checkGroups = [];
  const unchanged = unchangedQualificationChecks(savedDocuments, changedScenario,
    reviewed.qualificationReuse!.contractTextEquivalences);
  assert(!unchanged.has(catalogKey), 'The review must not admit changed catalog scenario steps');
  assert(unchanged.has(entry.slice!.checks[0]!), 'Unchanged independent checks remain reusable');
  observations.push({ case: 'changed requirements, owners and scenario', outcome: 'rejected' });
  if (process.env.STACK_BENCH_SLICE_EVIDENCE) writeFileSync(process.env.STACK_BENCH_SLICE_EVIDENCE,
    JSON.stringify({ rerun: 'STACK_BENCH_SLICE_EVIDENCE=<file> node --test dist/tests/qualification-slices.test.js',
      artifact: { path: entry.path, sha256: entry.sha256 }, snapshot: entry.slice!.snapshot,
      review: reviewed.qualificationReuse!.contractTextEquivalences, observations }, null, 2));
  assert.equal(plan.qualification.stacks.length, 3);
  const expandedStacks = structuredClone(plan);
  expandedStacks.qualification.stacks.push('convex');
  assert.doesNotThrow(() => validateQualificationSlice(artifact, entry,
    { ...context, calibration: expandedStacks }));
  const removedStack = structuredClone(expandedStacks);
  removedStack.qualification.stacks = removedStack.qualification.stacks.filter(stack => stack !== 'postgres');
  assert.throws(() => validateQualificationSlice(artifact, entry,
    { ...context, calibration: removedStack }), /measured stack is absent/);
  for (const change of [
    (p: CalibrationPlan) => { p.qualification.referenceRepetitions += 1; },
    (p: CalibrationPlan) => { p.qualification.mutationRepetitions += 1; },
  ]) {
    const changedPolicy = structuredClone(expandedStacks);
    change(changedPolicy);
    assert.throws(() => validateQualificationSlice(artifact, entry,
      { ...context, calibration: changedPolicy }), /source qualification policy differs/);
  }
  for (const change of [
    (a: typeof artifact) => { a.payload.runs[0].mutations.total -= 1; },
    (a: typeof artifact) => { a.payload.runs[0].ok = false; },
    (a: typeof artifact) => { a.payload.runner.platform = 'wrong'; },
    (a: typeof artifact) => { a.payload.qualifiedCheckKeys = []; },
    (a: typeof artifact) => { a.identities.recipe.sha256 = 'f'.repeat(64); },
    (a: typeof artifact) => { a.identities.fixture.sha256 = 'f'.repeat(64); },
    (a: typeof artifact) => { a.payload.qualificationScope.sha256 = 'f'.repeat(64); },
  ]) {
    const bad = structuredClone(artifact);
    change(bad);
    assert.throws(() => validateQualificationSlice(bad, entry, context));
  }
  const staleSnapshot = structuredClone(entry);
  staleSnapshot.slice!.snapshot.sha256 = 'f'.repeat(64);
  assert.throws(() => validateQualificationSlice(artifact, staleSnapshot, context), /stale digest/);
  assert.throws(() => validateQualificationSlice(artifact, entry,
    { ...context, calibration: { ...plan, qualificationReuse: undefined } }), /executable changed/);
  const wrongEquivalence = structuredClone(plan);
  wrongEquivalence.qualificationReuse!.scopes[0]!.toExecutableSha256 = 'f'.repeat(64);
  assert.throws(() => validateQualificationSlice(artifact, entry,
    { ...context, calibration: wrongEquivalence }), /executable changed/);
  const changedReference = structuredClone(plan);
  changedReference.references.entries.find(item => item.backend === 'postgres')!.sourceSha256 = 'f'.repeat(64);
  assert.throws(() => validateQualificationSlice(artifact, entry,
    { ...context, calibration: changedReference }), /source references differs/);
  // Failure cases before implementation: only an exact, evidenced reference
  // source pair may reuse named unchanged checks. It cannot excuse a changed
  // scenario, mutation, fixture, policy, or an unreviewed reference identity.
  const sourceReference = plan.references.entries.find(item => item.backend === 'postgres')!;
  const referenceReview = {
    stack: 'postgres', referenceId: sourceReference.id,
    fromSourceSha256: sourceReference.sourceSha256, toSourceSha256: 'f'.repeat(64),
    checks: [...entry.slice!.checks],
  };
  const referencePlan = structuredClone(changedReference);
  const referenceReuse = referencePlan.qualificationReuse!;
  referenceReuse.referenceSourceEquivalences = [referenceReview];
  referenceReuse.rationale = 'Only support submission receipt UI changed; this review check is unchanged.';
  referenceReuse.evidence = [{ path: relative(STACK_BENCH_ROOT, reviewPath), sha256: sha256(readFileSync(reviewPath)) }];
  const referenceContext = { ...context, calibration: referencePlan, references: referencePlan.references.entries };
  const referenceManifest = structuredClone(saved.mutations.postgres);
  referenceManifest.fixtureSha256 = referenceReview.toSourceSha256;
  writeFileSync(mutationPath, JSON.stringify(referenceManifest));
  const referenceObservations: Array<{ case: string; outcome: string }> = [];
  for (const [field, value] of [
    ['stack', 'mongodb'], ['referenceId', 'different-reference'],
    ['fromSourceSha256', 'e'.repeat(64)], ['toSourceSha256', 'e'.repeat(64)], ['checks', ['unreviewed']],
  ] as const) {
    const bad = structuredClone(referencePlan);
    const review = bad.qualificationReuse!.referenceSourceEquivalences![0]!;
    Object.assign(review, { [field]: value });
    assert.throws(() => validateQualificationSlice(artifact, entry, { ...referenceContext, calibration: bad }), field);
    referenceObservations.push({ case: `wrong ${field}`, outcome: 'rejected' });
  }
  for (const [name, change] of [
    ['no review', (p: CalibrationPlan) => { delete p.qualificationReuse!.referenceSourceEquivalences; }],
    ['no evidence', (p: CalibrationPlan) => { p.qualificationReuse!.evidence = []; }],
    ['tampered evidence', (p: CalibrationPlan) => { p.qualificationReuse!.evidence[0]!.sha256 = 'e'.repeat(64); }],
    ['empty rationale', (p: CalibrationPlan) => { p.qualificationReuse!.rationale = ''; }],
    ['changed fixture', (p: CalibrationPlan) => { p.fixture.sourceSha256 = 'e'.repeat(64); }],
    ['changed policy', (p: CalibrationPlan) => { p.qualification.referenceRepetitions += 1; }],
    ['changed reference ID', (p: CalibrationPlan) => { p.references.entries.find(r => r.backend === 'postgres')!.id = 'other'; }],
  ] as const) {
    const bad = structuredClone(referencePlan); change(bad);
    assert.throws(() => validateQualificationSlice(artifact, entry,
      { ...referenceContext, calibration: bad, references: bad.references.entries }), name);
    referenceObservations.push({ case: name, outcome: 'rejected' });
  }
  for (const field of ['setup', 'checkGroups']) {
    const changed = structuredClone(savedDocuments);
    const check = changed.release.checkCatalog.find(check => check.stableKey === entry.slice!.checks[0])!;
    const execution = (changed.execution.execution as Array<Record<string, unknown>>).find(item => item.id === check.executionId)!;
    execution[field] = [{ changed: true }];
    assert.throws(() => validateQualificationSlice(artifact, entry,
      { ...referenceContext, qualificationDocuments: changed }));
    referenceObservations.push({ case: `changed ${field}`, outcome: 'rejected' });
  }
  for (const change of [
    (m: typeof referenceManifest) => { m.fixtureSha256 = 'e'.repeat(64); },
    (m: typeof referenceManifest) => { m.mutations = []; },
  ]) {
    const changed = structuredClone(referenceManifest); change(changed);
    writeFileSync(mutationPath, JSON.stringify(changed));
    assert.throws(() => validateQualificationSlice(artifact, entry, referenceContext));
  }
  writeFileSync(mutationPath, JSON.stringify(referenceManifest));
  const originalArtifact = JSON.stringify(artifact);
  assert.doesNotThrow(() => validateQualificationSlice(artifact, entry, referenceContext));
  // Reference coverage may use the same measured positive baseline from this
  // mutation receipt; both paths must retain the original fixture identity.
  assert.doesNotThrow(() => validateQualificationSlice(artifact, { ...entry, kind: 'reference' }, referenceContext));
  assert.equal(JSON.stringify(artifact), originalArtifact);
  referenceObservations.push({ case: 'exact source pair and unchanged check', outcome: 'accepted' });
  const definition = JSON.parse(readFileSync(path, 'utf8'));
  definition.qualificationReuse = referenceReuse;
  assert.doesNotThrow(() => compileCalibrationDefinition(definition));
  for (const change of [
    (r: typeof referenceReview) => { r.checks = []; },
    (r: typeof referenceReview) => { r.checks.push(r.checks[0]!); },
    (r: typeof referenceReview) => { r.fromSourceSha256 = r.toSourceSha256; },
    (r: typeof referenceReview) => { r.toSourceSha256 = 'invalid'; },
  ]) {
    const bad = structuredClone(definition); change(bad.qualificationReuse.referenceSourceEquivalences[0]);
    assert.throws(() => compileCalibrationDefinition(bad));
  }
  if (process.env.STACK_BENCH_SLICE_EVIDENCE) {
    const proof = JSON.parse(readFileSync(process.env.STACK_BENCH_SLICE_EVIDENCE, 'utf8'));
    proof.referenceSourceReview = { review: referenceReview, observations: referenceObservations,
      sourceArtifactUnchanged: sha256(originalArtifact) === sha256(JSON.stringify(artifact)) };
    writeFileSync(process.env.STACK_BENCH_SLICE_EVIDENCE, JSON.stringify(proof, null, 2));
  }
  writeFileSync(mutationPath, JSON.stringify(saved.mutations.postgres));
  const unrelatedReference = structuredClone(plan);
  unrelatedReference.references.entries.find(item => item.backend === 'spacetime')!.sourceSha256 = 'f'.repeat(64);
  assert.doesNotThrow(() => validateQualificationSlice(artifact, entry,
    { ...context, calibration: unrelatedReference, references: unrelatedReference.references.entries }));
  const removedReference = structuredClone(plan);
  removedReference.references.entries = removedReference.references.entries.filter(item => item.backend !== 'postgres');
  assert.throws(() => validateQualificationSlice(artifact, entry,
    { ...context, calibration: removedReference, references: removedReference.references.entries }), /source references differs/);
  const changedControls = structuredClone(saved.mutations.postgres);
  changedControls.mutations = [];
  writeFileSync(mutationPath, JSON.stringify(changedControls));
  assert.throws(() => validateQualificationSlice(artifact, entry, context), /slice mutation controls changed/);
  const additionalTargets = structuredClone(saved.mutations.postgres);
  const selectedMutation = additionalTargets.mutations.find((mutation: { targets: string[] }) =>
    mutation.targets.some(key => entry.slice!.checks.includes(key)));
  assert(selectedMutation);
  const outsideSlice = savedDocuments.release.checkCatalog.find(check =>
    !entry.slice!.checks.includes(check.stableKey))!.stableKey;
  selectedMutation.targets.push(outsideSlice);
  writeFileSync(mutationPath, JSON.stringify(additionalTargets));
  assert.throws(() => validateQualificationSlice(artifact, entry, context), /mutation spans slice boundary/);
  selectedMutation.targets.pop();
  selectedMutation.targets.push('ecommerce.future.not-in-this-recipe');
  writeFileSync(mutationPath, JSON.stringify(additionalTargets));
  assert.doesNotThrow(() => validateQualificationSlice(artifact, entry, context));
  writeFileSync(mutationPath, JSON.stringify(saved.mutations.postgres));

  const nullEntry: CalibrationEvidence = { ...entry, kind: 'null', stack: undefined,
    path: 'tests/fixtures/qualification-evidence/ecommerce-l3-e804c1302/null-targeted.json',
    sha256: '8ad52d3b9f0274cae90761a0e46f7184a111852ab511a4e90702c1cd02677b58' };
  const nullArtifact = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, nullEntry.path), 'utf8'));
  const nullPlan = structuredClone(plan);
  nullPlan.qualification.stacks.push('convex');
  const nullScope = qualificationScopeIdentity({ kind: 'null', release: savedDocuments.release,
    stackBenchRoot: STACK_BENCH_ROOT });
  nullPlan.qualificationReuse!.scopes = [{ kind: 'null',
    fromExecutableSha256: nullArtifact.payload.qualificationScope.executableSha256,
    toExecutableSha256: nullScope.executableSha256 }];
  for (const reference of nullPlan.references.entries) reference.sourceSha256 = 'f'.repeat(64);
  const nullContext = { ...context, calibration: nullPlan, references: nullPlan.references.entries };
  assert.doesNotThrow(() => validateQualificationSlice(nullArtifact, nullEntry, nullContext));
  nullPlan.fixture.sourceSha256 = 'f'.repeat(64);
  assert.throws(() => validateQualificationSlice(nullArtifact, nullEntry, nullContext), /source fixture differs/);

  const stale: CalibrationEvidence = { ...entry, kind: 'reference',
    path: 'tests/fixtures/qualification-evidence/ecommerce-l3-7cd96d01b/postgres-reference.json',
    sha256: 'baaba1487f36fa51907c8ff4fc866997fc2625097e7fe333b3d2250801ed9baf',
    slice: { checks: entry.slice!.checks,
      snapshot: { path: 'tests/fixtures/qualification-evidence/ecommerce-l3-e804c1302/previous-inputs.json',
        sha256: 'ef95150fbc546ecbdc6a6e3c7a2ee0b4945dce127b9ad43fac3fc0971f3f01a5' } } };
  const previousArtifact = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, stale.path), 'utf8'));
  assert.throws(() => validateQualificationSlice(previousArtifact, stale, context), /changed check/);

  const manifest = JSON.parse(readFileSync(path, 'utf8'));
  manifest.qualification.evidence = [structuredClone(entry)];
  manifest.qualification.evidence[0].slice.checks = [];
  assert.throws(() => compileCalibrationDefinition(manifest), /non-empty/);
  manifest.qualification.evidence[0].slice.checks = ['duplicate', 'duplicate'];
  assert.throws(() => compileCalibrationDefinition(manifest), /duplicates checks/);
  manifest.qualification.evidence[0].slice.checks = ['one'];
  manifest.qualification.evidence[0].slice.untrusted = true;
  assert.throws(() => compileCalibrationDefinition(manifest), /unknown field/);
});
