import { dirname, resolve } from 'node:path';
import { readArtifact } from '../src/evidence/artifacts.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.js';
import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.js';
import { DEFAULT_TRACK } from '../src/composition/tracks.js';
import { validateCompiledCampaignPlan } from '../src/campaigns/campaign-compiler.js';
import type { CampaignAttemptPlan, CampaignSelection }
  from '../src/campaigns/campaign-compiler.js';
import { campaignAdmissionSmokeReuse, readCampaignAdmission }
  from '../src/campaigns/campaign-admission.js';
import type { CampaignAdmissionSmokeReuse } from '../src/campaigns/campaign-admission.js';
import { compileProgressionInput, dependencyRuntimeDefinition, progressionLevels,
  validateFeatureCatalogInput, validateProgressionInput }
  from '../src/progression/progression-definition.js';
import type { CompiledDependencyPolicyDefinition, CompiledProgressionDefinition,
  ProgressionInput } from '../src/progression/progression-definition.js';
import type { PricingAuthority } from '../src/evidence/pricing-authority.js';

type StudyCondition = CampaignAttemptPlan['condition'];
type UnknownRecord = Record<string, unknown>;

export interface BenchArguments {
  backend?: string;
  track: string;
  levels: string;
  levelsProvided: boolean;
  levelList: number[];
  model: string | null;
  agentAdapter: string;
  pricing?: PricingAuthority | null;
  fixRounds: number;
  maxStalledRepairs: number;
  maxBudgetUsd?: number;
  runIndex: number;
  out?: string;
  app?: string;
  url?: string;
  media: boolean;
  retainBackend?: boolean;
  guidance: string;
  guidanceDocument?: unknown;
  condition?: StudyCondition;
  selectionRequest?: CampaignSelection;
  taskMode?: string;
  packIds: string[];
  checkKeys: string[];
  featureIds: string[];
  requestedSpecifications: string[];
  expectedSpecifications: string[];
  observedSpecifications: string[];
  skipProbe?: boolean;
  skills?: string[];
  apiKey?: string;
  apiKeyFile?: string;
  mutations?: string;
  mutationShardIndex?: number;
  mutationShardCount?: number;
  mutationResumeFrom?: string;
  mutationCheckpointOut?: string;
  mutationBaselineBundle?: string;
  expectedMutationCalibration?: unknown;
  mutationMaxRuntimeMinutes: number;
  referenceMutationOnly?: boolean;
  seedFrom?: string;
  parentAttemptId?: string;
  repairFrom?: string;
  repairLevel?: number;
  recipe?: string;
  campaignFile?: string;
  featureCatalogSha256?: string;
  dependencyPolicySha256?: string;
  campaignSha256?: string;
  campaignAttemptId?: string;
  campaignAdmissionId?: string;
  progressionResumeFrom?: string;
  experimentIdentity?: { id: string; version: string; sha256: string; state: string };
  campaignAdmission?: { id: string } & CampaignAdmissionSmokeReuse;
  runMode?: CampaignAttemptPlan['mode'];
  featureCatalog?: ProgressionInput<CompiledProgressionDefinition>;
  dependencyPolicy?: ProgressionInput<CompiledDependencyPolicyDefinition>;
  progression?: ProgressionInput<CompiledProgressionDefinition>;
  progressionOwner?: UnknownRecord;
}

function normalizeGuidance(value: string): 'neutral' | 'prescribed' {
  if (value === 'neutral' || value === 'prescribed') return value;
  throw new Error(`guidance must be neutral or prescribed, received ${JSON.stringify(value)}`);
}

export function parseBenchArguments(argv: readonly string[]): BenchArguments {
  const args: BenchArguments = { model: null, agentAdapter: 'claude-code',
    fixRounds: 10, runIndex: 0, levels: '1', levelsProvided: false, media: true,
    levelList: [], maxStalledRepairs: 3, guidance: 'prescribed', track: DEFAULT_TRACK,
    packIds: [], checkKeys: [], featureIds: [], requestedSpecifications: [],
    expectedSpecifications: [], observedSpecifications: [],
    mutationMaxRuntimeMinutes: 60 };
  for (let i = 2; i < argv.length; i++) {
    const option = argv[i];
    if (!option) continue;
    const value = (): string => {
      const next = argv[++i];
      if (next === undefined) throw new Error(`${option} requires a value`);
      return next;
    };
    switch (option) {
      case '--backend': args.backend = value(); break;
      case '--track': args.track = value(); break;
      case '--levels': args.levels = value(); args.levelsProvided = true; break;
      case '--campaign-file': args.campaignFile = resolve(value()); break;
      case '--feature-catalog-sha256': args.featureCatalogSha256 = value(); break;
      case '--dependency-policy-sha256': args.dependencyPolicySha256 = value(); break;
      case '--campaign-sha256': args.campaignSha256 = value(); break;
      case '--campaign-attempt-id': args.campaignAttemptId = value(); break;
      case '--campaign-admission-id': args.campaignAdmissionId = value(); break;
      case '--progression-resume-from': args.progressionResumeFrom = resolve(value()); break;
      case '--recipe': args.recipe = value(); break;
      case '--pack': args.packIds.push(...value().split(',').filter(Boolean)); break;
      case '--check': args.checkKeys.push(...value().split(',').filter(Boolean)); break;
      case '--model': args.model = value(); break;
      case '--pricing-json': args.pricing = JSON.parse(value()); break;
      case '--fix-rounds': args.fixRounds = Number(value()); break;
      case '--max-stalled-repairs': args.maxStalledRepairs = Number(value()); break;
      case '--max-budget-usd': args.maxBudgetUsd = Number(value()); break;
      case '--run-index': args.runIndex = parseInt(value(), 10); break;
      case '--out': args.out = value(); break;
      case '--app': args.app = value(); break;
      case '--url': args.url = value(); break;
      case '--agent-adapter': args.agentAdapter = value(); break;
      case '--no-media': args.media = false; break;
      case '--retain-backend': args.retainBackend = true; break;
      case '--guidance': args.guidance = normalizeGuidance(value()); break;
      case '--guidance-document-json': args.guidanceDocument = JSON.parse(value()); break;
      case '--condition-json': args.condition = JSON.parse(value()); break;
      case '--selection-json': args.selectionRequest = JSON.parse(value()); break;
      case '--task-mode': args.taskMode = value(); break;
      case '--feature-module': args.featureIds.push(...value().split(',').filter(Boolean)); break;
      case '--request-spec': args.requestedSpecifications.push(...value().split(',').filter(Boolean)); break;
      case '--expect-spec': args.expectedSpecifications.push(...value().split(',').filter(Boolean)); break;
      case '--observe-spec': args.observedSpecifications.push(...value().split(',').filter(Boolean)); break;
      case '--skip-probe': args.skipProbe = true; break;
      case '--skills': args.skills = value().split(',').filter(Boolean); break;
      case '--skills-json': args.skills = JSON.parse(value()); break;
      case '--api-key': args.apiKey = value(); break;
      case '--api-key-file': args.apiKeyFile = resolve(value()); break;
      case '--mutations': args.mutations = resolve(value()); break;
      case '--mutation-shard-index': args.mutationShardIndex = Number(value()); break;
      case '--mutation-shard-count': args.mutationShardCount = Number(value()); break;
      case '--mutation-resume-from': args.mutationResumeFrom = resolve(value()); break;
      case '--mutation-checkpoint-out': args.mutationCheckpointOut = resolve(value()); break;
      case '--mutation-baseline-bundle': args.mutationBaselineBundle = resolve(value()); break;
      case '--expected-mutation-calibration-json':
        args.expectedMutationCalibration = JSON.parse(value()); break;
      case '--mutation-max-runtime-minutes': args.mutationMaxRuntimeMinutes = Number(value()); break;
      case '--reference-mutation-only': args.referenceMutationOnly = true; break;
      case '--seed-from': args.seedFrom = value(); break;
      case '--parent-attempt-id': args.parentAttemptId = value(); break;
      case '--repair-from': args.repairFrom = resolve(value()); break;
      case '--repair-level': args.repairLevel = Number(value()); break;
      default: console.error(`Unknown argument: ${option}`); process.exit(2);
    }
  }
  if (!args.backend && !args.repairFrom) {
    console.error('Usage: npm run bench -- --backend <stack> --levels 1-3 [--fix-rounds 10] [--run-index N]');
    process.exit(2);
  }
  if ((args.mutationResumeFrom || args.mutationCheckpointOut || args.mutationBaselineBundle)
      && !args.mutations) {
    throw new Error('mutation control options require --mutations');
  }
  if (args.expectedMutationCalibration && !args.mutations) {
    throw new Error('--expected-mutation-calibration-json requires --mutations');
  }
  if (!Number.isFinite(args.mutationMaxRuntimeMinutes) || args.mutationMaxRuntimeMinutes < 1
      || args.mutationMaxRuntimeMinutes > 120) {
    throw new Error('--mutation-max-runtime-minutes must be from 1 through 120');
  }
  if (args.referenceMutationOnly && (!args.mutations || args.agentAdapter !== 'reference-fixture'
      || args.fixRounds !== 0 || !args.app || args.campaignFile)) {
    throw new Error('--reference-mutation-only requires a mutation-bound reference fixture run');
  }
  if (args.mutationBaselineBundle && !args.referenceMutationOnly) {
    throw new Error('--mutation-baseline-bundle is an internal reference mutation option');
  }
  if (args.repairFrom && (args.repairLevel === undefined
      || !Number.isSafeInteger(args.repairLevel) || args.repairLevel < 1)) {
    throw new Error('--repair-from requires --repair-level with a positive integer');
  }
  if (args.campaignFile && (!args.campaignSha256 || !args.campaignAttemptId)) {
    throw new Error('--campaign-file requires --campaign-sha256 and --campaign-attempt-id');
  }
  if (!args.campaignFile && (args.campaignSha256 || args.campaignAttemptId
    || args.campaignAdmissionId || args.featureCatalogSha256 || args.dependencyPolicySha256)) {
    throw new Error('campaign binding requires --campaign-file');
  }
  if (args.progressionResumeFrom && !args.campaignFile) {
    throw new Error('--progression-resume-from requires a compiled campaign');
  }
  if (args.campaignFile) bindCampaign(args);
  if (args.progression) {
    if (args.levelsProvided) throw new Error('--levels cannot be combined with progression input');
    args.progression = validateProgressionInput(args.progression);
    args.levelList = progressionLevels(args.progression);
    args.levels = `${args.levelList[0]}-${args.levelList.at(-1)}`;
  } else {
    const [fromText, toText] = args.levels.split('-');
    const from = Number(fromText);
    const to = toText === undefined ? from : Number(toText);
    if (!Number.isSafeInteger(from) || from < 1 || !Number.isSafeInteger(to) || to < from) {
      throw new Error('--levels must be one positive level or an ascending range');
    }
    args.levelList = Array.from({ length: (to ?? from) - from + 1 }, (_, index) => from + index);
  }
  if (args.recipe && args.levelList.length !== 1) {
    throw new Error('--recipe requires exactly one requested level');
  }
  if (!Number.isInteger(args.fixRounds) || args.fixRounds < 0 || args.fixRounds > 20) {
    throw new Error('--fix-rounds must be an integer from 0 through 20');
  }
  if (!Number.isInteger(args.maxStalledRepairs) || args.maxStalledRepairs < 0
    || args.maxStalledRepairs > 20) {
    throw new Error('--max-stalled-repairs must be an integer from 0 through 20');
  }
  if (args.maxBudgetUsd !== undefined
    && (!Number.isFinite(args.maxBudgetUsd) || args.maxBudgetUsd <= 0)) {
    throw new Error('--max-budget-usd must be a positive number');
  }
  if ((args.mutationShardIndex === undefined) !== (args.mutationShardCount === undefined)) {
    throw new Error('--mutation-shard-index and --mutation-shard-count must be supplied together');
  }
  return args;
}

function bindCampaign(args: BenchArguments): void {
  if (!args.campaignFile) throw new Error('campaign file is required');
  const unsupported = [
    [args.maxStalledRepairs !== 3, '--max-stalled-repairs'],
    [args.skipProbe === true, '--skip-probe'],
    [args.mutations !== undefined || args.mutationShardIndex !== undefined
      || args.mutationShardCount !== undefined, '--mutations'],
    [args.seedFrom !== undefined, '--seed-from'],
    [args.repairFrom !== undefined || args.repairLevel !== undefined, '--repair-from'],
    [args.app !== undefined, '--app'], [args.url !== undefined, '--url'],
    [args.retainBackend === true, '--retain-backend'],
    [args.apiKey !== undefined || args.apiKeyFile !== undefined, 'credential override'],
    [args.recipe !== undefined || args.packIds.length > 0 || args.checkKeys.length > 0
      || args.featureIds.length > 0 || args.requestedSpecifications.length > 0
      || args.expectedSpecifications.length > 0 || args.observedSpecifications.length > 0,
    'direct recipe selection'],
  ].filter(([changed]) => changed).map(([, name]) => name);
  if (unsupported.length) {
    throw new Error(`campaign progression input cannot override ${unsupported.join(', ')}`);
  }
  const artifact = readArtifact(args.campaignFile, { expectedKind: 'campaign_plan' });
  const plan = validateCompiledCampaignPlan(artifact.payload);
  if (plan.contentSha256 !== args.campaignSha256) {
    throw new Error('--campaign-sha256 does not match the compiled campaign plan');
  }
  const attempt = plan.attempts.find(item => item.id === args.campaignAttemptId);
  if (attempt) {
    args.condition ??= structuredClone(attempt.condition);
    args.skills ??= structuredClone(attempt.skills);
    args.selectionRequest ??= structuredClone(plan.definition.selection);
    args.guidanceDocument ??= structuredClone(
      attempt.condition?.guidance?.documents?.[attempt.stack]);
  }
  if (!attempt || attempt.stack !== args.backend || attempt.agentAdapter !== args.agentAdapter
    || attempt.model !== args.model
    || canonicalDefinitionJson(attempt.pricing) !== canonicalDefinitionJson(args.pricing)
    || plan.definition.track !== args.track || attempt.guidance !== args.guidance
    || canonicalDefinitionJson(attempt.condition) !== canonicalDefinitionJson(args.condition)
    || canonicalDefinitionJson(attempt.skills) !== canonicalDefinitionJson(args.skills)
    || canonicalDefinitionJson(plan.definition.selection)
      !== canonicalDefinitionJson(args.selectionRequest)
    || canonicalDefinitionJson(attempt.condition?.guidance?.documents?.[attempt.stack])
      !== canonicalDefinitionJson(args.guidanceDocument)
    || plan.definition.budgets.fixRounds !== args.fixRounds
    || plan.definition.budgets.maxCostUsdPerAttempt !== (args.maxBudgetUsd ?? null)
    || args.parentAttemptId !== attempt.id || args.media !== false) {
    throw new Error('--campaign-attempt-id does not match the requested campaign attempt');
  }
  args.experimentIdentity = {
    id: plan.id, version: plan.version, sha256: plan.contentSha256, state: plan.state,
  };
  if (args.campaignAdmissionId) {
    const admission = readCampaignAdmission(dirname(args.campaignFile),
      args.campaignAdmissionId, plan);
    const image = plan.definition.runtime.buildImage
      ?? process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE;
    args.campaignAdmission = {
      id: args.campaignAdmissionId,
      ...campaignAdmissionSmokeReuse(admission, {
        agentAdapter: args.agentAdapter,
        runIndex: args.runIndex,
        backend: args.backend,
        image,
      }),
    };
  }
  args.runMode = structuredClone(attempt.mode);
  if (plan.featureCatalog) {
    args.featureCatalog = validateFeatureCatalogInput(plan.featureCatalog);
    if (args.featureCatalog.identity.sha256 !== args.featureCatalogSha256
      || canonicalDefinitionJson(attempt.featureCatalog)
        !== canonicalDefinitionJson(args.featureCatalog.identity)) {
      throw new Error('--feature-catalog-sha256 does not match the compiled campaign plan');
    }
  } else if (args.featureCatalogSha256 !== undefined || attempt.featureCatalog !== undefined) {
    throw new Error('campaign attempt has an unexpected feature catalog');
  }
  if (attempt.mode.id === 'dependency') {
    if (!plan.dependencyPolicy || !args.featureCatalog) {
      throw new Error('dependency campaign requires a feature catalog and dependency policy');
    }
    if (plan.dependencyPolicy.identity.sha256 !== args.dependencyPolicySha256
      || canonicalDefinitionJson(attempt.dependencyPolicy)
        !== canonicalDefinitionJson(plan.dependencyPolicy.identity)) {
      throw new Error('--dependency-policy-sha256 does not match the compiled campaign plan');
    }
    args.dependencyPolicy = plan.dependencyPolicy;
    args.progression = compileProgressionInput(dependencyRuntimeDefinition(
      args.featureCatalog, args.dependencyPolicy));
    args.progressionOwner = { schemaVersion: 1,
      campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
      attempt: { id: attempt.id, track: plan.definition.track, stack: attempt.stack,
        agentAdapter: attempt.agentAdapter, model: attempt.model,
        conditionSha256: attempt.condition.sha256 } };
  }
}
