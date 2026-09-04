import { validateCheckEvidence } from '../evidence/check-evidence.js';
import type { CheckEvidence } from '../evidence/check-evidence.js';
import { validateArtifact } from '../evidence/artifacts.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { sha256 } from '../evidence/provenance.js';
import { validateProgressionOwner } from './progression-state.js';

interface GradeCheck {
  id: string;
  nodeId?: string;
  points: number;
}

interface ProgressionGradingAction {
  prompt?: { nodeIds?: unknown };
  grading: { nodeIds?: unknown; checks?: unknown };
}

interface SelectedCheck {
  stableKey: string;
  points: number;
}

interface GradeCriterion {
  stableKey?: string;
  points?: number;
  evidence?: unknown;
}

interface GradeBundlePayload {
  observation?: string;
  source?: { sha256?: string };
  selection?: {
    sha256?: string;
    checks?: unknown;
    attemptedChecks?: unknown;
    reportedChecks?: unknown;
    notRun?: unknown;
  };
  outcome?: { kind?: string; reason?: string; phase?: string };
  error?: string;
  totals?: {
    score?: number;
    max?: number;
    regression?: { score?: number; max?: number } | null;
  };
  suites?: Record<string, {
    features?: Array<{ criteria?: GradeCriterion[] }>;
  }>;
}

interface BenchmarkRunPayload {
  backend?: string;
  model?: string;
  condition?: { contentSha256?: string };
  progressionOwner?: unknown;
  featureCatalog?: unknown;
  dependencyPolicy?: unknown;
}

interface GradeConversionOptions {
  owner?: unknown;
  runArtifact?: unknown;
  featureCatalogIdentity?: unknown;
  dependencyPolicyIdentity?: unknown;
  selectionSha256?: string;
  sourceSha256?: string;
  recipeIdentity?: unknown;
  sequence?: number;
}

interface ProgressionCheckResult {
  id: string;
  outcome: 'pass' | 'fail' | 'not-run';
}

interface ProgressionNodeResult {
  id: string;
  checks: ProgressionCheckResult[];
}

interface GradeEvidenceReference {
  kind: 'grade_bundle';
  id: string;
  sha256: string;
}

interface ProgressionResultBase {
  attemptId: string;
  runId: string;
  sourceSha256: string;
  selectionSha256: string;
  evidence?: GradeEvidenceReference;
}

export interface InconclusiveProgressionResult extends ProgressionResultBase {
  outcome: 'inconclusive';
  category: 'harness_failure' | 'inconclusive_evidence';
  reason: string;
}

export interface ConclusiveProgressionResult extends ProgressionResultBase {
  outcome: 'conclusive';
  applicationFailure?: { phase: string; reason: string };
  nodes: ProgressionNodeResult[];
}

export type ProgressionGradeResult =
  | InconclusiveProgressionResult
  | ConclusiveProgressionResult;

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const HASH = /^[a-f0-9]{64}$/;

function exactKeys(values: unknown, at: string): string[] {
  if (!Array.isArray(values)) throw new Error(`${at} must be an array`);
  const keys = values.map((value, index) => {
    const key = typeof value === 'string' ? value : object(value) ? value.stableKey : undefined;
    if (typeof key !== 'string' || !key) throw new Error(`${at}[${index}] has no stable key`);
    return key;
  });
  if (new Set(keys).size !== keys.length) throw new Error(`${at} contains duplicate checks`);
  return keys.sort();
}

function sameKeys(left: string[], right: string[]): boolean {
  return JSON.stringify([...left].sort()) === JSON.stringify([...right].sort());
}

function inconclusive(attemptId: string, runId: string, sourceSha256: string,
  selectionSha256: string, evidence: GradeEvidenceReference | null,
  category: InconclusiveProgressionResult['category'], reason: string): InconclusiveProgressionResult {
  return { attemptId, runId, sourceSha256, selectionSha256,
    ...(evidence ? { evidence } : {}),
    outcome: 'inconclusive', category, reason };
}

export function gradeBundleToProgressionResult(input: unknown, action: unknown,
  { owner, runArtifact, featureCatalogIdentity, dependencyPolicyIdentity, selectionSha256,
    sourceSha256, recipeIdentity, sequence }: GradeConversionOptions = {}): ProgressionGradeResult {
  const artifact = validateArtifact<GradeBundlePayload>(input,
    { source: '<progression-grade-bundle>' });
  const run = validateArtifact<BenchmarkRunPayload>(runArtifact,
    { source: '<progression-benchmark-run>' });
  const validatedOwner = validateProgressionOwner(owner, { requireWorkspace: true });
  if (artifact.kind !== 'grade_bundle' || !object(action) || !object(action.grading)) {
    throw new Error('grade bundle conversion requires an artifact and progression grading action');
  }
  const gradingAction: ProgressionGradingAction = {
    grading: action.grading,
    ...(object(action.prompt) ? { prompt: action.prompt } : {}),
  };
  const bundle = artifact.payload;
  const runAgentAdapter = run.identities.agentAdapter;
  const runStackAdapter = run.identities.stackAdapter;
  const runEngine = run.identities.engine;
  const gradeStackAdapter = artifact.identities.stackAdapter;
  const gradeEngine = artifact.identities.engine;
  const gradeRecipe = artifact.identities.recipe;
  if (sequence !== undefined && (!Number.isSafeInteger(sequence) || sequence < 1)) {
    throw new Error('progression grading sequence must be a positive integer');
  }
  if (typeof artifact.attempt.id !== 'string' || !artifact.attempt.id) {
    throw new Error('grade bundle attempt identity is required');
  }
  const attemptId = sequence === undefined
    ? artifact.attempt.id : `${run.id}-progression-${sequence}`;
  const evidence: GradeEvidenceReference | null = sequence === undefined ? null : {
    kind: 'grade_bundle',
    id: artifact.id,
    sha256: sha256(canonicalDefinitionJson(artifact)),
  };
  const campaignOwner = { schemaVersion: validatedOwner.schemaVersion,
    campaign: validatedOwner.campaign, attempt: validatedOwner.attempt };
  if (!['benchmark_run', 'repair_continuation'].includes(run.kind)
    || run.attempt.parentId !== validatedOwner.attempt.id
    || artifact.attempt.parentId !== run.id) {
    throw new Error('grade bundle does not belong to the owned progression benchmark run');
  }
  const ownerMismatches: string[] = [];
  const mismatch = (changed: boolean, field: string): void => {
    if (changed) ownerMismatches.push(field);
  };
  mismatch(!object(featureCatalogIdentity)
    || typeof featureCatalogIdentity.contentSha256 !== 'string'
    || !HASH.test(featureCatalogIdentity.contentSha256),
    'featureCatalogIdentity');
  mismatch(!object(dependencyPolicyIdentity)
    || typeof dependencyPolicyIdentity.contentSha256 !== 'string'
    || !HASH.test(dependencyPolicyIdentity.contentSha256),
    'dependencyPolicyIdentity');
  mismatch(run.payload.backend !== validatedOwner.attempt.stack, 'run.backend');
  mismatch(run.payload.model !== validatedOwner.attempt.model, 'run.model');
  mismatch(run.payload.condition?.contentSha256 !== validatedOwner.attempt.conditionSha256, 'run.condition');
  mismatch(canonicalDefinitionJson(run.payload.progressionOwner)
    !== canonicalDefinitionJson(campaignOwner), 'run.progressionOwner');
  mismatch(canonicalDefinitionJson(run.payload.featureCatalog)
    !== canonicalDefinitionJson(featureCatalogIdentity), 'run.featureCatalog');
  mismatch(canonicalDefinitionJson(run.payload.dependencyPolicy)
    !== canonicalDefinitionJson(dependencyPolicyIdentity), 'run.dependencyPolicy');
  mismatch(runAgentAdapter?.id !== validatedOwner.attempt.agentAdapter,
    'run.agentAdapter');
  mismatch(runStackAdapter?.id !== validatedOwner.attempt.stack, 'run.stackAdapter');
  mismatch(gradeEngine?.sha256 !== runEngine?.sha256,
    'grade.engine');
  mismatch(gradeStackAdapter?.id !== validatedOwner.attempt.stack,
    'grade.stackAdapter');
  if (ownerMismatches.length) {
    throw new Error(`grade bundle benchmark owner does not match the progression campaign attempt: ${ownerMismatches.join(', ')}`);
  }
  if (!object(recipeIdentity) || typeof recipeIdentity.id !== 'string'
    || typeof recipeIdentity.sha256 !== 'string' || !HASH.test(recipeIdentity.sha256)
    || gradeRecipe?.id !== recipeIdentity.id
    || gradeRecipe.sha256 !== recipeIdentity.sha256) {
    throw new Error('grade bundle recipe identity does not match the progression grading selection');
  }
  if (typeof sourceSha256 !== 'string' || !HASH.test(sourceSha256)
    || bundle.source?.sha256 !== sourceSha256) {
    throw new Error('grade bundle source identity does not match the graded application source');
  }
  if (bundle.observation !== 'scored') {
    throw new Error('progression grading requires a scored grade bundle');
  }
  const expectedValue = gradingAction.grading.checks ?? [];
  if (!Array.isArray(expectedValue)) throw new Error('progression checks must be an array');
  const expected = expectedValue as GradeCheck[];
  const nodeIds = gradingAction.grading.nodeIds ?? [];
  if (!Array.isArray(nodeIds) || nodeIds.some(id => typeof id !== 'string' || !id)
    || new Set(nodeIds).size !== nodeIds.length) {
    throw new Error('progression grading node ownership is invalid');
  }
  const validatedNodeIds = nodeIds as string[];
  const selectedNodes = new Set(validatedNodeIds);
  const expectedIds = exactKeys(expected.map(check => check.id), 'progression checks');
  const missingNodeOwner = validatedNodeIds.some(nodeId =>
    !expected.some(check => check.nodeId === nodeId));
  if (expected.some(check => typeof check.nodeId !== 'string' || !check.nodeId
    || !selectedNodes.has(check.nodeId)
    || !Number.isSafeInteger(check.points) || check.points < 1)
    || missingNodeOwner) {
    throw new Error('progression checks require node ownership and positive points');
  }
  if (!object(bundle.selection)) throw new Error('grade bundle selection is required');
  const selection = bundle.selection;
  if (typeof selectionSha256 !== 'string' || !selectionSha256) {
    throw new Error('progression grading selection identity is required');
  }
  if (selection.sha256 !== selectionSha256) {
    throw new Error('grade bundle selection identity does not match the progression action');
  }
  const selectedIds = exactKeys(selection.checks, 'grade bundle selected checks');
  if (!sameKeys(selectedIds, expectedIds)) {
    throw new Error('grade bundle selected checks do not match the progression action');
  }
  const selectedChecks = selection.checks as SelectedCheck[];
  const selectedById = new Map(selectedChecks.map(check => [check.stableKey, check]));
  for (const check of expected) {
    if (selectedById.get(check.id)?.points !== check.points) {
      throw new Error(`grade bundle points for ${check.id} do not match the progression action`);
    }
  }

  const bundleOutcome = bundle.outcome;
  const outcome = bundleOutcome?.kind ?? null;
  if (![null, 'passed', 'app_failure', 'harness_failure', 'inconclusive', 'ungraded']
    .includes(outcome)) {
    throw new Error(`grade bundle outcome ${JSON.stringify(outcome)} is not supported`);
  }
  if (outcome === 'harness_failure') {
    return inconclusive(attemptId, run.id, sourceSha256, selectionSha256, evidence,
      'harness_failure', bundleOutcome?.reason
      ?? bundle.error ?? 'grader reported a harness failure');
  }
  if (outcome === 'inconclusive' || outcome === 'ungraded') {
    return inconclusive(attemptId, run.id, sourceSha256, selectionSha256, evidence,
      'inconclusive_evidence', bundleOutcome?.reason
      ?? bundle.error ?? 'grader evidence was inconclusive');
  }
  if (bundleOutcome?.kind === 'app_failure'
    && Array.isArray(selection.notRun) && selection.notRun.length > 0) {
    const promptNodeIds = gradingAction.prompt?.nodeIds ?? [];
    if (!Array.isArray(promptNodeIds) || promptNodeIds.length === 0
      || promptNodeIds.some(id => typeof id !== 'string' || !selectedNodes.has(id))
      || new Set(promptNodeIds).size !== promptNodeIds.length) {
      throw new Error('progression application abort requires exact current prompt nodes');
    }
    const currentNodes = new Set(promptNodeIds);
    const attempted = exactKeys(selection.attemptedChecks,
      'grade bundle attempted checks');
    const reported = exactKeys(selection.reportedChecks,
      'grade bundle reported checks');
    const notRun = exactKeys(selection.notRun, 'grade bundle not-run checks');
    const accounted = [...attempted, ...notRun];
    const score = bundle.totals?.score;
    const max = bundle.totals?.max;
    const regression = bundle.totals?.regression ?? { score: 0, max: 0 };
    const regressionScore = regression.score;
    const regressionMax = regression.max;
    if (typeof bundleOutcome.phase !== 'string' || !bundleOutcome.phase
      || typeof bundleOutcome.reason !== 'string' || !bundleOutcome.reason
      || new Set(accounted).size !== accounted.length || !sameKeys(accounted, expectedIds)
      || reported.some(id => !attempted.includes(id))
      || typeof score !== 'number' || !Number.isSafeInteger(score) || score !== 0
      || typeof max !== 'number' || !Number.isSafeInteger(max)
      || typeof regressionScore !== 'number' || !Number.isSafeInteger(regressionScore)
      || regressionScore !== 0
      || typeof regressionMax !== 'number' || !Number.isSafeInteger(regressionMax)
      || max + regressionMax !== expected.reduce((sum, check) => sum + check.points, 0)) {
      throw new Error('grade bundle application abort is incomplete or inconsistent');
    }
    return { attemptId, runId: run.id, sourceSha256, selectionSha256,
      ...(evidence ? { evidence } : {}),
      outcome: 'conclusive',
      applicationFailure: { phase: bundleOutcome.phase, reason: bundleOutcome.reason },
      nodes: validatedNodeIds.map(nodeId => ({
      id: nodeId,
      checks: expected.filter(check => check.nodeId === nodeId)
        .map(check => ({ id: check.id,
          outcome: currentNodes.has(nodeId) ? 'fail' : 'not-run' })),
    })) };
  }

  const attemptedIds = exactKeys(selection.attemptedChecks,
    'grade bundle attempted checks');
  const reportedIds = exactKeys(selection.reportedChecks,
    'grade bundle reported checks');
  if (!sameKeys(attemptedIds, expectedIds) || !sameKeys(reportedIds, expectedIds)
    || !Array.isArray(selection.notRun) || selection.notRun.length !== 0) {
    throw new Error('grade bundle does not contain complete progression evidence');
  }
  const evidenceById = new Map<string, CheckEvidence>();
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
        if (criterion.points !== selectedById.get(criterion.stableKey)?.points) {
          throw new Error(`grade bundle criterion points for ${criterion.stableKey} are ${JSON.stringify(criterion.points)}, expected ${JSON.stringify(
            selectedById.get(criterion.stableKey)?.points)}`);
        }
        const checkedEvidence = validateCheckEvidence(criterion.evidence,
          { at: `${at}.evidence` });
        evidenceById.set(criterion.stableKey, checkedEvidence);
      }
    }
  }
  const missing = expectedIds.filter(id => !evidenceById.has(id));
  if (missing.length) throw new Error(`grade bundle is missing check evidence: ${missing.join(', ')}`);
  const checkEvidence = (id: string): CheckEvidence => {
    const found = evidenceById.get(id);
    if (!found) throw new Error(`grade bundle is missing check evidence: ${id}`);
    return found;
  };
  const nonMeasured = expectedIds.map(id => checkEvidence(id))
    .filter(evidence => !['passed', 'failed'].includes(evidence.status));
  if (nonMeasured.length > 0) {
    const harness = nonMeasured.some(evidence => evidence.status === 'harness_failure');
    return inconclusive(attemptId, run.id, sourceSha256, selectionSha256, evidence,
      harness ? 'harness_failure' : 'inconclusive_evidence',
      `${nonMeasured.length} selected ${nonMeasured.length === 1 ? 'check did' : 'checks did'} not produce measured evidence`);
  }
  const passedPoints = expected.reduce((total, check) => total
    + (checkEvidence(check.id).status === 'passed' ? check.points : 0), 0);
  const availablePoints = expected.reduce((total, check) => total + check.points, 0);
  const score = bundle.totals?.score;
  const max = bundle.totals?.max;
  const regression = bundle.totals?.regression ?? { score: 0, max: 0 };
  const regressionScore = regression.score;
  const regressionMax = regression.max;
  if (typeof score !== 'number' || !Number.isSafeInteger(score)
    || typeof max !== 'number' || !Number.isSafeInteger(max)
    || typeof regressionScore !== 'number' || !Number.isSafeInteger(regressionScore)
    || typeof regressionMax !== 'number' || !Number.isSafeInteger(regressionMax)
    || score + regressionScore !== passedPoints
    || max + regressionMax !== availablePoints) {
    throw new Error('grade bundle totals do not match its selected check evidence');
  }
  if (outcome === 'passed' && passedPoints !== availablePoints) {
    throw new Error('grade bundle passed outcome contradicts failed check evidence');
  }
  return { attemptId, runId: run.id, sourceSha256, selectionSha256,
    ...(evidence ? { evidence } : {}),
    outcome: 'conclusive', nodes: validatedNodeIds.map(nodeId => ({
    id: nodeId,
    checks: expected.filter(check => check.nodeId === nodeId).map(check => ({
      id: check.id,
      outcome: checkEvidence(check.id).status === 'passed' ? 'pass'
        : checkEvidence(check.id).status === 'failed' ? 'fail' : 'not-run',
    })),
  })) };
}
