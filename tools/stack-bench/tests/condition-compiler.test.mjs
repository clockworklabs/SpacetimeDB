import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { resolveStudyConditions, validateConditionReference } from '../condition-compiler.mjs';

const prescribed = { id: 'prescribed', version: '1.0.0',
  guidanceProfile: 'prescribed@1.0.0', probeProfile: 'none@1.0.0',
  repairPolicy: 'requested-only@1.0.0' };

const writeJson = (path, value) => {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
};

test('the prescribed condition binds independent guidance, probe, repair, and document identities', () => {
  const [condition] = resolveStudyConditions([prescribed], ['mongodb', 'postgres', 'spacetime']);
  assert.match(condition.sha256, /^[a-f0-9]{64}$/);
  assert.equal(condition.guidance.mode, 'prescribed');
  assert.equal(condition.guidance.material.designAdvice, true);
  assert.deepEqual(Object.keys(condition.guidance.documents), ['mongodb', 'postgres', 'spacetime']);
  assert.deepEqual(condition.probes.probes, []);
  assert.equal(condition.probes.scoreContribution, false);
  assert.equal(condition.probes.repairVisible, false);
  assert.equal(condition.repair.requestedEvidence, true);
  assert.equal(condition.repair.probeEvidence, false);
});

test('condition references are strict and versioned', () => {
  assert.deepEqual(validateConditionReference(prescribed), prescribed);
  assert.throws(() => validateConditionReference({ ...prescribed, surprise: true }), /surprise.*unknown/);
  assert.throws(() => validateConditionReference({ ...prescribed, probeProfile: 'none' }), /id@version/);
});

function customCondition({ guidance = {}, probes = {}, repair = {} } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-condition-'));
  const catalogRoot = join(root, 'conditions');
  writeFileSync(join(root, 'backend.md'), 'connection facts\n');
  writeJson(join(catalogRoot, 'catalog.json'), { schemaVersion: 1, kind: 'study-condition-catalog',
    guidanceProfiles: { 'neutral@1.0.0': 'guidance.json' },
    probeProfiles: { 'defaults@1.0.0': 'probes.json' },
    repairPolicies: { 'requested@1.0.0': 'repair.json' } });
  writeJson(join(catalogRoot, 'guidance.json'), { schemaVersion: 1, kind: 'backend-guidance-profile',
    id: 'neutral', version: '1.0.0', state: 'qualified', mode: 'neutral',
    material: { accessFacts: true, apiReference: true, designAdvice: false },
    documents: { fake: 'backend.md' }, ...guidance });
  writeJson(join(catalogRoot, 'probes.json'), { schemaVersion: 1, kind: 'capability-probe-profile',
    id: 'defaults', version: '1.0.0', state: 'qualified', firstBuildOnly: true,
    scoreContribution: false, repairVisible: false, probes: [], ...probes });
  writeJson(join(catalogRoot, 'repair.json'), { schemaVersion: 1, kind: 'repair-policy',
    id: 'requested', version: '1.0.0', state: 'qualified', requestedEvidence: true,
    probeEvidence: false, ...repair });
  const ref = { id: 'defaults', version: '1.0.0', guidanceProfile: 'neutral@1.0.0',
    probeProfile: 'defaults@1.0.0', repairPolicy: 'requested@1.0.0' };
  return { root, catalogPath: join(catalogRoot, 'catalog.json'), ref };
}

test('neutral guidance cannot smuggle design advice or omit a selected stack document', () => {
  const advice = customCondition({ guidance: {
    material: { accessFacts: true, apiReference: true, designAdvice: true },
  } });
  try {
    assert.throws(() => resolveStudyConditions([advice.ref], ['fake'], {
      stackBenchRoot: advice.root, catalogPath: advice.catalogPath,
    }), /designAdvice.*false/);
  } finally { rmSync(advice.root, { recursive: true, force: true }); }

  const missing = customCondition();
  try {
    assert.throws(() => resolveStudyConditions([missing.ref], ['other'], {
      stackBenchRoot: missing.root, catalogPath: missing.catalogPath,
    }), /documents.other.*required/);
  } finally { rmSync(missing.root, { recursive: true, force: true }); }
});

test('probe observations can never contribute score or enter repair evidence', () => {
  for (const overrides of [{ probes: { scoreContribution: true } },
    { probes: { repairVisible: true } }, { probes: { probes: ['durability'] } },
    { repair: { probeEvidence: true } }, { repair: { requestedEvidence: false } }]) {
    const fixture = customCondition(overrides);
    try {
      assert.throws(() => resolveStudyConditions([fixture.ref], ['fake'], {
        stackBenchRoot: fixture.root, catalogPath: fixture.catalogPath,
      }), /scoreContribution|repairVisible|probes|probeEvidence|requestedEvidence/);
    } finally { rmSync(fixture.root, { recursive: true, force: true }); }
  }
});
