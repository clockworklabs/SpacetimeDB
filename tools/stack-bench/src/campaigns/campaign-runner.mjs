import { randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';


import { currentEngineIdentity, emptyArtifactIdentities, readArtifactPayload,
  writeArtifact } from '../evidence/artifacts.mjs';
import { acquireCampaignLock, releaseCampaignLock } from './campaign-lock.mjs';
import { compileCampaignFile } from './campaign-compiler.mjs';
import { claimNextAttempt, finishCampaignExecution, initializeCampaignDirectory,
  markInterruptedExecution, readCampaignState, writeCampaignState } from './campaign-scheduler.mjs';
import { rescueSupervisedLease, runBounded } from '../references/reference-live.mjs';
import { canonicalDefinitionJson } from '../composition/definition-plan.mjs';
import { runPreflight } from '../runtime/preflight.mjs';
import { sha256 } from '../evidence/provenance.mjs';
import { validateReleaseManifest } from '../releases/release-manifest.mjs';
import { RUN_INDEX_CAP } from '../composition/tracks.mjs';
import { readProgressionState } from '../progression/progression-state.mjs';
import { liveProgressionStatus } from '../progression/live-progression.mjs';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../progression/progression-definition.mjs';
import { AGENT_ADAPTER_REGISTRY } from '../agents/agent-adapters.mjs';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.mjs';
import { executeStackCapability } from '../stacks/stack-adapter-contract.mjs';
import { DEFAULT_BUILD_IMAGE } from '../composition/product-config.mjs';
import { aggregateRunOutcome } from '../evidence/outcomes.mjs';
import { readCampaignAdmission, validateCampaignAdmission } from './campaign-admission.mjs';

import { STACK_BENCH_ROOT as ROOT } from '../project-paths.mjs';
const BENCH = join(ROOT, 'commands', 'bench.mjs');

export function expectedDependencyRunOutcomeKind(levels, terminalOutcome) {
  const expected = aggregateRunOutcome(levels ?? []).kind;
  if (terminalOutcome?.kind !== 'passed' && expected === 'passed') return null;
  return expected;
}

function contained(root, path, label) {
  const absoluteRoot = resolve(root);
  const absolute = resolve(absoluteRoot, path);
  const rel = relative(absoluteRoot, absolute);
  if (rel === '..' || rel.startsWith(`..${sep}`) || rel === '') {
    throw new Error(`${label} is not a child of the campaign directory`);
  }
  return absolute;
}

export function attemptArgv(plan, attempt, output, runIndex, campaignPlanPath = null,
  progressionResumeFrom = null, campaignAdmissionId = null, maxBudgetUsd = undefined) {
  if (!Number.isInteger(runIndex) || runIndex < 0 || runIndex > RUN_INDEX_CAP) {
    throw new Error(`attempt ${attempt.id} requires a run slot from 0 through ${RUN_INDEX_CAP}`);
  }
  const levels = `${Math.min(...attempt.levels)}-${Math.max(...attempt.levels)}`;
  const dependencyMode = attempt.mode?.id === 'dependency';
  if (dependencyMode !== Boolean(attempt.dependencyPolicy)) {
    throw new Error(`attempt ${attempt.id} mode and dependency policy do not match`);
  }
  const hasFeatureCatalog = Boolean(plan.featureCatalog);
  if (hasFeatureCatalog !== Boolean(attempt.featureCatalog)) {
    throw new Error(`attempt ${attempt.id} feature catalog does not match its campaign`);
  }
  const guidanceDocument = attempt.condition?.guidance?.documents?.[attempt.stack];
  if (!guidanceDocument) {
    throw new Error(`attempt ${attempt.id} has no guidance document for ${attempt.stack}`);
  }
  const plannedPricing = { unit: plan.definition.pricing.unit,
    rates: plan.definition.pricing.models[attempt.model] };
  if (canonicalDefinitionJson(attempt.pricing) !== canonicalDefinitionJson(plannedPricing)) {
    throw new Error(`attempt ${attempt.id} pricing does not match its campaign`);
  }
  const args = [BENCH,
    '--backend', attempt.stack,
    '--track', plan.definition.track];
  if (typeof campaignPlanPath !== 'string' || !campaignPlanPath) {
    throw new Error(`attempt ${attempt.id} requires its compiled campaign plan path`);
  }
  args.push('--campaign-file', resolve(campaignPlanPath),
    '--campaign-sha256', plan.contentSha256,
    '--campaign-attempt-id', attempt.id);
  if (campaignAdmissionId !== null) {
    if (typeof campaignAdmissionId !== 'string' || !campaignAdmissionId) {
      throw new Error(`attempt ${attempt.id} has an invalid campaign admission id`);
    }
    args.push('--campaign-admission-id', campaignAdmissionId);
  }
  if (hasFeatureCatalog) {
    if (canonicalDefinitionJson(attempt.featureCatalog)
      !== canonicalDefinitionJson(plan.featureCatalog?.identity)) {
      throw new Error(`attempt ${attempt.id} feature catalog identity does not match its campaign`);
    }
    args.push('--feature-catalog-sha256', attempt.featureCatalog.sha256);
  }
  if (dependencyMode) {
    args.push('--dependency-policy-sha256', attempt.dependencyPolicy.sha256);
  }
  if (dependencyMode) {
    if (progressionResumeFrom !== null) {
      if (typeof progressionResumeFrom !== 'string' || !progressionResumeFrom) {
        throw new Error(`attempt ${attempt.id} has an invalid progression resume directory`);
      }
      args.push('--progression-resume-from', resolve(progressionResumeFrom));
    }
  } else {
    if (progressionResumeFrom !== null) {
      throw new Error(`strict attempt ${attempt.id} cannot resume dependency progression state`);
    }
    args.push('--levels', levels);
  }
  args.push('--run-index', String(runIndex),
    '--out', output,
    '--agent-adapter', attempt.agentAdapter,
    '--model', attempt.model,
    '--pricing-json', JSON.stringify(attempt.pricing),
    '--guidance', attempt.guidance,
    '--fix-rounds', String(plan.definition.budgets.fixRounds),
    '--parent-attempt-id', attempt.id,
    '--no-media');
  if (!dependencyMode) {
    args.push('--guidance-document-json', JSON.stringify(guidanceDocument),
      '--condition-json', JSON.stringify(attempt.condition),
      '--selection-json', JSON.stringify(plan.definition.selection));
  }
  for (const pack of plan.definition.selection.packs ?? []) args.push('--pack', pack);
  for (const check of plan.definition.selection.checks ?? []) args.push('--check', check);
  if (!dependencyMode) args.push('--skills-json', JSON.stringify(attempt.skills));
  const plannedBudget = plan.definition.budgets.maxCostUsdPerAttempt;
  const executionBudget = maxBudgetUsd === undefined ? plannedBudget : maxBudgetUsd;
  if (executionBudget !== null) {
    if (!Number.isFinite(executionBudget) || executionBudget <= 0
      || (plannedBudget !== null && executionBudget > plannedBudget)) {
      throw new Error(`attempt ${attempt.id} has an invalid remaining cost budget`);
    }
    args.push('--max-budget-usd', String(Number(executionBudget.toFixed(6))));
  }
  return args;
}

export function validateCampaignRun(plan, attempt, run, {
  buildImage = null, resultDir = null,
} = {}) {
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter
    && item.model === attempt.model);
  const condition = plan.conditions.find(item => item.sha256 === attempt.condition?.sha256);
  const expectedLevels = [...attempt.levels].sort((a, b) => a - b);
  const actualLevels = (run.levels ?? []).map(level => level.level).sort((a, b) => a - b);
  const dependencyMode = attempt.mode?.id === 'dependency';
  const exactLevels = canonicalDefinitionJson(actualLevels) === canonicalDefinitionJson(expectedLevels);
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
    && ['harness_failure', 'ungraded'].includes(run.outcome?.kind);
  const dependencyPrefix = dependencyMode && actualLevels.length > 0
    && actualLevels.length <= expectedLevels.length
    && actualLevels.every((level, index) => level === expectedLevels[index])
    && run.validation?.ladder?.policy === 'dependency-gated';
  const mismatches = [];
  const mismatch = (condition, field) => { if (condition) mismatches.push(field); };
  mismatch(run.artifactEnvelope?.attempt?.parentId !== attempt.id, 'attempt.parentId');
  mismatch(canonicalDefinitionJson(run.mode ?? null)
    !== canonicalDefinitionJson(attempt.mode), 'mode');
  mismatch(run.track !== plan.definition.track, 'track');
  mismatch(run.backend !== attempt.stack, 'backend');
  mismatch(run.model !== attempt.model, 'model');
  mismatch(canonicalDefinitionJson(run.pricing ?? null)
    !== canonicalDefinitionJson(attempt.pricing), 'pricing');
  mismatch(run.guidance !== attempt.guidance, 'guidance');
  mismatch(!condition || canonicalDefinitionJson(run.condition)
    !== canonicalDefinitionJson(attempt.condition), 'condition');
  mismatch(canonicalDefinitionJson(run.selectionRequest)
    !== canonicalDefinitionJson(plan.definition.selection), 'selectionRequest');
  mismatch(canonicalDefinitionJson(run.featureCatalog ?? null)
    !== canonicalDefinitionJson(attempt.featureCatalog ?? null), 'featureCatalog');
  mismatch(canonicalDefinitionJson(run.dependencyPolicy ?? null)
    !== canonicalDefinitionJson(attempt.dependencyPolicy ?? null), 'dependencyPolicy');
  const expectedProgressionOwner = attempt.dependencyPolicy ? { schemaVersion: 1,
    campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
    attempt: { id: attempt.id, track: plan.definition.track, stack: attempt.stack,
      agentAdapter: attempt.agentAdapter, model: attempt.model,
      conditionSha256: attempt.condition.sha256 } } : null;
  mismatch(canonicalDefinitionJson(run.progressionOwner ?? null)
    !== canonicalDefinitionJson(expectedProgressionOwner), 'progressionOwner');
  mismatch(canonicalDefinitionJson(run.skills) !== canonicalDefinitionJson(attempt.skills), 'skills');
  mismatch(!exactLevels && !interruptedPrefix && !gatedPrefix && !dependencyPrefix, 'levels');
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
      const projectChecks = checks => checks?.map(check => ({
        stableKey: check?.stableKey, points: check?.points, treatment: check?.treatment,
      }));
      // A run that omits a selection field entirely must register as a
      // mismatch, not crash canonicalization: the crash message names no
      // field, and a validator that throws for a reason other than its own
      // verdict hides which identity actually diverged.
      const canonicalOrMissing = value => value === undefined
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
        for (const [name, regression] of [['firstBuild', level.firstBuild?.regression],
          ['final', level.regression]]) {
          mismatch(!Number.isSafeInteger(regression?.score)
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
    mismatch(!plannedLevel || plannedLevel.selection?.sha256 !== level.selection.sha256,
      `levels.L${level.level}.selection.sha256`);
    mismatch(!Array.isArray(plannedKeys)
      || canonicalDefinitionJson(plannedKeys) !== canonicalDefinitionJson(selectedKeys),
    `levels.L${level.level}.selection.observedChecks`);
    mismatch(observation.selectionSha256 !== level.selection.sha256, `${at}.selectionSha256`);
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
    const numericEvidence = Number.isSafeInteger(observation.observedPoints)
      && observation.observedPoints >= 0 && Number.isSafeInteger(observation.passedPoints)
      && observation.passedPoints >= 0 && observation.passedPoints <= observation.observedPoints;
    mismatch(observation.observedPoints !== null && !numericEvidence, `${at}.observedPoints`);
    mismatch(observation.passedPoints !== null && !numericEvidence, `${at}.passedPoints`);
    mismatch(numericEvidence && (selectedPoints === null
      || observation.observedPoints > selectedPoints
      || observation.observedPoints !== reportedPoints), `${at}.observedPoints`);
    const exactArtifact = `first-build-l${level.level}-observed/bundle.json`;
    mismatch(observation.artifact !== null && observation.artifact !== exactArtifact, `${at}.artifact`);
    mismatch(observation.artifact === null && numericEvidence, `${at}.artifact`);
    mismatch(observation.artifact !== null && !numericEvidence, `${at}.artifact`);
  }
  if (!dependencyMode && (exactLevels || gatedPrefix)
    && ['passed', 'app_failure'].includes(run.outcome?.kind)) {
    const levelOutcomes = (run.levels ?? []).map(level => level.outcome?.kind);
    mismatch(run.outcome.kind === 'passed' && levelOutcomes.some(kind => kind !== 'passed'),
      'outcome.kind');
    mismatch(run.outcome.kind === 'app_failure'
      && !levelOutcomes.some(kind => kind === 'app_failure'), 'outcome.kind');
    for (const level of run.levels ?? []) {
      const plannedLevel = condition?.requested?.levels?.find(item => item.level === level.level);
      const repair = level.repair;
      const at = `levels.L${level.level}.repair`;
      const validObject = repair && typeof repair === 'object' && !Array.isArray(repair);
      mismatch(!validObject, at);
      if (!validObject) continue;
      mismatch(!Number.isInteger(level.fixRounds) || level.fixRounds < 0
        || level.fixRounds > plan.definition.budgets.fixRounds, `levels.L${level.level}.fixRounds`);
      mismatch(repair.budgetRounds !== plan.definition.budgets.fixRounds, `${at}.budgetRounds`);
      mismatch(repair.roundsUsed !== level.fixRounds, `${at}.roundsUsed`);
      const validScore = Number.isInteger(level.score) && Number.isInteger(level.max)
        && level.max > 0 && level.score >= 0 && level.score <= level.max;
      mismatch(!validScore, `levels.L${level.level}.score`);
      const declaredPoints = plannedLevel?.selection?.scoredPoints;
      mismatch(!Number.isSafeInteger(declaredPoints) || declaredPoints <= 0,
        `levels.L${level.level}.plannedSelection.scoredPoints`);
      mismatch(validScore && level.max !== declaredPoints, `levels.L${level.level}.max`);
      const firstBuildScore = level.firstBuild?.score;
      const firstBuildMax = level.firstBuild?.max;
      const validFirstBuildScore = Number.isInteger(firstBuildScore)
        && Number.isInteger(firstBuildMax) && firstBuildMax > 0
        && firstBuildScore >= 0 && firstBuildScore <= firstBuildMax;
      mismatch(!validFirstBuildScore, `levels.L${level.level}.firstBuild.score`);
      mismatch(validFirstBuildScore && firstBuildMax !== declaredPoints,
        `levels.L${level.level}.firstBuild.max`);
      for (const [phase, outcome] of [['firstBuild', level.firstBuild?.outcome],
        ['final', level.outcome]]) {
        const atOutcome = `levels.L${level.level}.${phase}.outcome`;
        mismatch(!outcome || !['passed', 'app_failure'].includes(outcome.kind), `${atOutcome}.kind`);
        mismatch(Array.isArray(outcome?.inconclusive) && outcome.inconclusive.length > 0,
          `${atOutcome}.inconclusive`);
        mismatch(Array.isArray(outcome?.harnessFailures) && outcome.harnessFailures.length > 0,
          `${atOutcome}.harnessFailures`);
      }
      if (level.outcome?.kind === 'passed') {
        const expected = level.fixRounds > 0 ? 'corrected' : 'not-needed';
        mismatch(repair.status !== expected, `${at}.status`);
        mismatch(validScore && level.score !== level.max, `levels.L${level.level}.score`);
      } else if (level.outcome?.kind === 'app_failure') {
        const exhausted = repair.status === 'budget-exhausted'
          && repair.roundsUsed === repair.budgetRounds;
        const paused = repair.status === 'incomplete'
          && repair.stopReason === 'repeated-findings'
          && repair.roundsUsed > 0 && repair.roundsUsed < repair.budgetRounds;
        mismatch(!exhausted && !paused, `${at}.status`);
        // The level score covers only the newly requested criteria. A level can
        // earn every one of those points and still fail if an inherited
        // guarantee regressed. Test-development checks cannot create an
        // ordinary application failure.
        const inheritedDeficit = Number.isInteger(level.regression?.score)
          && Number.isInteger(level.regression?.max)
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
    if (typeof resultDir !== 'string' || !resultDir) {
      mismatch(true, 'progressionState');
    } else {
      try {
        const owner = { ...expectedProgressionOwner,
          workspace: { appDirectory: 'source' } };
        const progression = compileProgressionInput(dependencyRuntimeDefinition(
          plan.featureCatalog, plan.dependencyPolicy));
        const stored = readProgressionState(join(resolve(resultDir), 'progression-state.json'), {
          progression,
          featureCatalogIdentity: plan.featureCatalog.identity,
          dependencyPolicyIdentity: plan.dependencyPolicy.identity,
          owner,
        });
        const storedStatus = liveProgressionStatus(stored.state);
        mismatch(canonicalDefinitionJson(run.progressionStatus)
          !== canonicalDefinitionJson(storedStatus), 'progressionStatus');
        mismatch(canonicalDefinitionJson(ladder?.requestedLevels)
          !== canonicalDefinitionJson(expectedLevels), 'validation.ladder.requestedLevels');
        const conclusiveLevels = [...new Set(stored.state.attempts
          .filter(item => item.outcome === 'conclusive').map(item => item.level))];
        mismatch(canonicalDefinitionJson(ladder?.completedLevels)
          !== canonicalDefinitionJson(conclusiveLevels), 'validation.ladder.completedLevels');
        let hasPreGradeFailure = false;
        for (const level of run.levels ?? []) {
          const last = [...stored.state.attempts].reverse()
            .find(item => item.level === level.level);
          const preGradeFailure = !last
            && run.outcome?.kind === 'harness_failure'
            && level === run.levels.at(-1)
            && level.level === storedStatus.level
            && typeof level.error === 'string' && level.error.length > 0
            && level.score === null && level.max === null
            && level.outcome?.kind === 'harness_failure';
          hasPreGradeFailure ||= preGradeFailure;
          mismatch(!last && !preGradeFailure, `levels.L${level.level}.progressionAttempt`);
          if (!last) continue;
          mismatch(last?.selectionSha256 && level.selection?.sha256 !== last.selectionSha256,
            `levels.L${level.level}.selection.sha256`);
          const validScore = Number.isSafeInteger(level.score) && Number.isSafeInteger(level.max)
            && level.max > 0 && level.score >= 0 && level.score <= level.max;
          mismatch(last?.outcome === 'conclusive' && !validScore,
            `levels.L${level.level}.score`);
          mismatch(last?.outcome === 'inconclusive' && level.graded !== false,
            `levels.L${level.level}.graded`);
        }
        if (stored.state.phase === 'terminal') {
          // The graph records feature progress. The run outcome also includes
          // whole-app checks such as the UI contract and inherited guarantees.
          // A graph can therefore finish while one of those checks still fails.
          const expectedOutcome = expectedDependencyRunOutcomeKind(
            run.levels, stored.state.terminalOutcome);
          mismatch(expectedOutcome === null || run.outcome?.kind !== expectedOutcome, 'outcome.kind');
        } else if (!interruptedPrefix && !hasPreGradeFailure) {
          mismatch(stored.state.attempts.at(-1)?.outcome !== 'inconclusive',
            'progressionState.phase');
        }
      } catch {
        mismatch(true, 'progressionState');
      }
    }
  }
  if (plan.definition.budgets.maxCostUsdPerAttempt !== null) {
    const cost = run.totals?.costUsd;
    const missingAllowed = interruptedPrefix && actualLevels.length === 0 && cost == null;
    mismatch(!missingAllowed && (!Number.isFinite(cost)
      || cost > plan.definition.budgets.maxCostUsdPerAttempt), 'totals.costUsd');
  }
  if (mismatches.length) {
    throw new Error(`run.json does not match its planned campaign attempt: ${mismatches.join(', ')}`);
  }
  return run;
}

const TRANSIENT_PROVIDER_STATUSES = new Set([500, 502, 503, 504, 529]);

export function campaignRetryAuthority(run, { recoveryClean = false, requireCostReceipt = false } = {}) {
  const outcome = run?.outcome;
  const providerStatus = outcome?.provider?.providerStatus;
  const transient = outcome?.kind === 'harness_failure'
    && outcome.phase === 'coding-session'
    && outcome.reason !== 'provider-throttle-exhausted'
    && TRANSIENT_PROVIDER_STATUSES.has(providerStatus);
  const budgetKnown = !requireCostReceipt || (run?.totals?.costComplete === true
    && Number.isFinite(run?.totals?.costUsd) && run.totals.costUsd >= 0);
  return {
    transient,
    recoveryClean: recoveryClean === true,
    budgetKnown,
    cause: transient ? `provider-http-${providerStatus}` : null,
  };
}

function readAttemptResult(plan, attempt, output, processResult) {
  const withRetryAuthority = result => ({ ...result,
    retryAuthority: campaignRetryAuthority(result.run, {
      recoveryClean: publicRecoveryProvesCleanup(output, attempt.stack),
      requireCostReceipt: plan.definition.budgets.maxCostUsdPerAttempt !== null,
    }) });
  if (processResult.cancelled === true) {
    return withRetryAuthority({ exitCode: processResult.code, timedOut: false,
      run: { outcome: { kind: 'scheduler_interrupted',
        reason: 'campaign cancellation requested' } } });
  }
  const runPath = join(output, 'run.json');
  let run = null;
  let artifactError = null;
  if (existsSync(runPath)) {
    try {
      run = readArtifactPayload(runPath, { expectedKind: 'benchmark_run' });
      validateCampaignRun(plan, attempt, run, {
        buildImage: processResult.buildImage,
        resultDir: output,
      });
    }
    catch (error) { artifactError = error; }
  }
  if (artifactError) {
    const processDetail = processResult.code !== 0
      ? processFailureDetail(processResult) : null;
    return withRetryAuthority({ exitCode: processResult.code, timedOut: processResult.timedOut,
      run: { outcome: { kind: 'harness_failure',
        reason: `${processDetail ? `${processDetail}; ` : ''}partial run.json is invalid: ${artifactError.message}` } } });
  }
  if (!run && processResult.code !== 0 && !processResult.timedOut) {
    const detail = processFailureDetail(processResult);
    return withRetryAuthority({ exitCode: processResult.code, timedOut: false, run: { outcome: {
      kind: 'harness_failure', reason: detail || 'attempt ended before producing run.json' } } });
  }
  return withRetryAuthority({ exitCode: processResult.code, timedOut: processResult.timedOut, run });
}

export function remainingAttemptCostBudget(plan, claim, directory) {
  const cap = plan.definition.budgets.maxCostUsdPerAttempt;
  if (cap === null) return null;
  let spent = 0;
  for (const output of claim.priorOutputs ?? []) {
    const runPath = join(contained(directory, output, 'prior attempt output'), 'run.json');
    if (!existsSync(runPath)) {
      throw new Error(`cannot retry ${claim.attempt.id}: prior provider spend is unknown`);
    }
    const run = readArtifactPayload(runPath, { expectedKind: 'benchmark_run' });
    if (run.totals?.costComplete !== true || !Number.isFinite(run.totals?.costUsd)
      || run.totals.costUsd < 0) {
      throw new Error(`cannot retry ${claim.attempt.id}: prior provider spend is unknown`);
    }
    spent += run.totals.costUsd;
  }
  const remaining = Number((cap - spent).toFixed(6));
  if (remaining <= 0) {
    throw new Error(`cannot retry ${claim.attempt.id}: its $${cap} cost cap is exhausted`);
  }
  return remaining;
}

export function processFailureDetail(processResult) {
  const text = processResult.stderrTail || processResult.stdoutTail
    || processResult.error?.message || '';
  const lines = text.split(/\r?\n/).map(line => line.trim()).filter(Boolean);
  const explicit = lines.filter(line => /^Error:\s+/.test(line)).at(-1);
  return (explicit ?? lines.slice(-4).join(' | ')).slice(0, 800);
}

export function verifyCampaignRuntime(plan, env = process.env) {
  if (plan.state !== 'frozen') return structuredClone(plan.definition.runtime);
  const expected = plan.definition.runtime;
  const plannedEngine = plan.identities?.engine;
  if (!plannedEngine?.sha256 || currentEngineIdentity().sha256 !== plannedEngine.sha256) {
    throw new Error('running Stack Bench engine does not match the frozen campaign');
  }
  if (env.STACK_BENCH_CONTROLLER_IMAGE !== expected.controllerImage) {
    throw new Error('running controller image does not match the frozen campaign');
  }
  if (expected.releaseManifestSha256 === null) return structuredClone(expected);
  if (typeof env.STACK_BENCH_RELEASE_MANIFEST !== 'string'
    || env.STACK_BENCH_RELEASE_MANIFEST.trim() === '') {
    throw new Error('STACK_BENCH_RELEASE_MANIFEST is required for a frozen campaign');
  }
  let bytes;
  try { bytes = readFileSync(resolve(env.STACK_BENCH_RELEASE_MANIFEST)); }
  catch (error) {
    throw new Error(`cannot read frozen campaign release manifest: ${error.message}`, { cause: error });
  }
  if (sha256(bytes) !== expected.releaseManifestSha256) {
    throw new Error('release manifest does not match the frozen campaign');
  }
  let manifest;
  try { manifest = validateReleaseManifest(JSON.parse(bytes.toString('utf8'))); }
  catch (error) {
    throw new Error(`frozen campaign release manifest is invalid: ${error.message}`, { cause: error });
  }
  const controller = manifest.images.find(image => image.role === 'controller');
  const build = manifest.images.find(image => image.role === 'build-sandbox');
  if (controller?.reference !== expected.controllerImage
    || build?.reference !== expected.buildImage
    || controller?.platform !== expected.platform
    || build?.platform !== expected.platform) {
    throw new Error('release manifest images do not match the frozen campaign');
  }
  return structuredClone(expected);
}

export function campaignExecutionEnvironment(plan, env = process.env) {
  const executionEnv = { ...env };
  if (plan.definition.runtime.buildImage !== null) {
    if (executionEnv.STACK_BENCH_IMAGE
      && executionEnv.STACK_BENCH_IMAGE !== plan.definition.runtime.buildImage) {
      throw new Error('ambient STACK_BENCH_IMAGE conflicts with the campaign build image');
    }
    executionEnv.STACK_BENCH_IMAGE = plan.definition.runtime.buildImage;
  }
  verifyCampaignRuntime(plan, executionEnv);
  return executionEnv;
}

export function campaignSlotEnvironment(env, stack, runIndex) {
  const executionEnv = { ...env };
  if (stack !== 'spacetime') return executionEnv;
  const base = new URL(executionEnv.STACK_BENCH_STDB_URI ?? 'http://127.0.0.1:3210');
  if (base.protocol !== 'http:'
    || !['127.0.0.1', 'localhost', '[::1]'].includes(base.hostname)
    || !base.port) {
    throw new Error(`STACK_BENCH_STDB_URI must be an explicit loopback port, got ${base}`);
  }
  const port = Number(base.port) + runIndex;
  if (!Number.isInteger(runIndex) || runIndex < 0 || port > 65535) {
    throw new Error(`campaign run slot ${runIndex} cannot allocate a SpacetimeDB host port`);
  }
  base.port = String(port);
  executionEnv.STACK_BENCH_STDB_URI = base.toString().replace(/\/$/, '');
  return executionEnv;
}

function assertAdmissionReferences(plan, directory, state) {
  const ids = [...new Set(state.attempts.flatMap(attempt =>
    attempt.executions.map(execution => execution.admissionId)))];
  for (const id of ids) {
    const admission = readCampaignAdmission(directory, id, plan);
    if (!admission.ok) throw new Error(`campaign execution references failed admission ${id}`);
  }
  return state;
}

const RESOURCE_FREE_REQUIREMENTS = Object.freeze({
  docker: false,
  services: false,
  ports: false,
  credentials: false,
  providerAccess: false,
});

function hasNoAgentResources(agent) {
  return agent.costLimit === 'non-billable'
    && agent.apiKeyEnvironmentVariable === null
    && agent.credentialEnvironmentVariables.length === 0
    && agent.credentialFiles.length === 0
    && agent.outboundDestinations.length === 0
    && agent.requiredExecutables.length === 0
    && agent.credentialStatusCommand === null;
}

function hasNoStackResources(stack) {
  const adapter = STACK_ADAPTER_REGISTRY.get(stack.id);
  const capability = adapter.capabilities.admission;
  if (!capability?.operations.includes('requirements')) return false;
  return canonicalDefinitionJson(executeStackCapability(adapter, 'admission', 'requirements'))
    === canonicalDefinitionJson(RESOURCE_FREE_REQUIREMENTS);
}

export function campaignUsesNoExternalResources(plan) {
  return plan.stacks.every(hasNoStackResources)
    && plan.agents.every(agent => hasNoAgentResources(AGENT_ADAPTER_REGISTRY.get(agent.adapter)));
}

function resourceFreeAdmissionReport(request) {
  return {
    schemaVersion: 1,
    request: {
      backends: request.backends,
      track: request.track,
      levels: request.levelList,
      runIndex: request.runIndex,
      agentAdapter: request.agentAdapter,
      packs: request.packIds,
      checks: request.checkKeys,
      recipe: request.recipe ?? null,
      requestedScopeCount: request.requestedScopes?.length ?? 0,
      image: request.image,
      resultsDir: request.resultsDir,
      agentSkills: request.agentSkills ?? null,
      smoke: request.smoke,
    },
    ok: true,
    summary: { passed: 1, failed: 0, warnings: 0 },
    checks: [{ id: 'resources.none', status: 'pass',
      summary: 'The selected stack and agent require no external resources' }],
  };
}

export function inspectCampaign(directory, { requireCurrentInputs = true } = {}) {
  const current = readCampaignState(directory, { requireCurrentInputs });
  return { ...current,
    state: assertAdmissionReferences(current.plan, current.paths.root, current.state) };
}

function publicRecoveryProvesCleanup(output, backend) {
  const path = join(output, 'recovery.json');
  if (!existsSync(path)) return false;
  const recovery = readArtifactPayload(path, { expectedKind: 'recovery' });
  return recovery.status === 'clean'
    && recovery.backend === backend
    && recovery.cleanup?.succeeded === true
    && recovery.cleanup?.retained === false
    && recovery.resources?.backendState === 'released'
    && recovery.resources?.buildContainer?.running !== true
    && Array.isArray(recovery.resources?.locks)
    && recovery.resources.locks.every(resource => resource.released === true);
}

export function runCampaignAdmission(plan, directory,
  { env = process.env, preflight = runPreflight, now = new Date().toISOString(),
    uuid = randomUUID } = {}) {
  const executionEnv = campaignExecutionEnvironment(plan, env);
  const reports = [];
  const resourceFree = campaignUsesNoExternalResources(plan);
  for (const adapter of [...new Set(plan.agents.map(agent => agent.adapter))].sort()) {
    for (let runIndex = 0; runIndex < plan.summary.parallelism; runIndex += 1) {
      const request = {
        backends: plan.stacks.map(stack => stack.id),
        track: plan.definition.track,
        levels: `${Math.min(...plan.definition.levels)}-${Math.max(...plan.definition.levels)}`,
        levelList: plan.definition.levels,
        runIndex,
        agentAdapter: adapter,
        packIds: plan.definition.selection.packs ?? [],
        checkKeys: plan.definition.selection.checks ?? [],
        requestedScopes: plan.conditions.map(condition => condition.requested),
        featureCatalog: plan.featureCatalog,
        mode: plan.definition.mode,
        smoke: true,
        image: plan.definition.runtime.buildImage ?? executionEnv.STACK_BENCH_IMAGE
          ?? DEFAULT_BUILD_IMAGE,
        resultsDir: resolve(directory),
      };
      reports.push(resourceFree ? resourceFreeAdmissionReport(request) : preflight(request,
        { env: campaignSlotEnvironment(executionEnv,
          plan.stacks.some(stack => stack.id === 'spacetime') ? 'spacetime' : null, runIndex) }));
    }
  }
  const id = `${plan.id}-admission-${now.replace(/[-:.TZ]/g, '').slice(0, 14)}-${uuid()}`;
  const payload = validateCampaignAdmission({ schemaVersion: 1, campaignId: plan.id,
    campaignSha256: plan.contentSha256, createdAt: now,
    ok: reports.every(report => report.ok),
    runtime: plan.definition.runtime,
    agents: plan.agents.map(agent => ({ adapter: agent.adapter, model: agent.model,
      identity: agent.identity })),
    conditions: plan.conditions,
    reports }, plan, directory);
  const path = contained(directory, join('admissions', `${id}.json`), 'campaign admission');
  writeArtifact(path, { kind: 'campaign_admission', id,
    identities: emptyArtifactIdentities({ experiment: {
      id: plan.id, version: plan.version, sha256: plan.contentSha256, state: plan.state,
    } }), payload });
  return { id, path, payload };
}

export function prepareCampaign(campaignFile, directory) {
  const plan = compileCampaignFile(resolve(campaignFile));
  const lock = acquireCampaignLock(directory, plan);
  try { return initializeCampaignDirectory(plan, directory); }
  finally { releaseCampaignLock(lock); }
}

export function reconcileCampaign(campaignFile, directory,
  { rescue = rescueSupervisedLease } = {}) {
  const plan = compileCampaignFile(resolve(campaignFile));
  const lock = acquireCampaignLock(directory, plan);
  try {
    const initialized = initializeCampaignDirectory(plan, directory);
    const { state } = inspectCampaign(initialized.paths.root);
    const running = state.attempts.filter(item => item.status === 'running');
    if (!running.length) throw new Error('campaign has no running attempt to reconcile');
    for (const attempt of running) {
      const execution = attempt.executions.at(-1);
      const output = contained(initialized.paths.root, execution.output, 'attempt output');
      const supervisorState = contained(initialized.paths.root,
        join('.private', `${execution.id}.supervisor.json`), 'supervisor state');
      if (existsSync(supervisorState)) {
        rescue(supervisorState, output);
      } else if (!publicRecoveryProvesCleanup(output, attempt.plan.stack)) {
        throw new Error('running attempt has neither private supervisor authority nor public clean recovery proof');
      }
    }
    let reconciled = state;
    for (const attempt of running) {
      reconciled = markInterruptedExecution(reconciled, attempt.executions.at(-1).id, {
        reason: 'controller ended before recording completion; exact-owned cleanup was proven',
      });
    }
    writeCampaignState(initialized.paths.state, plan, reconciled);
    return reconciled;
  } finally { releaseCampaignLock(lock); }
}

export async function executeCampaign(campaignFile, directory,
  { mode = 'frozen', env = process.env, execute = runBounded,
    admit = runCampaignAdmission, rescue = rescueSupervisedLease, signal = null } = {}) {
  const plan = compileCampaignFile(resolve(campaignFile));
  if (!['frozen', 'model-free-trial'].includes(mode)) {
    throw new Error(`unknown campaign execution mode ${JSON.stringify(mode)}`);
  }
  if (mode === 'frozen' && plan.state !== 'frozen') {
    throw new Error('campaign execution requires a frozen plan; draft plans are inspection-only');
  }
  if (mode === 'model-free-trial') {
    if (plan.state !== 'draft') {
      throw new Error('campaign trial requires a draft plan; use campaign run for a frozen plan');
    }
    const billable = plan.agents.filter(agent => agent.costLimit !== 'non-billable');
    if (billable.length) {
      throw new Error(`campaign trial requires non-billable agent adapters; found ${billable
        .map(agent => agent.adapter).join(', ')}`);
    }
    const nonzeroPricing = plan.agents.filter(agent => Object.values(
      plan.definition.pricing.models[agent.model] ?? {}).some(value => value !== 0));
    if (nonzeroPricing.length) {
      throw new Error(`campaign trial requires zero pricing for every selected model; found ${nonzeroPricing
        .map(agent => agent.model).join(', ')}`);
    }
  }
  const executionEnv = campaignExecutionEnvironment(plan, env);
  const lock = acquireCampaignLock(directory, plan);
  try {
    const initialized = initializeCampaignDirectory(plan, directory);
    let { state } = inspectCampaign(initialized.paths.root);
    if (state.attempts.some(attempt => attempt.status === 'running')) {
      throw new Error('campaign has an unresolved running attempt; prove its owned resources are clean before reconciliation');
    }
    if (!state.attempts.some(attempt => attempt.status === 'pending')) return state;
    const admission = admit(plan, initialized.paths.root, { env: executionEnv });
    if (!admission?.payload?.ok || typeof admission.id !== 'string' || !admission.id) {
      throw new Error('campaign-wide preflight admission failed; no attempt was claimed');
    }
    const runClaim = async claim => {
      const output = contained(initialized.paths.root, claim.output, 'attempt output');
      // Preflight bind-mounts the exact attempt output to prove that evidence is
      // durable. The first execution's parent may already exist, while a retry's
      // execution-N directory never does; create both by the same rule before
      // starting the child so retries cannot fail for a different topology.
      mkdirSync(output, { recursive: true });
      const supervisorState = contained(initialized.paths.root,
        join('.private', `${claim.executionId}.supervisor.json`), 'supervisor state');
      let processResult;
      try {
        const remainingBudget = remainingAttemptCostBudget(plan, claim, initialized.paths.root);
        processResult = await execute(process.execPath,
        attemptArgv(plan, claim.attempt, output, claim.runIndex, initialized.paths.plan,
          claim.attempt.mode?.id !== 'dependency' || claim.resumeFrom === null ? null
            : contained(initialized.paths.root, claim.resumeFrom,
              'progression resume directory'), admission.id, remainingBudget), {
          cwd: ROOT,
          env: { ...campaignSlotEnvironment(executionEnv, claim.attempt.stack, claim.runIndex),
            STACK_BENCH_SUPERVISOR_STATE: supervisorState },
          stdio: 'inherit',
          logs: { stdout: join(output, 'process.stdout.log'), stderr: join(output, 'process.stderr.log') },
          timeoutMs: plan.definition.budgets.attemptTimeoutMinutes * 60_000,
          signal,
        });
        processResult.buildImage = executionEnv.STACK_BENCH_IMAGE;
      } catch (error) {
        let reason = `attempt launcher failed: ${error.message}`;
        if (existsSync(supervisorState)) {
          try { rescue(supervisorState, output); }
          catch (cleanupError) {
            return { cleanupRequired: true,
              reason: `${reason}; cleanup failed: ${cleanupError.message}` };
          }
        }
        return { exitCode: null, timedOut: false,
          run: { outcome: { kind: 'harness_failure', reason } } };
      }
      try {
        writeArtifact(join(output, 'process.json'), { kind: 'campaign_process',
          id: `${claim.executionId}-process`,
          attempt: { id: claim.executionId, parentId: claim.attempt.id },
          identities: emptyArtifactIdentities({ experiment: {
            id: plan.id, version: plan.version, sha256: plan.contentSha256, state: plan.state,
          } }),
          payload: { schemaVersion: 1, executionId: claim.executionId, runIndex: claim.runIndex,
            exitCode: processResult.code ?? null, signal: processResult.signal ?? null,
            timedOut: processResult.timedOut === true,
            streams: processResult.logs ? Object.fromEntries(Object.entries(processResult.logs)
              .map(([name, log]) => [name, { ...log, path: `process.${name}.log` }])) : null } });
      } catch (error) {
        let reason = `could not record campaign process evidence: ${error.message}`;
        if (existsSync(supervisorState)) {
          try { rescue(supervisorState, output); }
          catch (cleanupError) {
            return { cleanupRequired: true,
              reason: `${reason}; cleanup failed: ${cleanupError.message}` };
          }
        }
        return { exitCode: processResult.code ?? null, timedOut: false,
          run: { outcome: { kind: 'harness_failure', reason } } };
      }
      let cleanupError = null;
      if (!processResult.ok && existsSync(supervisorState)) {
        try { rescue(supervisorState, output); }
        catch (error) { cleanupError = error; }
      }
      if (cleanupError) {
        return { cleanupRequired: true,
          reason: `attempt cleanup failed: ${cleanupError.message}` };
      }
      return readAttemptResult(plan, claim.attempt, output, processResult);
    };
    const active = new Map();
    const invalidAtStart = state.summary.invalid;
    let stopLaunching = signal?.aborted === true;
    while (true) {
      while (!stopLaunching && !signal?.aborted && active.size < plan.summary.parallelism) {
        const next = claimNextAttempt(state, { admissionId: admission.id });
        state = next.state;
        if (!next.claim) break;
        writeCampaignState(initialized.paths.state, plan, state);
        const promise = runClaim(next.claim).then(result => ({ claim: next.claim, result }),
          error => ({ claim: next.claim, result: { exitCode: null, timedOut: false,
            run: { outcome: { kind: 'harness_failure',
              reason: `campaign worker failed: ${error.message}` } } } }));
        active.set(next.claim.executionId, promise);
      }
      if (!active.size) return state;
      const completed = await Promise.race(active.values());
      active.delete(completed.claim.executionId);
      if (signal?.aborted) stopLaunching = true;
      if (completed.result.cleanupRequired === true) {
        // Keep the execution running in durable state. Its private supervisor
        // authority still exists, so reconcile can retry exact-owned cleanup.
        // Marking it invalid here would strand that authority permanently.
        stopLaunching = true;
        continue;
      }
      state = finishCampaignExecution(state, completed.claim.executionId,
        completed.result, {
          retries: plan.definition.attemptPolicy.retries,
          retryOn: plan.definition.attemptPolicy.retryOn,
        });
      writeCampaignState(initialized.paths.state, plan, state);
      if (state.summary.invalid > invalidAtStart) stopLaunching = true;
    }
  } finally {
    releaseCampaignLock(lock);
  }
}
