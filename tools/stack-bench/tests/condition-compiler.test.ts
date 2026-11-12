import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { resolveGuidanceProfile, resolveStudyConditions,
  validateConditionReference } from '../src/campaigns/condition-compiler.js';

const prescribed = { id: 'prescribed',
  guidanceProfile: 'prescribed', repairPolicy: 'scored-only' };
const requested = { track: 'example', levels: [{ level: 1,
  recipe: { id: 'example.l1', contentSha256: 'a'.repeat(64),
    meaningSha256: 'b'.repeat(64), executionSha256: 'c'.repeat(64) },
  selection: { sha256: 'd'.repeat(64), completeness: 'full', scoredPoints: 10,
    taskPacks: ['example.core'], requested: { packs: [], checks: [] } },
  task: { sha256: 'e'.repeat(64), requirementSha256: 'f'.repeat(64),
    contractSha256: '1'.repeat(64), requirementIds: ['example.requirement'],
    contractIds: ['example.contract'] } }] };

const writeJson = (path: string, value: unknown): void => {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
};

test('production framing is explicit and does not change legacy or grading scope', () => {
  const resolve = (productionQuality?: boolean) => resolveStudyConditions([
    { ...prescribed, ...(productionQuality === undefined ? {} : { productionQuality }) },
  ], ['mongodb'], { requested })[0];
  const legacy = resolve(), enabled = resolve(true), disabled = resolve(false);
  assert.equal(Object.hasOwn(legacy, 'productionQuality'), false);
  assert.deepEqual(resolve(), legacy);
  assert.equal(enabled.productionQuality, true);
  assert.equal(disabled.productionQuality, false);
  assert.notEqual(enabled.contentSha256, disabled.contentSha256);
  assert.notEqual(enabled.contentSha256, legacy.contentSha256);
  assert.deepEqual(enabled.requested, legacy.requested);
  assert.deepEqual(enabled.guidance, legacy.guidance);
  assert.throws(() => validateConditionReference({ ...prescribed, productionQuality: 'true' }), /must be a boolean/);
});

test('dev workflow is opt-in and changes only the SpacetimeDB skill identity', () => {
  const stacks = ['spacetime', 'mongodb', 'postgres'];
  const neutral = resolveGuidanceProfile('neutral', stacks);
  const dev = resolveGuidanceProfile('neutral-dev', stacks);
  assert.deepEqual(dev.documents, neutral.documents);
  assert.deepEqual(dev.credentialAliases, neutral.credentialAliases);
  assert.deepEqual(dev.material, neutral.material);
  for (const stack of ['mongodb', 'postgres']) {
    assert.deepEqual(dev.skills[stack], neutral.skills[stack]);
  }
  assert.deepEqual(dev.skills.spacetime!.ids,
    [...neutral.skills.spacetime!.ids, 'spacetime-dev']);
  assert.notEqual(dev.skills.spacetime!.sha256, neutral.skills.spacetime!.sha256);
  assert.notEqual(dev.contentSha256, neutral.contentSha256);
  const managed = resolveGuidanceProfile('neutral-managed-dev', stacks);
  assert.deepEqual(managed.documents, neutral.documents);
  for (const stack of ['mongodb', 'postgres']) assert.deepEqual(managed.skills[stack], neutral.skills[stack]);
  assert.deepEqual(managed.skills.spacetime!.ids, [...neutral.skills.spacetime!.ids, 'spacetime-managed-dev']);
  assert.notEqual(managed.contentSha256, dev.contentSha256);
});

test('the prescribed condition binds independent guidance, repair, and document identities', () => {
  const [condition] = resolveStudyConditions([prescribed], ['mongodb', 'postgres', 'spacetime'],
    { requested });
  assert.match(condition.contentSha256, /^[a-f0-9]{64}$/);
  assert.equal(condition.guidance.mode, 'prescribed');
  assert.equal(condition.guidance.material.designAdvice, true);
  assert.deepEqual(Object.keys(condition.guidance.documents), ['mongodb', 'postgres', 'spacetime']);
  const spacetimeSkills = condition.guidance.skills.spacetime;
  const mongodbSkills = condition.guidance.skills.mongodb;
  assert.ok(spacetimeSkills);
  assert.ok(mongodbSkills);
  assert.deepEqual(spacetimeSkills.ids,
    ['typescript-server', 'typescript-client', 'cli']);
  assert.deepEqual(mongodbSkills.ids, []);
  assert.deepEqual(condition.guidance.credentialAliases, {
    'stackbench-admin-2026': 'store-admin-2026',
    'stackbench-customer-2026': 'store-customer-2026',
    'stackbench-staff-2026': 'store-staff-2026',
  });
  assert.match(spacetimeSkills.sha256, /^[a-f0-9]{64}$/);
  assert.equal(condition.repair.scoredEvidence, true);
  assert.equal(condition.repair.observedEvidence, false);
  assert.equal(condition.repair.scenarioValues, 'failed-observations');
  assert.deepEqual(condition.requested, requested);
  const requestedLevel = requested.levels[0];
  assert.ok(requestedLevel);
  const [changed] = resolveStudyConditions([prescribed], ['mongodb', 'postgres', 'spacetime'],
    { requested: { ...requested, levels: [{ ...requestedLevel, selection: {
      ...requestedLevel.selection, sha256: 'e'.repeat(64),
    } }] } });
  assert.notEqual(changed.contentSha256, condition.contentSha256);
});

test('neutral guidance uses current stack documents, skills, and credential aliases', () => {
  const profile = resolveGuidanceProfile('neutral', ['mongodb', 'postgres', 'spacetime']);
  assert.equal(profile.material.designAdvice, true);
  assert.deepEqual(Object.keys(profile.documents), ['mongodb', 'postgres', 'spacetime']);
  assert.deepEqual(profile.skills.spacetime?.ids, ['typescript-server', 'typescript-client', 'cli']);
  assert.deepEqual(profile.credentialAliases, {
    'stackbench-admin-2026': 'store-admin-2026',
    'stackbench-customer-2026': 'store-customer-2026',
    'stackbench-staff-2026': 'store-staff-2026',
  });
  const [condition] = resolveStudyConditions([
    { id: 'neutral', guidanceProfile: 'neutral', repairPolicy: 'scored-only' },
  ], ['mongodb', 'postgres', 'spacetime'], { requested });
  assert.equal(condition.guidance.mode, 'neutral');
  assert.equal(condition.guidance.material.designAdvice, true);
  assert.deepEqual(Object.keys(condition.guidance.documents), ['mongodb', 'postgres', 'spacetime']);
});

test('condition references use stable IDs', () => {
  assert.deepEqual(validateConditionReference(prescribed), prescribed);
  assert.throws(() => validateConditionReference({ ...prescribed, surprise: true }), /surprise.*unknown/);
  assert.throws(() => validateConditionReference({ ...prescribed, repairPolicy: 'scored-only@1.1.0' }), /must use an id/);
  assert.throws(() => resolveStudyConditions([prescribed], ['postgres']), /requested/);
});

test('modular condition references are canonical without mutating caller-owned input', () => {
  const input = { ...prescribed, specifications: { levels: [
    { level: 2, requested: ['example.spec.second'], expected: [], observed: [] },
    { level: 1, requested: [], expected: ['example.spec.first'], observed: [] },
  ] } };
  const original = structuredClone(input);
  const validated = validateConditionReference(input);
  assert.deepEqual(input, original);
  assert.ok(validated.specifications);
  assert.deepEqual(validated.specifications.levels.map(entry => entry.level), [1, 2]);
  assert.throws(() => validateConditionReference({ ...prescribed, specifications: { levels: [
    { level: 1, requested: ['example.spec.same'],
      expected: ['example.spec.same'], observed: [] },
  ] } }), /both requested and expected/);
});

function customCondition({ guidance = {}, repair = {} } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-condition-'));
  const catalogRoot = join(root, 'conditions');
  writeFileSync(join(root, 'backend.md'), 'connection facts\n');
  writeJson(join(catalogRoot, 'catalog.json'), { schemaVersion: 1, kind: 'study-condition-catalog',
    guidanceProfiles: { neutral: 'guidance.json' },
    repairPolicies: { scored: 'repair.json' } });
  writeJson(join(catalogRoot, 'guidance.json'), { schemaVersion: 1, kind: 'backend-guidance-profile',
    id: 'neutral', mode: 'neutral',
    material: { accessFacts: true, apiReference: true, designAdvice: false },
    documents: { postgres: 'backend.md' }, applicationInterfaces: { postgres: 'http' },
    skills: { postgres: [] }, ...guidance });
  writeJson(join(catalogRoot, 'repair.json'), { schemaVersion: 1, kind: 'repair-policy',
    id: 'scored', scoredEvidence: true,
    observedEvidence: false, scenarioValues: 'failed-observations', ...repair });
  const ref = { id: 'defaults', guidanceProfile: 'neutral', repairPolicy: 'scored' };
  return { root, catalogPath: join(catalogRoot, 'catalog.json'), ref };
}

test('guidance records selected design advice and requires each stack document and interface', () => {
  const advice = customCondition({ guidance: {
    material: { accessFacts: true, apiReference: true, designAdvice: true },
  } });
  try {
    assert.equal(resolveStudyConditions([advice.ref], ['postgres'], {
      stackBenchRoot: advice.root, catalogPath: advice.catalogPath, requested,
    })[0]!.guidance.material.designAdvice, true);
  } finally { rmSync(advice.root, { recursive: true, force: true }); }

  const missing = customCondition();
  try {
    assert.throws(() => resolveStudyConditions([missing.ref], ['mongodb'], {
      stackBenchRoot: missing.root, catalogPath: missing.catalogPath, requested,
    }), /documents.mongodb.*required/);
  } finally { rmSync(missing.root, { recursive: true, force: true }); }

  const mismatched = customCondition({ guidance: { applicationInterfaces: { postgres: 'reducer' } } });
  try {
    assert.throws(() => resolveStudyConditions([mismatched.ref], ['postgres'], {
      stackBenchRoot: mismatched.root, catalogPath: mismatched.catalogPath, requested,
    }), /applicationInterfaces.postgres.*must be http/);
  } finally { rmSync(mismatched.root, { recursive: true, force: true }); }
});

test('observed-only evidence can never enter repairs and scored evidence remains available', () => {
  for (const overrides of [{ repair: { observedEvidence: true } },
    { repair: { scoredEvidence: false } }, { repair: { scenarioValues: 'disclosed' } },
    { repair: { scenarioValues: 'withheld' } }]) {
    const fixture = customCondition(overrides);
    try {
      assert.throws(() => resolveStudyConditions([fixture.ref], ['postgres'], {
        stackBenchRoot: fixture.root, catalogPath: fixture.catalogPath, requested,
      }), /observedEvidence|scoredEvidence|scenarioValues/);
    } finally { rmSync(fixture.root, { recursive: true, force: true }); }
  }
});
