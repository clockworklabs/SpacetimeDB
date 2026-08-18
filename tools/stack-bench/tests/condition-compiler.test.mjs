import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { resolveGuidanceProfile, resolveStudyConditions,
  validateConditionReference } from '../condition-compiler.mjs';

const prescribed = { id: 'prescribed', version: '1.0.0',
  guidanceProfile: 'prescribed@1.0.0', repairPolicy: 'scored-only@1.0.0' };
const requested = { track: 'example', levels: [{ level: 1,
  recipe: { id: 'example.l1', version: '1.0.0', contentSha256: 'a'.repeat(64),
    meaningSha256: 'b'.repeat(64), executionSha256: 'c'.repeat(64), state: 'qualified' },
  selection: { sha256: 'd'.repeat(64), completeness: 'full', scoredPoints: 10,
    taskPacks: ['example.core'], requested: { packs: [], checks: [] } },
  task: { sha256: 'e'.repeat(64), requirementSha256: 'f'.repeat(64),
    contractSha256: '1'.repeat(64), requirementIds: ['example.requirement'],
    contractIds: ['example.contract'] } }] };

const modularRequested = { track: 'example', levels: [{ ...requested.levels[0],
  selection: { schemaVersion: 3, sha256: 'd'.repeat(64), scoredPoints: 2,
    requested: { features: ['example.feature'], specifications: {
      requested: [], expected: ['example.spec@1.0.0'], observed: [],
    }, checks: [] },
    promptPacks: ['example.feature'], features: ['example.feature'],
    specifications: { requested: [], expected: ['example.spec@1.0.0'], observed: [] },
    scoredChecks: [
      { stableKey: 'example.feature.check', points: 1, treatment: 'requested' },
      { stableKey: 'example.spec.check', points: 1, treatment: 'expected' },
    ], observedChecks: [] },
}] };

const writeJson = (path, value) => {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
};

test('the prescribed condition binds independent guidance, repair, and document identities', () => {
  const [condition] = resolveStudyConditions([prescribed], ['mongodb', 'postgres', 'spacetime'],
    { requested });
  assert.match(condition.sha256, /^[a-f0-9]{64}$/);
  assert.equal(condition.guidance.mode, 'prescribed');
  assert.equal(condition.guidance.material.designAdvice, true);
  assert.deepEqual(Object.keys(condition.guidance.documents), ['mongodb', 'postgres', 'spacetime']);
  assert.deepEqual(condition.guidance.skills.spacetime.ids,
    ['typescript-server', 'typescript-client']);
  assert.deepEqual(condition.guidance.skills.mongodb.ids, []);
  assert.match(condition.guidance.skills.spacetime.sha256, /^[a-f0-9]{64}$/);
  assert.equal(condition.repair.scoredEvidence, true);
  assert.equal(condition.repair.observedEvidence, false);
  assert.deepEqual(condition.requested, requested);
  const [changed] = resolveStudyConditions([prescribed], ['mongodb', 'postgres', 'spacetime'],
    { requested: { ...requested, levels: [{ ...requested.levels[0], selection: {
      ...requested.levels[0].selection, sha256: 'e'.repeat(64),
    } }] } });
  assert.notEqual(changed.sha256, condition.sha256);
});

test('packaged neutral guidance exists symmetrically without architecture advice', () => {
  const neutral = { id: 'neutral', version: '1.0.0', guidanceProfile: 'neutral@1.0.0',
    repairPolicy: 'scored-only@1.0.0' };
  const [condition] = resolveStudyConditions([neutral], ['mongodb', 'postgres', 'spacetime'],
    { requested });
  assert.equal(condition.guidance.state, 'qualified');
  assert.equal(condition.guidance.mode, 'neutral');
  assert.equal(condition.guidance.material.designAdvice, false);
  assert.deepEqual(Object.keys(condition.guidance.documents), ['mongodb', 'postgres', 'spacetime']);
});

test('neutral guidance 1.1 adds only the SpacetimeDB packaging contract', () => {
  const oldProfile = resolveGuidanceProfile('neutral@1.0.0', ['mongodb', 'postgres', 'spacetime']);
  const nextProfile = resolveGuidanceProfile('neutral@1.1.0', ['mongodb', 'postgres', 'spacetime']);
  assert.equal(nextProfile.state, 'qualified');
  assert.equal(nextProfile.material.designAdvice, false);
  assert.deepEqual(nextProfile.documents.mongodb, oldProfile.documents.mongodb);
  assert.deepEqual(nextProfile.documents.postgres, oldProfile.documents.postgres);
  assert.notDeepEqual(nextProfile.documents.spacetime, oldProfile.documents.spacetime);
});

test('expected modular specifications are scored under the ordinary repair policy', () => {
  const selected = { id: 'defaults', version: '1.0.0', guidanceProfile: 'neutral@1.0.0',
    repairPolicy: 'scored-only@1.0.0' };
  const [condition] = resolveStudyConditions([selected], ['mongodb', 'postgres', 'spacetime'],
    { requested: modularRequested });
  assert.deepEqual(condition.requested.levels[0].selection.specifications.expected,
    ['example.spec@1.0.0']);
  assert.equal(condition.repair.scoredEvidence, true);
});

test('condition references are strict and versioned', () => {
  assert.deepEqual(validateConditionReference(prescribed), prescribed);
  assert.throws(() => validateConditionReference({ ...prescribed, surprise: true }), /surprise.*unknown/);
  assert.throws(() => validateConditionReference({ ...prescribed, repairPolicy: 'scored-only' }), /id@version/);
  assert.throws(() => resolveStudyConditions([prescribed], ['postgres']), /requested/);
});

test('modular condition references are canonical without mutating caller-owned input', () => {
  const input = { ...prescribed, specifications: { levels: [
    { level: 2, requested: ['example.spec.second@1.0.0'], expected: [], observed: [] },
    { level: 1, requested: [], expected: ['example.spec.first@1.0.0'], observed: [] },
  ] } };
  const original = structuredClone(input);
  const validated = validateConditionReference(input);
  assert.deepEqual(input, original);
  assert.deepEqual(validated.specifications.levels.map(entry => entry.level), [1, 2]);
  assert.throws(() => validateConditionReference({ ...prescribed, specifications: { levels: [
    { level: 1, requested: ['example.spec.same@1.0.0'],
      expected: ['example.spec.same@1.0.0'], observed: [] },
  ] } }), /both requested and expected/);
});

function customCondition({ guidance = {}, repair = {} } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-condition-'));
  const catalogRoot = join(root, 'conditions');
  writeFileSync(join(root, 'backend.md'), 'connection facts\n');
  writeJson(join(catalogRoot, 'catalog.json'), { schemaVersion: 1, kind: 'study-condition-catalog',
    guidanceProfiles: { 'neutral@1.0.0': 'guidance.json' },
    repairPolicies: { 'scored@1.0.0': 'repair.json' } });
  writeJson(join(catalogRoot, 'guidance.json'), { schemaVersion: 1, kind: 'backend-guidance-profile',
    id: 'neutral', version: '1.0.0', state: 'qualified', mode: 'neutral',
    material: { accessFacts: true, apiReference: true, designAdvice: false },
    documents: { fake: 'backend.md' }, skills: { fake: [] }, ...guidance });
  writeJson(join(catalogRoot, 'repair.json'), { schemaVersion: 1, kind: 'repair-policy',
    id: 'scored', version: '1.0.0', state: 'qualified', scoredEvidence: true,
    observedEvidence: false, ...repair });
  const ref = { id: 'defaults', version: '1.0.0', guidanceProfile: 'neutral@1.0.0',
    repairPolicy: 'scored@1.0.0' };
  return { root, catalogPath: join(catalogRoot, 'catalog.json'), ref };
}

test('neutral guidance cannot smuggle design advice or omit a selected stack document', () => {
  const advice = customCondition({ guidance: {
    material: { accessFacts: true, apiReference: true, designAdvice: true },
  } });
  try {
    assert.throws(() => resolveStudyConditions([advice.ref], ['fake'], {
      stackBenchRoot: advice.root, catalogPath: advice.catalogPath, requested,
    }), /designAdvice.*false/);
  } finally { rmSync(advice.root, { recursive: true, force: true }); }

  const missing = customCondition();
  try {
    assert.throws(() => resolveStudyConditions([missing.ref], ['other'], {
      stackBenchRoot: missing.root, catalogPath: missing.catalogPath, requested,
    }), /documents.other.*required/);
  } finally { rmSync(missing.root, { recursive: true, force: true }); }
});

test('observed-only evidence can never enter repairs and scored evidence remains available', () => {
  for (const overrides of [{ repair: { observedEvidence: true } },
    { repair: { scoredEvidence: false } }]) {
    const fixture = customCondition(overrides);
    try {
      assert.throws(() => resolveStudyConditions([fixture.ref], ['fake'], {
        stackBenchRoot: fixture.root, catalogPath: fixture.catalogPath, requested,
      }), /observedEvidence|scoredEvidence/);
    } finally { rmSync(fixture.root, { recursive: true, force: true }); }
  }
});
