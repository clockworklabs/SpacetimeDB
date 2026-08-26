import { validateCheckEvidence } from '../evidence/check-evidence.mjs';
import { validateArtifact } from '../evidence/artifacts.mjs';
import { canonicalDefinitionJson } from '../composition/definition-plan.mjs';
import { sha256 } from '../evidence/provenance.mjs';
import { validateProgressionOwner } from './progression-state.mjs';

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const HASH = /^[a-f0-9]{64}$/;

function exactKeys(values, at) {
  if (!Array.isArray(values)) throw new Error(`${at} must be an array`);
  const keys = values.map((value, index) => {
    const key = typeof value === 'string' ? value : value?.stableKey;
    if (typeof key !== 'string' || !key) throw new Error(`${at}[${index}] has no stable key`);
    return key;
  });
  if (new Set(keys).size !== keys.length) throw new Error(`${at} contains duplicate checks`);
  return keys.sort();
}

function sameKeys(left, right) {
  return JSON.stringify([...left].sort()) === JSON.stringify([...right].sort());
}

function inconclusive(attemptId, runId, sourceSha256, selectionSha256, evidence, category, reason) {
  return { attemptId, runId, sourceSha256, selectionSha256,
    ...(evidence ? { evidence } : {}),
    outcome: 'inconclusive', category, reason };
}

export function gradeBundleToProgressionResult(input, action,
  { owner, runArtifact, featureCatalogIdentity, dependencyPolicyIdentity, selectionSha256,
    sourceSha256, recipeIdentity, sequence } = {}) {
  const artifact = validateArtifact(input, { source: '<progression-grade-bundle>' });
  const run = validateArtifact(runArtifact, { source: '<progression-benchmark-run>' });
  owner = validateProgressionOwner(owner, { requireWorkspace: true });
  if (artifact.kind !== 'grade_bundle' || !object(action?.grading)) {
    throw new Error('grade bundle conversion requires an artifact and progression grading action');
  }
  const bundle = artifact.payload;
  if (sequence !== undefined && (!Number.isSafeInteger(sequence) || sequence < 1)) {
    throw new Error('progression grading sequence must be a positive integer');
  }
  const attemptId = sequence === undefined
    ? artifact.attempt.id : `${run.id}-progression-${sequence}`;
  const evidence = sequence === undefined ? null : {
    kind: 'grade_bundle',
    id: artifact.id,
    sha256: sha256(canonicalDefinitionJson(artifact)),
  };
  const campaignOwner = { schemaVersion: owner.schemaVersion,
    campaign: owner.campaign, attempt: owner.attempt };
  if (!['benchmark_run', 'repair_continuation'].includes(run.kind)
    || run.attempt.parentId !== owner.attempt.id
    || artifact.attempt.parentId !== run.id) {
    throw new Error('grade bundle does not belong to the owned progression benchmark run');
  }
  const ownerMismatches = [];
  const mismatch = (changed, field) => { if (changed) ownerMismatches.push(field); };
  mismatch(!object(featureCatalogIdentity) || !HASH.test(featureCatalogIdentity?.sha256 ?? ''),
    'featureCatalogIdentity');
  mismatch(!object(dependencyPolicyIdentity) || !HASH.test(dependencyPolicyIdentity?.sha256 ?? ''),
    'dependencyPolicyIdentity');
  mismatch(run.payload.backend !== owner.attempt.stack, 'run.backend');
  mismatch(run.payload.model !== owner.attempt.model, 'run.model');
  mismatch(run.payload.condition?.sha256 !== owner.attempt.conditionSha256, 'run.condition');
  mismatch(canonicalDefinitionJson(run.payload.progressionOwner)
    !== canonicalDefinitionJson(campaignOwner), 'run.progressionOwner');
  mismatch(canonicalDefinitionJson(run.payload.featureCatalog)
    !== canonicalDefinitionJson(featureCatalogIdentity), 'run.featureCatalog');
  mismatch(canonicalDefinitionJson(run.payload.dependencyPolicy)
    !== canonicalDefinitionJson(dependencyPolicyIdentity), 'run.dependencyPolicy');
  mismatch(run.identities.agentAdapter?.id !== owner.attempt.agentAdapter,
    'run.agentAdapter');
  mismatch(run.identities.stackAdapter?.id !== owner.attempt.stack, 'run.stackAdapter');
  mismatch(artifact.identities.engine?.sha256 !== run.identities.engine?.sha256,
    'grade.engine');
  mismatch(artifact.identities.stackAdapter?.id !== owner.attempt.stack, 'grade.stackAdapter');
  if (ownerMismatches.length) {
    throw new Error(`grade bundle benchmark owner does not match the progression campaign attempt: ${ownerMismatches.join(', ')}`);
  }
  if (!object(recipeIdentity) || typeof recipeIdentity.id !== 'string'
    || typeof recipeIdentity.version !== 'string' || !HASH.test(recipeIdentity.sha256 ?? '')
    || artifact.identities.recipe?.id !== recipeIdentity.id
    || artifact.identities.recipe?.version !== recipeIdentity.version
    || artifact.identities.recipe?.sha256 !== recipeIdentity.sha256) {
    throw new Error('grade bundle recipe identity does not match the progression grading selection');
  }
  if (!HASH.test(sourceSha256 ?? '') || bundle.source?.sha256 !== sourceSha256) {
    throw new Error('grade bundle source identity does not match the graded application source');
  }
  if (bundle.observation !== 'scored') {
    throw new Error('progression grading requires a scored grade bundle');
  }
  const expected = action.grading.checks ?? [];
  const nodeIds = action.grading.nodeIds ?? [];
  if (!Array.isArray(nodeIds) || nodeIds.some(id => typeof id !== 'string' || !id)
    || new Set(nodeIds).size !== nodeIds.length) {
    throw new Error('progression grading node ownership is invalid');
  }
  const selectedNodes = new Set(nodeIds);
  const expectedIds = exactKeys(expected.map(check => check.id), 'progression checks');
  if (expected.some(check => typeof check.nodeId !== 'string' || !check.nodeId
    || !selectedNodes.has(check.nodeId)
    || !Number.isSafeInteger(check.points) || check.points < 1)
    || nodeIds.some(nodeId => !expected.some(item => item.nodeId === nodeId))) {
    throw new Error('progression checks require node ownership and positive points');
  }
  if (!object(bundle.selection)) throw new Error('grade bundle selection is required');
  if (typeof selectionSha256 !== 'string' || !selectionSha256) {
    throw new Error('progression grading selection identity is required');
  }
  if (bundle.selection.sha256 !== selectionSha256) {
    throw new Error('grade bundle selection identity does not match the progression action');
  }
  const selectedIds = exactKeys(bundle.selection.checks, 'grade bundle selected checks');
  if (!sameKeys(selectedIds, expectedIds)) {
    throw new Error('grade bundle selected checks do not match the progression action');
  }
  const selectedById = new Map(bundle.selection.checks.map(check => [check.stableKey, check]));
  for (const check of expected) {
    if (selectedById.get(check.id)?.points !== check.points) {
      throw new Error(`grade bundle points for ${check.id} do not match the progression action`);
    }
  }

  const outcome = bundle.outcome?.kind ?? null;
  if (![null, 'passed', 'app_failure', 'harness_failure', 'inconclusive', 'ungraded']
    .includes(outcome)) {
    throw new Error(`grade bundle outcome ${JSON.stringify(outcome)} is not supported`);
  }
  if (outcome === 'harness_failure') {
    return inconclusive(attemptId, run.id, sourceSha256, selectionSha256, evidence,
      'harness_failure', bundle.outcome.reason
      ?? bundle.error ?? 'grader reported a harness failure');
  }
  if (['inconclusive', 'ungraded'].includes(outcome)) {
    return inconclusive(attemptId, run.id, sourceSha256, selectionSha256, evidence,
      'inconclusive_evidence', bundle.outcome.reason
      ?? bundle.error ?? 'grader evidence was inconclusive');
  }
  if (outcome === 'app_failure') {
    const attempted = exactKeys(bundle.selection.attemptedChecks,
      'grade bundle attempted checks');
    const reported = exactKeys(bundle.selection.reportedChecks,
      'grade bundle reported checks');
    const notRun = exactKeys(bundle.selection.notRun, 'grade bundle not-run checks');
    const accounted = [...attempted, ...notRun];
    const regression = bundle.totals?.regression ?? { score: 0, max: 0 };
    if (typeof bundle.outcome.phase !== 'string' || !bundle.outcome.phase
      || typeof bundle.outcome.reason !== 'string' || !bundle.outcome.reason
      || new Set(accounted).size !== accounted.length || !sameKeys(accounted, expectedIds)
      || reported.some(id => !attempted.includes(id))
      || !Number.isSafeInteger(bundle.totals?.score) || bundle.totals.score !== 0
      || !Number.isSafeInteger(bundle.totals?.max)
      || !Number.isSafeInteger(regression.score) || regression.score !== 0
      || !Number.isSafeInteger(regression.max)
      || bundle.totals.max + regression.max !== expected.reduce((sum, check) => sum + check.points, 0)) {
      throw new Error('grade bundle application abort is incomplete or inconsistent');
    }
    return { attemptId, runId: run.id, sourceSha256, selectionSha256,
      ...(evidence ? { evidence } : {}),
      outcome: 'conclusive', nodes: nodeIds.map(nodeId => ({
      id: nodeId,
      checks: expected.filter(check => check.nodeId === nodeId)
        .map(check => ({ id: check.id, outcome: 'fail' })),
    })) };
  }

  const attemptedIds = exactKeys(bundle.selection.attemptedChecks,
    'grade bundle attempted checks');
  const reportedIds = exactKeys(bundle.selection.reportedChecks,
    'grade bundle reported checks');
  if (!sameKeys(attemptedIds, expectedIds) || !sameKeys(reportedIds, expectedIds)
    || !Array.isArray(bundle.selection.notRun) || bundle.selection.notRun.length !== 0) {
    throw new Error('grade bundle does not contain complete progression evidence');
  }
  const evidenceById = new Map();
  for (const [suiteId, suite] of Object.entries(bundle.suites ?? {})) {
    for (const [featureIndex, feature] of (suite?.features ?? []).entries()) {
      for (const [criterionIndex, criterion] of (feature?.criteria ?? []).entries()) {
        if (criterion?.stableKey === undefined) continue;
        const at = `grade bundle ${suiteId}.features[${featureIndex}].criteria[${criterionIndex}]`;
        if (!expectedIds.includes(criterion.stableKey)) {
          throw new Error(`${at} reports unselected check ${criterion.stableKey}`);
        }
        if (evidenceById.has(criterion.stableKey)) {
          throw new Error(`grade bundle repeats check ${criterion.stableKey}`);
        }
        if (criterion.points !== selectedById.get(criterion.stableKey).points) {
          throw new Error(`grade bundle criterion points for ${criterion.stableKey} do not match selection`);
        }
        validateCheckEvidence(criterion.evidence, { at: `${at}.evidence` });
        evidenceById.set(criterion.stableKey, criterion.evidence);
      }
    }
  }
  const missing = expectedIds.filter(id => !evidenceById.has(id));
  if (missing.length) throw new Error(`grade bundle is missing check evidence: ${missing.join(', ')}`);
  const nonMeasured = expectedIds.map(id => evidenceById.get(id))
    .filter(evidence => !['passed', 'failed'].includes(evidence.status));
  if (nonMeasured.length) {
    const harness = nonMeasured.some(evidence => evidence.status === 'harness_failure');
    return inconclusive(attemptId, run.id, sourceSha256, selectionSha256, evidence,
      harness ? 'harness_failure' : 'inconclusive_evidence',
      'one or more selected checks did not produce measured evidence');
  }
  const passedPoints = expected.reduce((total, check) => total
    + (evidenceById.get(check.id).status === 'passed' ? check.points : 0), 0);
  const availablePoints = expected.reduce((total, check) => total + check.points, 0);
  const regression = bundle.totals?.regression ?? { score: 0, max: 0 };
  if (!Number.isSafeInteger(bundle.totals?.score) || !Number.isSafeInteger(bundle.totals?.max)
    || !Number.isSafeInteger(regression.score) || !Number.isSafeInteger(regression.max)
    || bundle.totals.score + regression.score !== passedPoints
    || bundle.totals.max + regression.max !== availablePoints) {
    throw new Error('grade bundle totals do not match its selected check evidence');
  }
  if (outcome === 'passed' && passedPoints !== availablePoints) {
    throw new Error('grade bundle passed outcome contradicts failed check evidence');
  }
  return { attemptId, runId: run.id, sourceSha256, selectionSha256,
    ...(evidence ? { evidence } : {}),
    outcome: 'conclusive', nodes: nodeIds.map(nodeId => ({
    id: nodeId,
    checks: expected.filter(check => check.nodeId === nodeId).map(check => ({
      id: check.id,
      outcome: evidenceById.get(check.id).status === 'passed' ? 'pass' : 'fail',
    })),
  })) };
}
