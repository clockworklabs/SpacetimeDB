import { dirname, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { readArtifact } from '../src/evidence/artifacts.js';
import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.js';
import { DEFAULT_TRACK, RUN_INDEX_CAP } from '../src/composition/tracks.js';
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
import { validatePricingAuthority } from '../src/evidence/pricing-authority.js';
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

const stringListJson = (value: string): string[] => {
  const parsed: unknown = JSON.parse(value);
  if (!Array.isArray(parsed) || !parsed.every(item => typeof item === 'string')) {
    throw new Error('--skills-json must be an array of strings');
  }
  return parsed;
};

interface BenchCliOptions extends Partial<BenchArguments> {
  pack?: string[];
  check?: string[];
  pricingJson?: unknown;
  guidanceDocumentJson?: unknown;
  conditionJson?: StudyCondition;
  selectionJson?: CampaignSelection;
  featureModule?: string[];
  requestSpec?: string[];
  expectSpec?: string[];
  observeSpec?: string[];
  skillsJson?: string[];
  expectedMutationCalibrationJson?: unknown;
}

function parseCli(argv: readonly string[]): BenchCliOptions {
  const strings = ['backend', 'track', 'levels', 'campaign-file', 'campaign-attempt-id',
    'campaign-admission-id', 'progression-resume-from', 'recipe', 'model', 'pricing-json',
    'fix-rounds', 'max-stalled-repairs', 'max-budget-usd', 'run-index', 'out', 'app', 'url',
    'agent-adapter', 'guidance', 'guidance-document-json', 'condition-json', 'selection-json',
    'task-mode', 'skills', 'skills-json', 'api-key', 'api-key-file', 'mutations',
    'mutation-shard-index', 'mutation-shard-count', 'mutation-resume-from',
    'mutation-checkpoint-out', 'mutation-baseline-bundle',
    'expected-mutation-calibration-json', 'mutation-max-runtime-minutes', 'seed-from',
    'parent-attempt-id', 'repair-from', 'repair-level'] as const;
  const multiple = ['pack', 'check', 'feature-module', 'request-spec', 'expect-spec',
    'observe-spec'] as const;
  const options = Object.fromEntries([
    ...strings.map(name => [name, { type: 'string' as const }]),
    ...multiple.map(name => [name, { type: 'string' as const, multiple: true }]),
    ...['no-media', 'retain-backend', 'reference-mutation-only'].map(name =>
      [name, { type: 'boolean' as const }]),
  ]);
  const { values } = parseArgs({ args: [...argv.slice(2)], options, strict: true,
    allowPositionals: false });
  const parsed: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(values)) {
    parsed[key.replace(/-([a-z])/g, (_, letter: string) => letter.toUpperCase())] = value;
  }
  for (const key of ['pack', 'check', 'featureModule', 'requestSpec', 'expectSpec',
    'observeSpec']) {
    const value = parsed[key] as string[] | undefined;
    if (value) parsed[key] = value.flatMap(item => item.split(',').filter(Boolean));
  }
  for (const key of ['fixRounds', 'maxStalledRepairs', 'maxBudgetUsd', 'mutationShardIndex',
    'mutationShardCount', 'mutationMaxRuntimeMinutes', 'repairLevel']) {
    if (typeof parsed[key] === 'string') parsed[key] = Number(parsed[key]);
  }
  if (typeof parsed.runIndex === 'string') parsed.runIndex = Number(parsed.runIndex);
  for (const key of ['campaignFile', 'progressionResumeFrom', 'apiKeyFile', 'mutations',
    'mutationResumeFrom', 'mutationCheckpointOut', 'mutationBaselineBundle', 'repairFrom']) {
    if (typeof parsed[key] === 'string') parsed[key] = resolve(parsed[key]);
  }
  for (const key of ['pricingJson', 'guidanceDocumentJson', 'conditionJson', 'selectionJson',
    'expectedMutationCalibrationJson']) {
    if (typeof parsed[key] === 'string') parsed[key] = JSON.parse(parsed[key]);
  }
  if (typeof parsed.guidance === 'string') parsed.guidance = normalizeGuidance(parsed.guidance);
  if (parsed.skills !== undefined && parsed.skillsJson !== undefined) {
    throw new Error('--skills and --skills-json cannot be used together');
  }
  if (typeof parsed.skills === 'string') parsed.skills = parsed.skills.split(',').filter(Boolean);
  if (typeof parsed.skillsJson === 'string') parsed.skillsJson = stringListJson(parsed.skillsJson);
  if (parsed.noMedia === true) parsed.media = false;
  delete parsed.noMedia;
  return parsed as BenchCliOptions;
}

export function parseBenchArguments(argv: readonly string[]): BenchArguments {
  const args: BenchArguments = { model: null, agentAdapter: 'claude-code',
    fixRounds: 10, runIndex: 0, levels: '1', levelsProvided: false, media: true,
    levelList: [], maxStalledRepairs: 3, guidance: 'prescribed', track: DEFAULT_TRACK,
    packIds: [], checkKeys: [], featureIds: [], requestedSpecifications: [],
    expectedSpecifications: [], observedSpecifications: [],
    mutationMaxRuntimeMinutes: 60 };
  const { pack, check, pricingJson, guidanceDocumentJson, conditionJson, selectionJson,
    featureModule, requestSpec, expectSpec, observeSpec, skillsJson,
    expectedMutationCalibrationJson, ...options } = parseCli(argv);
  Object.assign(args, options);
  if (pack) args.packIds = pack;
  if (check) args.checkKeys = check;
  if (pricingJson !== undefined) {
    args.pricing = validatePricingAuthority(pricingJson, { at: '--pricing-json' });
  }
  if (guidanceDocumentJson !== undefined) args.guidanceDocument = guidanceDocumentJson;
  if (conditionJson !== undefined) args.condition = conditionJson;
  if (selectionJson !== undefined) args.selectionRequest = selectionJson;
  if (featureModule) args.featureIds = featureModule;
  if (requestSpec) args.requestedSpecifications = requestSpec;
  if (expectSpec) args.expectedSpecifications = expectSpec;
  if (observeSpec) args.observedSpecifications = observeSpec;
  if (skillsJson !== undefined) args.skills = skillsJson;
  args.levelsProvided = options.levels !== undefined;
  if (expectedMutationCalibrationJson !== undefined) {
    args.expectedMutationCalibration = expectedMutationCalibrationJson;
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
  if (args.campaignFile && !args.campaignAttemptId) {
    throw new Error('--campaign-file requires --campaign-attempt-id');
  }
  if (!args.campaignFile && (args.campaignAttemptId || args.campaignAdmissionId)) {
    throw new Error('campaign binding requires --campaign-file');
  }
  if (args.progressionResumeFrom && !args.campaignFile) {
    throw new Error('--progression-resume-from requires a compiled campaign');
  }
  if (args.campaignFile) {
    const allowed = new Set(['--campaign-file', '--campaign-attempt-id',
      '--campaign-admission-id', '--progression-resume-from', '--run-index', '--out',
      '--max-budget-usd']);
    const override = argv.slice(2).find(value => value.startsWith('--')
      && !allowed.has(value.split('=', 1)[0]!));
    if (override) throw new Error(`campaign attempts cannot override ${override}`);
    bindCampaign(args);
  }
  if (!args.backend && !args.repairFrom) {
    throw new Error('--backend is required unless --repair-from or --campaign-file is supplied');
  }
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
  if (!Number.isSafeInteger(args.runIndex) || args.runIndex < 0 || args.runIndex > RUN_INDEX_CAP) {
    throw new Error(`--run-index must be an integer from 0 through ${RUN_INDEX_CAP}`);
  }
  if ((args.mutationShardIndex === undefined) !== (args.mutationShardCount === undefined)) {
    throw new Error('--mutation-shard-index and --mutation-shard-count must be supplied together');
  }
  return args;
}

function bindCampaign(args: BenchArguments): void {
  if (!args.campaignFile) throw new Error('campaign file is required');
  const artifact = readArtifact(args.campaignFile, { expectedKind: 'campaign_plan' });
  const plan = validateCompiledCampaignPlan(artifact.payload);
  const attempt = plan.attempts.find(item => item.id === args.campaignAttemptId);
  if (!attempt) throw new Error('--campaign-attempt-id is not in the compiled campaign plan');
  const plannedBudget = plan.definition.budgets.maxCostUsdPerAttempt;
  if (args.maxBudgetUsd !== undefined
    && (plannedBudget === null || args.maxBudgetUsd > plannedBudget)) {
    throw new Error('--max-budget-usd exceeds the compiled campaign budget');
  }
  args.backend = attempt.stack;
  args.track = plan.definition.track;
  args.model = attempt.model;
  args.agentAdapter = attempt.agentAdapter;
  args.pricing = validatePricingAuthority(attempt.pricing, { at: 'compiled campaign pricing' });
  args.guidance = attempt.guidance;
  args.condition = structuredClone(attempt.condition);
  args.skills = structuredClone(attempt.skills);
  args.selectionRequest = structuredClone(plan.definition.selection);
  args.guidanceDocument = structuredClone(
    attempt.condition.guidance.documents[attempt.stack]);
  args.packIds = structuredClone(plan.definition.selection.packs ?? []);
  args.checkKeys = structuredClone(plan.definition.selection.checks ?? []);
  args.fixRounds = plan.definition.budgets.fixRounds;
  args.maxBudgetUsd ??= plannedBudget ?? undefined;
  args.parentAttemptId = attempt.id;
  args.media = false;
  args.levels = `${Math.min(...attempt.levels)}-${Math.max(...attempt.levels)}`;
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
  }
  if (attempt.mode.id === 'dependency') {
    if (!plan.dependencyPolicy || !args.featureCatalog) {
      throw new Error('dependency campaign requires a feature catalog and dependency policy');
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
