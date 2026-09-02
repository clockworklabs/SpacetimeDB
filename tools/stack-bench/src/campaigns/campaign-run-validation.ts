import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { ARTIFACT_FILE, readArtifactPayload } from '../evidence/artifacts.js';
import { durableCostLedger } from '../evidence/cost-proof.js';
import { aggregateRunOutcome, classifyBundle } from '../evidence/outcomes.js';
import type { OutcomeBundle, RunOutcome as EvidenceRunOutcome } from '../evidence/outcomes.js';
import type { BenchmarkRunRecord, GradeBundleSelection, RunLevelRecord }
  from '../evidence/benchmark-run.js';
import { hashDirectory, sha256 } from '../evidence/provenance.js';
import { liveProgressionStatus } from '../progression/live-progression.js';
import { DEPENDENCY_MODE_POLICY } from '../progression/dependency-definition.js';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../progression/progression-definition.js';
import { readProgressionState } from '../progression/progression-state.js';
import { campaignProgressionOwner } from './campaign-compiler.js';
import type { CompiledCampaignPlan } from './campaign-compiler.js';
import type { CampaignExtensionSeed } from './campaign-scheduler.js';

type UnknownRecord = Record<string, unknown>;
type PlannedLevel = CompiledCampaignPlan['conditions'][number]['requested']['levels'][number];
type PlannedCheck = NonNullable<PlannedLevel['selection']['scoredChecks']>[number];
const integer = (value: unknown): value is number =>
  typeof value === 'number' && Number.isInteger(value);
const safeInteger = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value);
const finite = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value);
const record = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

export interface RunOutcome extends Partial<EvidenceRunOutcome>, UnknownRecord {
  provider?: { providerStatus?: number | null };
}

interface RunRegression { score?: number; max?: number }
interface RunSelection extends Omit<Partial<GradeBundleSelection>,
'scoredChecks' | 'observedChecks'>, UnknownRecord {
  specifications?: unknown;
  scoredChecks?: PlannedCheck[];
  observedChecks?: PlannedCheck[];
  regressionChecks?: Array<{ stableKey?: string; points?: number }>;
  scoredPoints?: number;
  evaluationSha256?: string;
  regressionPoints?: number;
}

interface RunObservation extends UnknownRecord {
  selectedChecks?: unknown;
  selectionSha256?: unknown;
  scoreContribution?: unknown;
  repairVisible?: unknown;
  sourceSha256?: unknown;
  reportedChecks?: unknown;
  observedPoints?: unknown;
  passedPoints?: unknown;
  artifact?: unknown;
}

interface RunLevel extends Omit<Partial<RunLevelRecord>,
'selection' | 'outcome' | 'firstBuild' | 'regression' | 'repair'>, UnknownRecord {
  level: number;
  score?: number;
  max?: number;
  graded?: boolean;
  fixRounds?: number;
  contractPass?: boolean;
  selection?: RunSelection;
  outcome?: RunOutcome;
  repair?: { status?: string; budgetRounds?: number; roundsUsed?: number; stopReason?: string };
  regression?: RunRegression | null;
  firstBuild?: {
    score?: number;
    max?: number;
    outcome?: RunOutcome;
    regression?: RunRegression | null;
    observations?: RunObservation;
    source?: { sha256?: string };
  };
}

export interface BenchmarkRun extends Partial<Pick<BenchmarkRunRecord,
'parentAttemptId' | 'mode' | 'track' | 'backend' | 'model' | 'pricing' | 'guidance'
| 'condition' | 'selectionRequest' | 'featureCatalog' | 'dependencyPolicy'
>>, UnknownRecord {
  id?: string | null;
  artifactEnvelope?: {
    attempt?: { parentId?: string };
    identities?: {
      agentAdapter?: { sha256?: string };
      engine?: { sha256?: string };
      experiment?: { id?: string; version?: string; sha256?: string; state?: string };
      stackAdapter?: { id?: string; version?: string };
    };
  };
  mode?: unknown;
  track?: string;
  backend?: string;
  model?: string;
  pricing?: unknown;
  guidance?: string;
  condition?: unknown;
  selectionRequest?: unknown;
  featureCatalog?: unknown;
  dependencyPolicy?: unknown;
  progressionOwner?: unknown;
  progressionStatus?: unknown;
  progressionSeed?: unknown;
  skills?: unknown;
  levels?: RunLevel[];
  outcome?: RunOutcome;
  validation?: { ladder?: {
    policy?: string;
    requestedLevels?: unknown;
    completedLevels?: unknown;
    stoppedAfterLevel?: unknown;
    blockedLevels?: unknown;
  } };
  runtime?: { buildImage?: string | null };
  totals?: { score?: number; max?: number; costUsd?: number | null; costComplete?: boolean };
  backendLease?: { runId?: string; backend?: string; state?: string };
  contaminated?: boolean;
  contamination?: { verdict?: string };
}

interface GradeBundle extends UnknownRecord {
  source?: { sha256?: string };
  selection?: { sha256?: string };
  totals?: { score?: number; max?: number };
}

interface ValidationCondition {
  sha256: string;
  requested?: CompiledCampaignPlan['conditions'][number]['requested'];
}

interface CampaignValidationAttempt {
  id: string;
  stack: string;
  model: string;
  guidance: string;
  levels: number[];
  agentAdapter: string;
  skills: unknown;
  pricing?: unknown;
  mode: { id?: string; [key: string]: unknown };
  condition: ValidationCondition;
  featureCatalog?: CompiledCampaignPlan['attempts'][number]['featureCatalog'];
  dependencyPolicy?: CompiledCampaignPlan['attempts'][number]['dependencyPolicy'];
}

interface CampaignValidationPlan {
  id: string;
  version: string;
  state: string;
  contentSha256: string;
  definition: {
    track: string;
    selection: unknown;
    runtime: { buildImage: string | null };
    budgets: { fixRounds: number; maxCostUsdPerAttempt: number | null };
  };
  identities: { engine: { sha256: string | null } };
  agents: Array<{ adapter: string; model: string; costLimit: string;
    identity: { sha256: string | null } }>;
  stacks: Array<{ id: string; version: string | null }>;
  conditions: ValidationCondition[];
  featureCatalog: CompiledCampaignPlan['featureCatalog'];
  dependencyPolicy: CompiledCampaignPlan['dependencyPolicy'];
}

export function expectedDependencyRunOutcomeKind(
  levels: Array<{ level?: number; outcome?: { kind?: string } }>,
  terminalOutcome: { kind?: string } | null | undefined): string | null {
  const expected = aggregateRunOutcome(levels.map((level, index) => ({
    level: level.level ?? index + 1,
    ...(typeof level.outcome?.kind === 'string' ? { outcome: { kind: level.outcome.kind } } : {}),
  }))).kind;
  if (terminalOutcome?.kind !== 'passed' && expected === 'passed') return null;
  return expected;
}

type Mismatch = (condition: unknown, field: string) => void;

function validatePackageEvidence(run: BenchmarkRun, resultDir: string | null,
  finalGradedLevel: RunLevel | null, mismatch: Mismatch): void {
  if (resultDir === null || !finalGradedLevel
    || !['passed', 'app_failure'].includes(run.outcome?.kind ?? '')) return;
  if (!resultDir) {
    mismatch(true, 'packageEvidence');
    return;
  }
  let source = null;
  let grading = null;
  try {
    const sourcePath = join(resolve(resultDir), 'source');
    if (!existsSync(sourcePath)) throw new Error('missing source');
    source = hashDirectory(sourcePath);
  } catch {
    mismatch(true, 'packageEvidence.source');
  }
  try {
    const bundlePath = join(resolve(resultDir), 'grading', ARTIFACT_FILE.gradeBundle);
    if (!existsSync(bundlePath)) throw new Error('missing grading bundle');
    grading = readArtifactPayload(bundlePath, { expectedKind: 'grade_bundle' }) as GradeBundle;
  } catch {
    mismatch(true, 'packageEvidence.grading');
  }
  if (!source || !grading) return;
  mismatch(grading.source?.sha256 !== source.sha256,
    'packageEvidence.grading.sourceSha256');
  mismatch(grading.selection?.sha256 !== finalGradedLevel.selection?.sha256,
    'packageEvidence.grading.selectionSha256');
  mismatch(grading.totals?.score !== finalGradedLevel.score
    || grading.totals?.max !== finalGradedLevel.max, 'packageEvidence.grading.score');
  const gradingOutcome = classifyBundle(grading as OutcomeBundle);
  mismatch(gradingOutcome.kind !== finalGradedLevel.outcome?.kind
    || canonicalDefinitionJson(gradingOutcome.appFailures)
      !== canonicalDefinitionJson(finalGradedLevel.outcome?.appFailures ?? [])
    || canonicalDefinitionJson(gradingOutcome.inconclusive)
      !== canonicalDefinitionJson(finalGradedLevel.outcome?.inconclusive ?? [])
    || canonicalDefinitionJson(gradingOutcome.harnessFailures)
      !== canonicalDefinitionJson(finalGradedLevel.outcome?.harnessFailures ?? []),
  'packageEvidence.grading.outcome');
}

function validateDependencyEvidence(plan: CampaignValidationPlan,
  attempt: CampaignValidationAttempt, run: BenchmarkRun, resultDir: string | null,
  expectedLevels: number[], interruptedPrefix: boolean, mismatch: Mismatch): void {
  if (typeof resultDir !== 'string' || !resultDir) {
    mismatch(true, 'progressionState');
    return;
  }
  try {
    const owner = campaignProgressionOwner(plan, attempt, { workspace: true });
    const featureCatalog = plan.featureCatalog!;
    const dependencyPolicy = plan.dependencyPolicy!;
    const progression = compileProgressionInput(dependencyRuntimeDefinition(
      featureCatalog, dependencyPolicy));
    const stored = readProgressionState(join(resolve(resultDir), ARTIFACT_FILE.progressionState), {
      progression,
      featureCatalogIdentity: featureCatalog.identity,
      dependencyPolicyIdentity: dependencyPolicy.identity,
      owner,
    });
    const storedStatus = liveProgressionStatus(stored.state);
    mismatch(canonicalDefinitionJson(run.progressionStatus)
      !== canonicalDefinitionJson(storedStatus), 'progressionStatus');
    const ladder = run.validation?.ladder;
    mismatch(canonicalDefinitionJson(ladder?.requestedLevels)
      !== canonicalDefinitionJson(expectedLevels), 'validation.ladder.requestedLevels');
    const conclusiveLevels = [...new Set(stored.state.attempts
      .filter(item => item.outcome === 'conclusive').map(item => item.level))];
    mismatch(canonicalDefinitionJson(ladder?.completedLevels)
      !== canonicalDefinitionJson(conclusiveLevels), 'validation.ladder.completedLevels');
    for (const level of run.levels ?? []) {
      const matching = [...stored.state.attempts].reverse().find(item =>
        item.level === level.level && item.selectionSha256 === level.selection?.sha256);
      mismatch(!matching, `levels.L${level.level}.progressionAttempt`);
      if (!matching) continue;
      const validScore = safeInteger(level.score) && safeInteger(level.max)
        && level.max > 0 && level.score >= 0 && level.score <= level.max;
      mismatch(matching.outcome === 'conclusive' && !validScore, `levels.L${level.level}.score`);
      mismatch(matching.outcome === 'inconclusive' && level.graded !== false,
        `levels.L${level.level}.graded`);
      const codingInterruption = matching.outcome === 'inconclusive'
        && level.outcome?.phase === 'coding-session';
      mismatch(codingInterruption && matching.category !== level.outcome?.kind,
        `levels.L${level.level}.progressionAttempt.category`);
      mismatch(codingInterruption && matching.reason !== level.outcome?.reason,
        `levels.L${level.level}.progressionAttempt.reason`);
    }
    if (stored.state.phase === 'terminal') {
      const points = storedStatus.score.uniqueChecks;
      mismatch(run.totals?.score !== points.passedPoints
        || run.totals?.max !== points.availablePoints, 'totals.score');
      const expectedOutcome = expectedDependencyRunOutcomeKind(
        run.levels ?? [], stored.state.terminalOutcome);
      mismatch(expectedOutcome === null || run.outcome?.kind !== expectedOutcome, 'outcome.kind');
    } else if (!interruptedPrefix) {
      const seed = record(run.progressionSeed) ? run.progressionSeed : null;
      const stoppedLevel = run.levels?.at(-1);
      const stoppedDuringRecheck = seed !== null && safeInteger(seed.fromDepth)
        && Array.isArray(seed.validatedDepths)
        && seed.validatedDepths.length < seed.fromDepth
        && stoppedLevel !== undefined && stoppedLevel.level <= seed.fromDepth
        && stoppedLevel.outcome?.kind === 'app_failure';
      mismatch(!stoppedDuringRecheck
        && stored.state.attempts.at(-1)?.outcome !== 'inconclusive', 'progressionState.phase');
    }
  } catch {
    mismatch(true, 'progressionState');
  }
}

export function validateCampaignRun(plan: CampaignValidationPlan, attempt: CampaignValidationAttempt,
  input: unknown, {
  buildImage = null, resultDir = null, progressionSeed = null,
}: { buildImage?: string | null; resultDir?: string | null;
  progressionSeed?: CampaignExtensionSeed | null } = {}): BenchmarkRun {
  if (!record(input)) throw new Error('campaign run must be an object');
  const run = input as BenchmarkRun;
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter
    && item.model === attempt.model);
  const condition = plan.conditions.find(item => item.sha256 === attempt.condition?.sha256);
  const expectedLevels = [...attempt.levels].sort((a, b) => a - b);
  const actualLevels = (run.levels ?? []).map(level => level.level).sort((a, b) => a - b);
  const dependencyMode = attempt.mode?.id === 'dependency';
  const exactLevels = canonicalDefinitionJson(actualLevels) === canonicalDefinitionJson(expectedLevels);
  const allAtOnceLevels = dependencyMode && attempt.mode?.workSelection === 'all-at-once'
    && actualLevels.length === 1 && actualLevels[0] === expectedLevels.at(-1);
  const ladder = run.validation?.ladder;
  const lastActualLevel = actualLevels.at(-1) ?? null;
  const blockedLevels = expectedLevels.slice(actualLevels.length);
  const gatedPrefix = actualLevels.length > 0 && actualLevels.length < expectedLevels.length
    && actualLevels.every((level, index) => level === expectedLevels[index])
    && run.outcome?.kind === 'app_failure'
    && ladder?.policy === 'pass-before-next-level'
    && canonicalDefinitionJson(ladder.requestedLevels) === canonicalDefinitionJson(expectedLevels)
    && canonicalDefinitionJson(ladder.completedLevels) === canonicalDefinitionJson(actualLevels)
    && ladder.stoppedAfterLevel === lastActualLevel
    && canonicalDefinitionJson(ladder.blockedLevels) === canonicalDefinitionJson(blockedLevels)
    && run.levels?.at(-1)?.outcome?.kind === 'app_failure'
    && run.levels.at(-1)?.repair?.status === 'budget-exhausted';
  const interruptedPrefix = actualLevels.length < expectedLevels.length
    && actualLevels.every((level, index) => level === expectedLevels[index])
    && ['provider_failure', 'harness_failure', 'ungraded'].includes(run.outcome?.kind ?? '');
  const dependencyPrefix = dependencyMode && actualLevels.length > 0
    && actualLevels.length <= expectedLevels.length
    && actualLevels.every((level, index) => level === expectedLevels[index])
    && run.validation?.ladder?.policy === DEPENDENCY_MODE_POLICY;
  const mismatches: string[] = [];
  const mismatch = (condition: unknown, field: string): void => {
    if (condition) mismatches.push(field);
  };
  const progressionStatus = run.progressionStatus && typeof run.progressionStatus === 'object'
    && !Array.isArray(run.progressionStatus) ? run.progressionStatus as UnknownRecord : null;
  const progressionLevel = dependencyMode && integer(progressionStatus?.level)
    ? progressionStatus.level : null;
  const gradedLevels = (run.levels ?? []).filter(level => safeInteger(level.score)
    && safeInteger(level.max) && level.max > 0);
  const finalGradedLevel = progressionLevel === null
    ? gradedLevels.at(-1) ?? null
    : gradedLevels.find(level => level.level === progressionLevel) ?? null;
  validatePackageEvidence(run, resultDir, finalGradedLevel, mismatch);
  mismatch(run.artifactEnvelope?.attempt?.parentId !== attempt.id, 'attempt.parentId');
  mismatch(canonicalDefinitionJson(run.mode ?? null)
    !== canonicalDefinitionJson(attempt.mode), 'mode');
  mismatch(run.track !== plan.definition.track, 'track');
  mismatch(run.backend !== attempt.stack, 'backend');
  mismatch(run.model !== attempt.model, 'model');
  mismatch(canonicalDefinitionJson(run.pricing ?? null)
    !== canonicalDefinitionJson(attempt.pricing ?? null), 'pricing');
  mismatch(run.guidance !== attempt.guidance, 'guidance');
  mismatch(!condition || canonicalDefinitionJson(run.condition)
    !== canonicalDefinitionJson(attempt.condition), 'condition');
  mismatch(canonicalDefinitionJson(run.selectionRequest)
    !== canonicalDefinitionJson(plan.definition.selection), 'selectionRequest');
  mismatch(canonicalDefinitionJson(run.featureCatalog ?? null)
    !== canonicalDefinitionJson(attempt.featureCatalog ?? null), 'featureCatalog');
  mismatch(canonicalDefinitionJson(run.dependencyPolicy ?? null)
    !== canonicalDefinitionJson(attempt.dependencyPolicy ?? null), 'dependencyPolicy');
  const expectedProgressionOwner = attempt.dependencyPolicy
    ? campaignProgressionOwner(plan, attempt) : null;
  mismatch(canonicalDefinitionJson(run.progressionOwner ?? null)
    !== canonicalDefinitionJson(expectedProgressionOwner), 'progressionOwner');
  mismatch(canonicalDefinitionJson(run.skills) !== canonicalDefinitionJson(attempt.skills), 'skills');
  const actualSeed = record(run.progressionSeed) ? run.progressionSeed : null;
  const expectedSeed = progressionSeed === null ? null
    : Object.fromEntries(Object.entries(progressionSeed).filter(([field]) => field !== 'source'));
  const validatedDepths = actualSeed?.validatedDepths;
  mismatch(canonicalDefinitionJson(actualSeed === null ? null
    : Object.fromEntries(Object.entries(actualSeed).filter(([field]) => field !== 'validatedDepths')))
    !== canonicalDefinitionJson(expectedSeed), 'progressionSeed');
  if (progressionSeed !== null) {
    const passedRechecks = (run.levels ?? []).filter(level => level.level <= progressionSeed.fromDepth
      && level.graded === true && level.outcome?.kind === 'passed').map(level => level.level);
    mismatch(canonicalDefinitionJson(validatedDepths)
      !== canonicalDefinitionJson(passedRechecks), 'progressionSeed.validatedDepths');
  }
  mismatch(!exactLevels && !allAtOnceLevels && !interruptedPrefix && !gatedPrefix
    && !dependencyPrefix, 'levels');
  mismatch(run.artifactEnvelope?.identities?.agentAdapter?.sha256 !== agent?.identity.sha256,
    'identities.agentAdapter.sha256');
  mismatch(run.artifactEnvelope?.identities?.engine?.sha256 !== plan.identities.engine.sha256,
    'identities.engine.sha256');
  mismatch(run.artifactEnvelope?.identities?.experiment?.id !== plan.id
    || run.artifactEnvelope?.identities?.experiment?.version !== plan.version
    || run.artifactEnvelope?.identities?.experiment?.sha256 !== plan.contentSha256
    || run.artifactEnvelope?.identities?.experiment?.state !== plan.state,
  'identities.experiment');
  mismatch(run.artifactEnvelope?.identities?.stackAdapter?.id !== attempt.stack,
    'identities.stackAdapter.id');
  mismatch(run.artifactEnvelope?.identities?.stackAdapter?.version
    !== plan.stacks.find(item => item.id === attempt.stack)?.version,
    'identities.stackAdapter.version');
  mismatch(plan.definition.runtime.buildImage !== null
    && run.runtime?.buildImage !== plan.definition.runtime.buildImage, 'runtime.buildImage');
  mismatch(plan.definition.runtime.buildImage === null && buildImage !== null
    && run.runtime?.buildImage !== buildImage, 'runtime.buildImage');
  for (const level of run.levels ?? []) {
    if (dependencyMode) continue;
    const plannedLevel = condition?.requested?.levels?.find(item => item.level === level.level);
    if (plannedLevel?.selection?.schemaVersion === 3) {
      const selectionAt = `levels.L${level.level}.selection`;
      const projectChecks = (checks: PlannedCheck[] | undefined) => checks?.map(check => ({
        stableKey: check?.stableKey, points: check?.points, treatment: check?.treatment,
      }));
      // Missing selection fields are identity mismatches, not validator crashes.
      const canonicalOrMissing = (value: unknown) => value === undefined
        ? '"<missing>"' : canonicalDefinitionJson(value);
      mismatch(level.selection?.schemaVersion !== 3, `${selectionAt}.schemaVersion`);
      mismatch(level.selection?.sha256 !== plannedLevel.selection.sha256,
        `${selectionAt}.sha256`);
      mismatch(canonicalOrMissing(level.selection?.specifications)
        !== canonicalDefinitionJson(plannedLevel.selection.specifications),
      `${selectionAt}.specifications`);
      mismatch(canonicalOrMissing(projectChecks(level.selection?.scoredChecks))
        !== canonicalDefinitionJson(plannedLevel.selection.scoredChecks),
      `${selectionAt}.scoredChecks`);
      mismatch(canonicalOrMissing(projectChecks(level.selection?.observedChecks))
        !== canonicalDefinitionJson(plannedLevel.selection.observedChecks),
      `${selectionAt}.observedChecks`);
      mismatch(level.selection?.scoredPoints !== plannedLevel.selection.scoredPoints,
        `${selectionAt}.scoredPoints`);
      const priorChecks = (condition?.requested?.levels ?? [])
        .filter(item => item.level < level.level)
        .flatMap(item => item.selection?.scoredChecks ?? []);
      const expectedRegression = priorChecks.map(check => ({ stableKey: check.stableKey,
        points: check.points }));
      const actualRegression = (level.selection?.regressionChecks ?? []).map(check => ({
        stableKey: check?.stableKey, points: check?.points,
      }));
      mismatch(canonicalDefinitionJson(actualRegression)
        !== canonicalDefinitionJson(expectedRegression), `${selectionAt}.regressionChecks`);
      const regressionPoints = expectedRegression.reduce((total, check) => total + check.points, 0);
      if (regressionPoints > 0) {
        const expectedEvaluationSha256 = sha256(Buffer.from(canonicalDefinitionJson({
          schemaVersion: 1,
          selectionSha256: plannedLevel.selection.sha256,
          regressionChecks: expectedRegression.map(check => check.stableKey).sort(),
        })));
        mismatch(level.selection?.evaluationSha256 !== expectedEvaluationSha256,
          `${selectionAt}.evaluationSha256`);
        mismatch(level.selection?.regressionPoints !== regressionPoints,
          `${selectionAt}.regressionPoints`);
        const regressions: Array<[string, RunRegression | null | undefined]> = [
          ['firstBuild', level.firstBuild?.regression], ['final', level.regression],
        ];
        for (const [name, regression] of regressions) {
          mismatch(!safeInteger(regression?.score) || !safeInteger(regression?.max)
            || regression.max !== regressionPoints || regression.score < 0
            || regression.score > regression.max, `levels.L${level.level}.${name}.regression`);
        }
      } else {
        mismatch(level.regression !== null && level.regression !== undefined,
          `levels.L${level.level}.regression`);
      }
    }
    const selectedChecks = level.selection?.observedChecks;
    if (!Array.isArray(selectedChecks) || selectedChecks.length === 0) {
      mismatch(level.firstBuild?.observations !== undefined,
        `levels.L${level.level}.firstBuild.observations`);
      continue;
    }
    const at = `levels.L${level.level}.firstBuild.observations`;
    const observation = level.firstBuild?.observations;
    const selectedKeys = selectedChecks.map(check => check?.stableKey);
    const plannedKeys = plannedLevel?.selection?.observedChecks?.map(check => check.stableKey);
    const selectedPoints = selectedChecks.every(check => Number.isSafeInteger(check?.points)
      && check.points >= 0)
      ? selectedChecks.reduce((total, check) => total + check.points, 0) : null;
    mismatch(!observation || typeof observation !== 'object' || Array.isArray(observation), at);
    if (!observation || typeof observation !== 'object' || Array.isArray(observation)) continue;
    mismatch(new Set(selectedKeys).size !== selectedKeys.length
      || selectedKeys.some(key => typeof key !== 'string' || !key), `${at}.selectedChecks`);
    mismatch(canonicalDefinitionJson(observation.selectedChecks)
      !== canonicalDefinitionJson(selectedKeys), `${at}.selectedChecks`);
    mismatch(!plannedLevel || plannedLevel.selection?.sha256 !== level.selection?.sha256,
      `levels.L${level.level}.selection.sha256`);
    mismatch(!Array.isArray(plannedKeys)
      || canonicalDefinitionJson(plannedKeys) !== canonicalDefinitionJson(selectedKeys),
    `levels.L${level.level}.selection.observedChecks`);
    mismatch(observation.selectionSha256 !== level.selection?.sha256, `${at}.selectionSha256`);
    mismatch(observation.scoreContribution !== false, `${at}.scoreContribution`);
    mismatch(observation.repairVisible !== false, `${at}.repairVisible`);
    mismatch(observation.sourceSha256 !== (level.firstBuild?.source?.sha256 ?? null),
      `${at}.sourceSha256`);
    mismatch(!Array.isArray(observation.reportedChecks)
      || new Set(observation.reportedChecks).size !== observation.reportedChecks.length
      || observation.reportedChecks.some(key => !selectedKeys.includes(key)), `${at}.reportedChecks`);
    const reportedPoints = Array.isArray(observation.reportedChecks)
      ? observation.reportedChecks.reduce((total, key) => total
        + (selectedChecks.find(check => check.stableKey === key)?.points ?? 0), 0) : null;
    const numericEvidence = safeInteger(observation.observedPoints)
      && observation.observedPoints >= 0 && safeInteger(observation.passedPoints)
      && observation.passedPoints >= 0 && observation.passedPoints <= observation.observedPoints;
    mismatch(observation.observedPoints !== null && !numericEvidence, `${at}.observedPoints`);
    mismatch(observation.passedPoints !== null && !numericEvidence, `${at}.passedPoints`);
    mismatch(numericEvidence && (selectedPoints === null
      || (safeInteger(observation.observedPoints)
        && (observation.observedPoints > selectedPoints
          || observation.observedPoints !== reportedPoints))), `${at}.observedPoints`);
    const exactArtifact = `first-build-l${level.level}-observed/${ARTIFACT_FILE.gradeBundle}`;
    mismatch(observation.artifact !== null && observation.artifact !== exactArtifact, `${at}.artifact`);
    mismatch(observation.artifact === null && numericEvidence, `${at}.artifact`);
    mismatch(observation.artifact !== null && !numericEvidence, `${at}.artifact`);
  }
  if (!dependencyMode && (exactLevels || gatedPrefix)
    && ['passed', 'app_failure'].includes(run.outcome?.kind ?? '')) {
    const runOutcome = run.outcome!;
    const levelOutcomes = (run.levels ?? []).map(level => level.outcome?.kind);
    mismatch(runOutcome.kind === 'passed' && levelOutcomes.some(kind => kind !== 'passed'),
      'outcome.kind');
    mismatch(runOutcome.kind === 'app_failure'
      && !levelOutcomes.some(kind => kind === 'app_failure'), 'outcome.kind');
    for (const level of run.levels ?? []) {
      const plannedLevel = condition?.requested?.levels?.find(item => item.level === level.level);
      const repair = level.repair;
      const at = `levels.L${level.level}.repair`;
      const validObject = repair && typeof repair === 'object' && !Array.isArray(repair);
      mismatch(!validObject, at);
      if (!validObject) continue;
      mismatch(!integer(level.fixRounds) || level.fixRounds < 0
        || level.fixRounds > plan.definition.budgets.fixRounds, `levels.L${level.level}.fixRounds`);
      mismatch(repair.budgetRounds !== plan.definition.budgets.fixRounds, `${at}.budgetRounds`);
      mismatch(repair.roundsUsed !== level.fixRounds, `${at}.roundsUsed`);
      const validScore = integer(level.score) && integer(level.max)
        && level.max > 0 && level.score >= 0 && level.score <= level.max;
      mismatch(!validScore, `levels.L${level.level}.score`);
      const declaredPoints = plannedLevel?.selection?.scoredPoints;
      mismatch(!safeInteger(declaredPoints) || declaredPoints <= 0,
        `levels.L${level.level}.plannedSelection.scoredPoints`);
      mismatch(validScore && level.max !== declaredPoints, `levels.L${level.level}.max`);
      const firstBuildScore = level.firstBuild?.score;
      const firstBuildMax = level.firstBuild?.max;
      const validFirstBuildScore = integer(firstBuildScore)
        && integer(firstBuildMax) && firstBuildMax > 0
        && firstBuildScore >= 0 && firstBuildScore <= firstBuildMax;
      mismatch(!validFirstBuildScore, `levels.L${level.level}.firstBuild.score`);
      mismatch(validFirstBuildScore && firstBuildMax !== declaredPoints,
        `levels.L${level.level}.firstBuild.max`);
      const outcomes: Array<[string, RunOutcome | undefined]> = [
        ['firstBuild', level.firstBuild?.outcome], ['final', level.outcome],
      ];
      for (const [phase, outcome] of outcomes) {
        const atOutcome = `levels.L${level.level}.${phase}.outcome`;
        mismatch(!outcome || !['passed', 'app_failure'].includes(outcome.kind ?? ''),
          `${atOutcome}.kind`);
        mismatch(Array.isArray(outcome?.inconclusive) && outcome.inconclusive.length > 0,
          `${atOutcome}.inconclusive`);
        mismatch(Array.isArray(outcome?.harnessFailures) && outcome.harnessFailures.length > 0,
          `${atOutcome}.harnessFailures`);
      }
      if (level.outcome?.kind === 'passed') {
        const expected = integer(level.fixRounds) && level.fixRounds > 0
          ? 'corrected' : 'not-needed';
        mismatch(repair.status !== expected, `${at}.status`);
        mismatch(validScore && level.score !== level.max, `levels.L${level.level}.score`);
      } else if (level.outcome?.kind === 'app_failure') {
        const exhausted = repair.status === 'budget-exhausted'
          && repair.roundsUsed === repair.budgetRounds;
        const paused = repair.status === 'incomplete'
          && integer(repair.roundsUsed) && integer(repair.budgetRounds)
          && repair.roundsUsed > 0
          && (repair.stopReason === 'no-source-change'
            ? repair.roundsUsed <= repair.budgetRounds
            : repair.stopReason === 'repeated-findings'
              && repair.roundsUsed < repair.budgetRounds);
        mismatch(!exhausted && !paused, `${at}.status`);
        // A perfect level score does not excuse an inherited or contract regression.
        const inheritedDeficit = integer(level.regression?.score)
          && integer(level.regression?.max)
          && level.regression.max > 0
          && level.regression.score < level.regression.max;
        const contractFailure = level.contractPass === false;
        mismatch(validScore && level.score === level.max
          && !inheritedDeficit && !contractFailure, `levels.L${level.level}.score`);
      } else {
        mismatch(true, `${at}.levelOutcome`);
      }
    }
  }
  if (dependencyMode) {
    validateDependencyEvidence(plan, attempt, run, resultDir, expectedLevels,
      interruptedPrefix, mismatch);
  }
  if (plan.definition.budgets.maxCostUsdPerAttempt !== null) {
    const cost = run.totals?.costUsd;
    const missingAllowed = interruptedPrefix && actualLevels.length === 0 && cost == null;
    mismatch(!missingAllowed && (!finite(cost)
      || cost > plan.definition.budgets.maxCostUsdPerAttempt), 'totals.costUsd');
  }
  if (agent?.costLimit === 'native') {
    const agentFailed = ['provider_failure', 'harness_failure'].includes(run.outcome?.kind ?? '');
    mismatch(!agentFailed && run.totals?.costComplete !== true, 'totals.costComplete');
    mismatch(!agentFailed
      && !durableCostLedger(run as Parameters<typeof durableCostLedger>[0]).complete,
      'costEvidence');
  }
  if (mismatches.length) {
    throw new Error(`${ARTIFACT_FILE.run} does not match its planned campaign attempt: ${mismatches.join(', ')}`);
  }
  return run;
}
