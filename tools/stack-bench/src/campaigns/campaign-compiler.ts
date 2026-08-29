import { readFileSync, realpathSync } from 'node:fs';
import { isAbsolute, join, relative, resolve } from 'node:path';

import { agentAdapterIdentity, AGENT_ADAPTER_REGISTRY } from '../agents/agent-adapters.mjs';
import { resolveCalibrationForRelease } from '../composition/calibration-compiler.mjs';
import { canonicalDefinitionJson, canonicalizeDefinition } from '../composition/definition-plan.mjs';
import { currentEngineIdentity } from '../evidence/artifacts.mjs';
import { sha256 } from '../evidence/provenance.js';
import { PRICING_RATE_FIELDS, PRICING_UNIT, validatePricingRates }
  from '../evidence/pricing-authority.js';
import type { PricingRates } from '../evidence/pricing-authority.js';
import { recipeReleaseIdentity, resolveRecipeRelease } from '../composition/recipe-release.mjs';
import { createBoundRecipeTaskRequest, createRecipeTaskRequest } from '../composition/recipe-selection.mjs';
import type { RecipeTaskRequestResult } from '../composition/recipe-selection.mjs';
import type { RecipeBinding } from '../composition/recipe-release.mjs';
import { compileDependencyPolicyInput, compileFeatureCatalogInput, progressionLevels,
  selectFeatureCatalogLevels, validateDependencyPolicyInput, validateFeatureCatalogInput }
  from '../progression/progression-definition.js';
import type { CompiledDependencyPolicyDefinition, CompiledProgressionDefinition,
  ProgressionInput } from '../progression/progression-definition.js';
import { resolveProgressionRecipeLevelSelection, validateProgressionRecipeBindings }
  from '../progression/progression-recipe-selection.js';
import type { ProgressionRecipeSelections as ProgressionLevelSelection }
  from '../progression/progression-recipe-selection.js';
import { resolveFeatureCatalog } from '../progression/feature-catalog-selection.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.mjs';
import type { StackAdapter } from '../stacks/stack-adapter-contract.mjs';
import { listTracks, loadTrack, RUN_INDEX_CAP } from '../composition/tracks.mjs';
import type { ConditionReference, ResolvedStudyCondition } from './condition-compiler.js';
import { resolveStudyConditions, validateConditionReference } from './condition-compiler.js';
import type { CampaignModeInput } from './campaign-mode.js';
import { validateCampaignMode } from './campaign-mode.js';

type UnknownRecord = Record<string, unknown>;
type CampaignState = 'draft' | 'frozen';
type NonEmpty<T> = [T, ...T[]];
type FeatureCatalogInput = ProgressionInput<CompiledProgressionDefinition>;
type DependencyPolicyInput = ProgressionInput<CompiledDependencyPolicyDefinition>;

interface CampaignMode extends CampaignModeInput {
  strikes?: UnknownRecord;
}

export interface CampaignStackSelection {
  id: string;
  adapterVersion: string;
  repetitions?: number;
}

export interface CampaignAgentSelection {
  adapter: string;
  adapterVersion: string;
  model: string;
}

export interface CampaignLevelSelection {
  level: number;
  recipe: string;
  features?: string[];
  checks?: string[];
}

export interface CampaignSelection {
  packs?: string[];
  checks?: string[];
  levels?: CampaignLevelSelection[];
}

export interface CampaignDefinition {
  schemaVersion: number;
  kind: 'campaign-manifest';
  id: string;
  version: string;
  state: CampaignState;
  title: string;
  track: string;
  mode: CampaignMode;
  levels: number[];
  selection: CampaignSelection;
  stacks: CampaignStackSelection[];
  agents: CampaignAgentSelection[];
  conditions: ConditionReference[];
  repetitions: number;
  ordering: { method: 'balanced-rotation'; seed: string };
  parallelism: number;
  budgets: {
    fixRounds: number;
    attemptTimeoutMinutes: number;
    maxCostUsdPerAttempt: number | null;
  };
  attemptPolicy: {
    retries: number;
    retryOn: string[];
    excludeFromAnalysis: string[];
  };
  pricing: {
    currency: 'USD';
    unit: string;
    capturedAt: string;
    source: string;
    models: Record<string, PricingRates>;
  };
  analysis: {
    primaryMetric: string;
    secondaryMetrics: string[];
    dispersion: string;
    invalidAttempts: 'report-separately';
    missingData: 'no-imputation';
    comparisonUnit: 'stack-agent-condition-recipe';
  };
  runtime: {
    releaseManifestSha256: string | null;
    controllerImage: string | null;
    buildImage: string | null;
    platform: 'linux/amd64';
  };
  featureCatalog?: string | FeatureCatalogInput['definition'];
}

interface Identity {
  id: string;
  version: string;
  sha256: string;
}

interface RecipeIdentity extends UnknownRecord {
  id: string;
  version: string;
  state: string;
  contentSha256: string;
  meaningSha256: string;
  executionSha256: string;
}

interface PublicBinding extends UnknownRecord {
  level: number;
  promotion: { alias: string; status: string; catalog?: unknown };
  recipe: RecipeIdentity;
  fixture: UnknownRecord & { state: string };
  calibration: (Identity & { state: string; buildImage: string | null }) | null;
  qualifiedStacks: Array<{ id: string; status: string }>;
  selection: {
    sha256: string;
    completeness: string;
    scoredPoints: number;
    taskPacks: string[];
    requested: unknown;
  } | null;
  task: {
    sha256: string;
    requirementSha256: string;
    contractSha256: string;
    requirementIds: string[];
    contractIds: string[];
  } | null;
}

interface BindingRecord {
  level: number;
  modular: CampaignLevelSelection | null;
  binding: RecipeBinding;
  calibration: ResolvedCalibration | null;
  publicBinding: PublicBinding;
  qualificationStaleness: unknown[];
}

interface ResolvedCalibration {
  id: string;
  version: string;
  state: string;
  contentSha256: string;
  qualification: {
    buildImage?: string;
    stacks: Array<{ id: string; status: string }>;
    checks?: string[];
    featureCatalog?: { id: string; version: string; sha256: string };
  };
  qualificationStaleness: unknown[];
}

interface ResolvedStackIdentity extends UnknownRecord {
  id: string;
  version: string;
  sha256: string | null;
  state: string | null;
}

interface ResolvedAgent extends CampaignAgentSelection, UnknownRecord {
  costLimit: string;
  identity: UnknownRecord & { id?: string; version?: string };
}

interface CampaignResolvedInputs {
  bindings: PublicBinding[];
  stacks: ResolvedStackIdentity[];
  agents: ResolvedAgent[];
  conditions: ResolvedStudyCondition[];
  featureCatalog: FeatureCatalogInput | null;
  dependencyPolicy: DependencyPolicyInput | null;
}

export interface CampaignAttemptPlan extends UnknownRecord {
  id: string;
  stack: string;
  model: string;
  guidance: string;
  repetition: number;
  order: number;
  levels: number[];
  agentAdapter: string;
  skills: string[];
  pricing: { unit: string; rates: PricingRates };
  mode: CampaignMode;
  condition: ResolvedStudyCondition;
  featureCatalog?: Identity;
  dependencyPolicy?: Identity;
  parentAttemptId: string;
}

export interface CompiledCampaignPlan {
  campaignSchemaVersion: number;
  id: string;
  version: string;
  title: string;
  state: CampaignState;
  source: { sha256: string };
  contentSha256: string;
  definition: CampaignDefinition;
  identities: { engine: Identity; [key: string]: unknown };
  bindings: PublicBinding[];
  stacks: ResolvedStackIdentity[];
  agents: Array<CampaignAgentSelection & {
    costLimit?: string;
    identity?: UnknownRecord & { id?: string; version?: string };
  }>;
  conditions: NonEmpty<ResolvedStudyCondition>;
  attempts: NonEmpty<CampaignAttemptPlan>;
  summary: {
    attempts: number;
    stacks: number;
    agents: number;
    conditions: number;
    repetitions: number;
    repetitionsByStack: Record<string, number>;
    parallelism: number;
  };
  featureCatalog: FeatureCatalogInput | null;
  dependencyPolicy: DependencyPolicyInput | null;
}

export type CalibrationResolver = (release: Parameters<typeof resolveCalibrationForRelease>[0], options: {
  trackRoot: string; stackBenchRoot: string;
}) => ResolvedCalibration | null;

export interface CompilerOptions {
  stackBenchRoot?: string;
  calibrationResolver?: CalibrationResolver;
}

interface ValidationOptions {
  requireCurrentInputs?: boolean;
  calibrationResolver?: CalibrationResolver;
}

export const CAMPAIGN_SCHEMA_VERSION = 5;
import { STACK_BENCH_ROOT as ROOT } from '../project-paths.mjs';
const HASH = /^[a-f0-9]{64}$/;
const ID = /^[a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*$/;
const VERSION = /^\d+\.\d+\.\d+$/;
const ROOT_FIELDS = new Set(['schemaVersion', 'kind', 'id', 'version', 'state', 'title',
  'track', 'mode', 'levels', 'selection', 'stacks', 'agents', 'conditions', 'repetitions', 'ordering',
  'parallelism', 'budgets', 'attemptPolicy', 'pricing', 'analysis', 'featureCatalog']);
ROOT_FIELDS.add('runtime');
const LEGACY_SELECTION_FIELDS = new Set(['packs', 'checks']);
const MODULAR_SELECTION_FIELDS = new Set(['levels']);
const MODULAR_LEVEL_FIELDS = new Set(['level', 'recipe', 'features', 'checks']);
const PROGRESSION_LEVEL_FIELDS = new Set(['level', 'recipe']);
const STACK_FIELDS = new Set(['id', 'adapterVersion', 'repetitions']);
const AGENT_FIELDS = new Set(['adapter', 'adapterVersion', 'model']);
const ORDERING_FIELDS = new Set(['method', 'seed']);
const BUDGET_FIELDS = new Set(['fixRounds', 'attemptTimeoutMinutes', 'maxCostUsdPerAttempt']);
const ATTEMPT_POLICY_FIELDS = new Set(['retries', 'retryOn', 'excludeFromAnalysis']);
const PRICING_FIELDS = new Set(['currency', 'unit', 'capturedAt', 'source', 'models']);
const PRICE_MODEL_FIELDS = new Set(PRICING_RATE_FIELDS);
const ANALYSIS_FIELDS = new Set(['primaryMetric', 'secondaryMetrics', 'dispersion',
  'invalidAttempts', 'missingData', 'comparisonUnit']);
const RUNTIME_FIELDS = new Set(['releaseManifestSha256', 'controllerImage', 'buildImage', 'platform']);
const RETRY_CAUSES = new Set(['provider_failure', 'harness_failure', 'inconclusive']);
const EXCLUSION_CAUSES = new Set(['provider_failure', 'harness_failure', 'inconclusive',
  'ungraded', 'contaminated']);
const OUTCOME_METRICS = new Set(['firstBuildScoreRate', 'finalScoreRate', 'totalCostUsd',
  'totalDurationMs', 'fixRounds', 'correctionSuccessRate', 'correctionCostUsd',
  'correctionSpendUsd', 'firstBuildCoverageRate', 'finalCoverageRate',
  'invalidAttemptRate']);
const DISPERSION = new Set(['median-iqr', 'mean-sd']);
const IMAGE_DIGEST = /^[^\s@]+@sha256:[a-f0-9]{64}$/;
const EXACT_REF = /^([a-z][a-z0-9]*(?:[._:-][a-z0-9]+)*)@(\d+\.\d+\.\d+)$/;
const ISO = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/;

const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
function fail(at: string, message: string): never {
  throw new Error(`invalid campaign at ${at}: ${message}`);
}

function strict<T>(value: T, at: string, fields: ReadonlySet<string>):
  asserts value is T & UnknownRecord {
  if (!object(value)) throw new Error(`invalid campaign at ${at}: must be an object`);
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`${at}.${key}`, 'is unknown');
}

function string(value: unknown, at: string): string {
  if (typeof value !== 'string' || !value.trim()) {
    throw new Error(`invalid campaign at ${at}: must be a non-empty string`);
  }
  return value;
}

function identifier(value: unknown, at: string): string {
  if (typeof value !== 'string' || !ID.test(value)) {
    throw new Error(`invalid campaign at ${at}: is invalid`);
  }
  return value;
}

function version(value: unknown, at: string): string {
  if (typeof value !== 'string' || !VERSION.test(value)) {
    throw new Error(`invalid campaign at ${at}: must be a semantic version`);
  }
  return value;
}

function integer(value: unknown, at: string, { min = 0, max = Number.MAX_SAFE_INTEGER }:
  { min?: number; max?: number } = {}): number {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < min || value > max) {
    throw new Error(`invalid campaign at ${at}: must be an integer from ${min} through ${max}`);
  }
  return value;
}

function finite(value: unknown, at: string, { min = 0 }: { min?: number } = {}): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < min) {
    throw new Error(`invalid campaign at ${at}: must be a number of at least ${min}`);
  }
  return value;
}

function exactArray<T>(value: unknown, at: string,
  validate: (value: unknown, at: string) => T,
  { nonEmpty = false, sort = false }: { nonEmpty?: boolean; sort?: boolean } = {}): T[] {
  if (!Array.isArray(value)) {
    throw new Error(`invalid campaign at ${at}: must be ${nonEmpty ? 'a non-empty' : 'an'} array`);
  }
  if (nonEmpty && value.length === 0) fail(at, 'must be a non-empty array');
  const normalized = value.map((item, index) => validate(item, `${at}[${index}]`));
  const keys = normalized.map(item => typeof item === 'string' ? item : canonicalDefinitionJson(item));
  if (new Set(keys).size !== keys.length) fail(at, 'must not contain duplicates');
  return sort ? normalized.sort((a, b) => canonicalDefinitionJson(a).localeCompare(canonicalDefinitionJson(b))) : normalized;
}

export function validateCampaignDefinition(input: unknown,
  { source = '<campaign>' }: { source?: string } = {}): CampaignDefinition {
  const cloned = structuredClone(input);
  strict(cloned, source, ROOT_FIELDS);
  const value = cloned as unknown as CampaignDefinition;
  if (value.schemaVersion !== CAMPAIGN_SCHEMA_VERSION) fail(`${source}.schemaVersion`, 'is unsupported');
  if (value.kind !== 'campaign-manifest') fail(`${source}.kind`, 'must be campaign-manifest');
  identifier(value.id, `${source}.id`);
  version(value.version, `${source}.version`);
  if (!['draft', 'frozen'].includes(value.state)) fail(`${source}.state`, 'must be draft or frozen');
  string(value.title, `${source}.title`);
  identifier(value.track, `${source}.track`);
  value.mode = validateCampaignMode(value.mode, { at: `${source}.mode` }) as CampaignMode;
  if (value.mode.id === 'dependency' && value.featureCatalog === undefined) {
    fail(`${source}.featureCatalog`, 'is required for dependency mode');
  }
  if (value.featureCatalog !== undefined) {
    if (typeof value.featureCatalog === 'string') {
      if (!EXACT_REF.test(value.featureCatalog)) {
        fail(`${source}.featureCatalog`, 'must be an exact id@version reference');
      }
      value.levels = exactArray(value.levels, `${source}.levels`,
        (level, at) => integer(level, at, { min: 1 }), { nonEmpty: true });
    } else {
      const featureCatalog = compileFeatureCatalogInput(value.featureCatalog);
      value.featureCatalog = featureCatalog.definition;
      const availableLevels = progressionLevels(featureCatalog);
      value.levels ??= availableLevels;
      value.levels = exactArray(value.levels, `${source}.levels`,
        (level, at) => integer(level, at, { min: 1 }), { nonEmpty: true });
      if (value.levels.some(level => !availableLevels.includes(level))) {
        fail(`${source}.levels`, 'must exist in feature catalog');
      }
      if (value.levels.at(0) !== availableLevels.at(0)) {
        fail(`${source}.levels`, 'must start at the first feature catalog level');
      }
    }
  } else {
    value.levels = exactArray(value.levels, `${source}.levels`,
      (level, at) => integer(level, at, { min: 1 }), { nonEmpty: true });
  }
  for (let index = 1; index < value.levels.length; index += 1) {
    if (value.levels[index] !== value.levels[index - 1]! + 1) {
      fail(`${source}.levels`, 'must be ascending and contiguous');
    }
  }

  const modularSelection = object(value.selection)
    && Object.hasOwn(value.selection, 'levels');
  if (value.featureCatalog && !modularSelection) {
    fail(`${source}.selection`, 'feature catalog campaigns require recipe bindings by level');
  }
  strict(value.selection, `${source}.selection`,
    modularSelection ? MODULAR_SELECTION_FIELDS : LEGACY_SELECTION_FIELDS);
  if (modularSelection) {
    value.selection.levels = exactArray(value.selection.levels, `${source}.selection.levels`,
      (entry, at): CampaignLevelSelection => {
        strict(entry, at, value.featureCatalog ? PROGRESSION_LEVEL_FIELDS : MODULAR_LEVEL_FIELDS);
        integer(entry.level, `${at}.level`, { min: 1 });
        if (typeof entry.recipe !== 'string'
          || !/^[a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*@\d+\.\d+\.\d+$/.test(entry.recipe)) {
          fail(`${at}.recipe`, 'must be an exact id@version reference');
        }
        if (!value.featureCatalog) {
          entry.features = exactArray(entry.features, `${at}.features`, string,
            { nonEmpty: true, sort: true });
          entry.checks = exactArray(entry.checks, `${at}.checks`, string, { sort: true });
        }
        return entry as unknown as CampaignLevelSelection;
      }, { nonEmpty: true });
    if (canonicalDefinitionJson(value.selection.levels.map(entry => entry.level))
      !== canonicalDefinitionJson(value.levels)) {
      fail(`${source}.selection.levels`, 'must bind every requested level once and in order');
    }
  } else {
    value.selection.packs = exactArray(value.selection.packs, `${source}.selection.packs`, string,
      { sort: true });
    value.selection.checks = exactArray(value.selection.checks, `${source}.selection.checks`, string,
      { sort: true });
  }

  value.stacks = exactArray(value.stacks, `${source}.stacks`, (stack, at): CampaignStackSelection => {
    strict(stack, at, STACK_FIELDS);
    identifier(stack.id, `${at}.id`);
    version(stack.adapterVersion, `${at}.adapterVersion`);
    if (stack.repetitions !== undefined) {
      integer(stack.repetitions, `${at}.repetitions`, { min: 1, max: 100 });
    }
    return stack as unknown as CampaignStackSelection;
  }, { nonEmpty: true, sort: true });
  if (new Set(value.stacks.map(stack => stack.id)).size !== value.stacks.length) {
    fail(`${source}.stacks`, 'must name each stack once');
  }
  value.agents = exactArray(value.agents, `${source}.agents`, (agent, at): CampaignAgentSelection => {
    strict(agent, at, AGENT_FIELDS);
    identifier(agent.adapter, `${at}.adapter`);
    version(agent.adapterVersion, `${at}.adapterVersion`);
    string(agent.model, `${at}.model`);
    return agent as unknown as CampaignAgentSelection;
  }, { nonEmpty: true, sort: true });
  const agentKeys = value.agents.map(agent => canonicalDefinitionJson(agent));
  if (new Set(agentKeys).size !== agentKeys.length) fail(`${source}.agents`, 'contains a duplicate configuration');
  value.conditions = exactArray(value.conditions, `${source}.conditions`, (condition, at) =>
    validateConditionReference(condition, at), { nonEmpty: true, sort: true });
  integer(value.repetitions, `${source}.repetitions`, { min: 1, max: 100 });
  value.parallelism ??= 1;
  integer(value.parallelism, `${source}.parallelism`, { min: 1, max: RUN_INDEX_CAP + 1 });
  for (const stack of value.stacks) stack.repetitions ??= value.repetitions;

  strict(value.ordering, `${source}.ordering`, ORDERING_FIELDS);
  if (value.ordering.method !== 'balanced-rotation') fail(`${source}.ordering.method`, 'must be balanced-rotation');
  string(value.ordering.seed, `${source}.ordering.seed`);

  strict(value.budgets, `${source}.budgets`, BUDGET_FIELDS);
  integer(value.budgets.fixRounds, `${source}.budgets.fixRounds`, { min: 0, max: 20 });
  integer(value.budgets.attemptTimeoutMinutes, `${source}.budgets.attemptTimeoutMinutes`, { min: 10, max: 480 });
  if (value.budgets.maxCostUsdPerAttempt !== null) {
    finite(value.budgets.maxCostUsdPerAttempt, `${source}.budgets.maxCostUsdPerAttempt`, { min: 0.01 });
  }

  strict(value.attemptPolicy, `${source}.attemptPolicy`, ATTEMPT_POLICY_FIELDS);
  integer(value.attemptPolicy.retries, `${source}.attemptPolicy.retries`, { min: 0, max: 10 });
  value.attemptPolicy.retryOn = exactArray(value.attemptPolicy.retryOn,
    `${source}.attemptPolicy.retryOn`, (item, at) => {
      if (typeof item !== 'string' || !RETRY_CAUSES.has(item)) fail(at, 'has an unsupported retry cause');
      return item;
    }, { sort: true });
  value.attemptPolicy.excludeFromAnalysis = exactArray(value.attemptPolicy.excludeFromAnalysis,
    `${source}.attemptPolicy.excludeFromAnalysis`, (item, at) => {
      if (typeof item !== 'string' || !EXCLUSION_CAUSES.has(item)) fail(at, 'has an unsupported exclusion cause');
      return item;
    }, { sort: true });
  if (value.attemptPolicy.retries > 0 && value.attemptPolicy.retryOn.length === 0) {
    fail(`${source}.attemptPolicy.retryOn`, 'must name a cause when retries are enabled');
  }

  strict(value.runtime, `${source}.runtime`, RUNTIME_FIELDS);
  for (const field of ['releaseManifestSha256', 'controllerImage', 'buildImage'] as const) {
    if (value.runtime[field] !== null && typeof value.runtime[field] !== 'string') {
      fail(`${source}.runtime.${field}`, 'must be a string or null');
    }
  }
  if (value.runtime.releaseManifestSha256 !== null && !HASH.test(value.runtime.releaseManifestSha256)) {
    fail(`${source}.runtime.releaseManifestSha256`, 'must be a SHA-256 digest or null');
  }
  for (const field of ['controllerImage', 'buildImage'] as const) {
    if (value.runtime[field] !== null && !IMAGE_DIGEST.test(value.runtime[field])) {
      fail(`${source}.runtime.${field}`, 'must be an exact image digest reference or null');
    }
  }
  if (value.runtime.platform !== 'linux/amd64') fail(`${source}.runtime.platform`, 'must be linux/amd64');

  strict(value.pricing, `${source}.pricing`, PRICING_FIELDS);
  if (value.pricing.currency !== 'USD') fail(`${source}.pricing.currency`, 'must be USD');
  if (value.pricing.unit !== PRICING_UNIT) {
    fail(`${source}.pricing.unit`, `must be ${PRICING_UNIT}`);
  }
  if (typeof value.pricing.capturedAt !== 'string' || !ISO.test(value.pricing.capturedAt)
    || Number.isNaN(Date.parse(value.pricing.capturedAt))) {
    fail(`${source}.pricing.capturedAt`, 'must be an ISO-8601 timestamp');
  }
  string(value.pricing.source, `${source}.pricing.source`);
  if (!object(value.pricing.models) || Object.keys(value.pricing.models).length === 0) {
    fail(`${source}.pricing.models`, 'must be a non-empty object');
  }
  for (const [model, rates] of Object.entries(value.pricing.models)) {
    string(model, `${source}.pricing.models key`);
    strict(rates, `${source}.pricing.models.${model}`, PRICE_MODEL_FIELDS);
    try {
      value.pricing.models[model] = validatePricingRates(rates,
        { at: `${source}.pricing.models.${model}` });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      fail(`${source}.pricing.models.${model}`, message
        .replace(`${source}.pricing.models.${model} `, ''));
    }
  }
  const selectedModels = new Set(value.agents.map(agent => agent.model));
  for (const model of selectedModels) {
    if (!Object.hasOwn(value.pricing.models, model)) fail(`${source}.pricing.models`, `has no rates for ${model}`);
  }

  strict(value.analysis, `${source}.analysis`, ANALYSIS_FIELDS);
  if (!OUTCOME_METRICS.has(value.analysis.primaryMetric)) fail(`${source}.analysis.primaryMetric`, 'is unsupported');
  value.analysis.secondaryMetrics = exactArray(value.analysis.secondaryMetrics,
    `${source}.analysis.secondaryMetrics`, (item, at) => {
      if (typeof item !== 'string' || !OUTCOME_METRICS.has(item)) fail(at, 'is unsupported');
      return item;
    });
  if (value.analysis.secondaryMetrics.includes(value.analysis.primaryMetric)) {
    fail(`${source}.analysis.secondaryMetrics`, 'must not repeat the primary metric');
  }
  if (!DISPERSION.has(value.analysis.dispersion)) fail(`${source}.analysis.dispersion`, 'is unsupported');
  if (value.analysis.invalidAttempts !== 'report-separately') {
    fail(`${source}.analysis.invalidAttempts`, 'must be report-separately');
  }
  if (value.analysis.missingData !== 'no-imputation') fail(`${source}.analysis.missingData`, 'must be no-imputation');
  if (value.analysis.comparisonUnit !== 'stack-agent-condition-recipe') {
    fail(`${source}.analysis.comparisonUnit`, 'must be stack-agent-condition-recipe');
  }
  if (value.state === 'frozen') {
    if (value.budgets.maxCostUsdPerAttempt === null) {
      fail(`${source}.budgets.maxCostUsdPerAttempt`, 'is required for a frozen campaign');
    }
    for (const field of ['controllerImage', 'buildImage'] as const) {
      if (value.runtime[field] === null) fail(`${source}.runtime.${field}`, 'is required for a frozen campaign');
    }
  }
  return canonicalizeDefinition(value) as unknown as CampaignDefinition;
}

function loadJson(path: string): unknown {
  try { return JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`cannot read campaign ${path}: ${message}`, { cause: error });
  }
}

function exactSource(path: string): string {
  if (!isAbsolute(path)) throw new Error('campaign path must be absolute after resolution');
  return realpathSync(path);
}

function rotate<T>(values: T[], offset: number): T[] {
  return [...values.slice(offset), ...values.slice(0, offset)];
}

function identityForStack(adapter: StackAdapter): ResolvedStackIdentity {
  return { id: adapter.id, version: adapter.version, sha256: null, state: null };
}

function calibrationIdentity(value: ResolvedCalibration | null): PublicBinding['calibration'] {
  return value ? { id: value.id, version: value.version, sha256: value.contentSha256,
    state: value.state, buildImage: value.qualification?.buildImage ?? null } : null;
}

function imageContentDigest(value: string | null): string | null {
  return value?.slice(value.lastIndexOf('@') + 1) ?? null;
}

function campaignIdentityDocument(definition: CampaignDefinition, engine: Identity,
  featureCatalog: FeatureCatalogInput | null, dependencyPolicy: DependencyPolicyInput | null,
  bindings: PublicBinding[], stacks: ResolvedStackIdentity[], agents: CompiledCampaignPlan['agents'],
  conditions: ResolvedStudyCondition[]): unknown {
  return canonicalizeDefinition({ campaignSchemaVersion: CAMPAIGN_SCHEMA_VERSION,
    definition, engine, ...(featureCatalog ? { featureCatalog } : {}),
    ...(dependencyPolicy ? { dependencyPolicy } : {}),
    bindings, stacks, agents, conditions });
}

function expandAttempts(definition: CampaignDefinition, requestedLevels: number[],
  featureCatalogIdentity: Identity | null, dependencyPolicyIdentity: Identity | null,
  stacks: ResolvedStackIdentity[], agents: CompiledCampaignPlan['agents'],
  studyConditions: ResolvedStudyCondition[]): CampaignAttemptPlan[] {
  const repetitionsByStack = new Map(definition.stacks.map(stack =>
    [stack.id, stack.repetitions ?? definition.repetitions]));
  const conditions = agents.flatMap((agent, agentIndex) => studyConditions.flatMap(
    (condition, conditionIndex) => stacks.map(stack => ({
      agent, agentIndex, condition, conditionIndex, stack,
      key: canonicalDefinitionJson({ agent: { adapter: agent.adapter, model: agent.model },
        condition: condition.sha256, stack: stack.id }),
    })))).sort((left, right) => {
    const leftHash = sha256(`${definition.ordering.seed}\0${left.key}`);
    const rightHash = sha256(`${definition.ordering.seed}\0${right.key}`);
    return leftHash.localeCompare(rightHash) || left.key.localeCompare(right.key);
  });
  const attempts: CampaignAttemptPlan[] = [];
  const repetitions = Math.max(...repetitionsByStack.values());
  for (let repetition = 1; repetition <= repetitions; repetition += 1) {
    rotate(conditions, (repetition - 1) % conditions.length)
      .filter(({ stack }) => repetition <= (repetitionsByStack.get(stack.id) ?? 0))
      .forEach(({ agent, agentIndex, condition, conditionIndex, stack }, order) => attempts.push({
        id: `${definition.id}-r${repetition}-c${conditionIndex + 1}-a${agentIndex + 1}-${stack.id}`,
        repetition,
        order: order + 1,
        stack: stack.id,
        agentAdapter: agent.adapter,
        model: agent.model,
        pricing: { unit: definition.pricing.unit,
          rates: definition.pricing.models[agent.model]! },
        condition: { id: condition.id, version: condition.version, sha256: condition.sha256,
          requested: condition.requested, guidance: condition.guidance,
          repair: condition.repair },
        guidance: condition.guidance.mode,
        skills: condition.guidance.skills[stack.id]!.ids,
        mode: { id: definition.mode.id, version: definition.mode.version },
        levels: requestedLevels,
        ...(featureCatalogIdentity ? { featureCatalog: featureCatalogIdentity } : {}),
        ...(dependencyPolicyIdentity ? { dependencyPolicy: dependencyPolicyIdentity } : {}),
        parentAttemptId: definition.id,
      }));
  }
  return attempts;
}

function resolveCampaignInputs(definition: CampaignDefinition, {
  stackBenchRoot = ROOT,
  calibrationResolver = resolveCalibrationForRelease,
}: CompilerOptions = {}): CampaignResolvedInputs {
  if (!listTracks({ includeInternal: true }).includes(definition.track)) {
    fail('track', `is unknown; available tracks: ${listTracks({ includeInternal: true }).join(', ')}`);
  }
  const track = loadTrack(definition.track);
  let resolvedFeatureCatalog = null;
  try {
    resolvedFeatureCatalog = definition.featureCatalog
      ? resolveFeatureCatalog(definition.featureCatalog, track) : null;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    fail('featureCatalog', message.replace(/^feature catalog /, ''));
  }
  if (resolvedFeatureCatalog
    && definition.levels.some(level => !progressionLevels(resolvedFeatureCatalog).includes(level))) {
    fail('levels', 'must exist in feature catalog');
  }
  if (resolvedFeatureCatalog
    && definition.levels[0] !== progressionLevels(resolvedFeatureCatalog)[0]) {
    fail('levels', 'must start at the first feature catalog level');
  }
  const featureCatalog = resolvedFeatureCatalog
    ? selectFeatureCatalogLevels(resolvedFeatureCatalog, definition.levels) : null;
  const modularLevels = new Map<number, CampaignLevelSelection>((definition.selection.levels ?? [])
    .map(selection => [selection.level, selection]));
  const bindingRecords: BindingRecord[] = definition.levels.map(level => {
    const modular = modularLevels.get(level) ?? null;
    const binding = resolveRecipeRelease(track, level, modular?.recipe ?? null);
    if (!binding) fail('levels', `L${level} has no recipe release`);
    const selectedTask = modular ? null : createRecipeTaskRequest(binding, {
      packIds: definition.selection.packs, checkKeys: definition.selection.checks });
    const calibration = calibrationResolver(binding.release, {
      trackRoot: track.dir, stackBenchRoot: resolve(stackBenchRoot),
    });
    const publicBinding: PublicBinding = {
      level,
      promotion: { alias: binding.alias, status: binding.status, catalog: binding.catalog },
      recipe: recipeReleaseIdentity(binding.release),
      fixture: binding.release.components.fixture,
      calibration: calibrationIdentity(calibration),
      qualifiedStacks: calibration?.qualification.stacks ?? [],
      selection: selectedTask?.selection ?? null,
      task: selectedTask ? {
        sha256: selectedTask.task.sha256,
        requirementSha256: selectedTask.task.requirementSha256,
        contractSha256: selectedTask.task.contractSha256,
        requirementIds: selectedTask.task.requirementIds,
        contractIds: selectedTask.task.contractIds,
      } : null,
    };
    return { level, modular, binding, calibration, publicBinding,
      qualificationStaleness: calibration?.qualificationStaleness ?? [] };
  });
  const bindings = bindingRecords.map(record => record.publicBinding);
  if (featureCatalog) {
    for (const record of bindingRecords) {
      const qualifiedCatalog = record.calibration?.qualification.featureCatalog;
      if (qualifiedCatalog && (qualifiedCatalog.id !== resolvedFeatureCatalog!.identity.id
        || qualifiedCatalog.version !== resolvedFeatureCatalog!.identity.version)) {
        fail('featureCatalog', `L${record.level} calibration qualifies `
          + `${qualifiedCatalog.id}@${qualifiedCatalog.version}, not `
          + `${resolvedFeatureCatalog!.identity.id}@${resolvedFeatureCatalog!.identity.version}`);
      }
    }
  }
  let progressionSelections: Map<number, ProgressionLevelSelection> | null = null;
  if (featureCatalog) {
    validateProgressionRecipeBindings(featureCatalog, bindingRecords, { levels: definition.levels });
    progressionSelections = new Map(bindingRecords.map(record =>
      [record.level, resolveProgressionRecipeLevelSelection(record.binding,
        featureCatalog, record.level, { cumulative: definition.mode.id === 'dependency' })]));
  }
  if (definition.state === 'frozen') {
    if (progressionSelections) {
      for (const record of bindingRecords) {
        const selected = progressionSelections.get(record.level)?.grader.checkKeys ?? [];
        const qualified = new Set(record.calibration?.qualification.checks
          ?? record.binding.release.checkCatalog.map(check => check.stableKey));
        const missing = selected.filter(check => !qualified.has(check));
        if (missing.length > 0) {
          fail('state', `cannot freeze L${record.level}: calibration does not cover `
            + `${missing.length} selected checks`);
        }
      }
    }
    for (const binding of bindings) {
      if (binding.recipe.state !== 'qualified' || binding.promotion.status !== 'promoted'
        || binding.fixture.state !== 'qualified' || binding.calibration?.state !== 'qualified') {
        fail('state', `cannot freeze with unqualified L${binding.level} recipe, fixture, calibration, or promotion`);
      }
      const supported = new Map(binding.qualifiedStacks.map(stack => [stack.id, stack.status]));
      for (const stack of definition.stacks) {
        if (supported.get(stack.id) !== 'qualified') {
          fail('state', `cannot freeze L${binding.level} for unqualified stack ${stack.id}`);
        }
      }
    }
    for (const record of bindingRecords) {
      const qualifiedImage = record.publicBinding.calibration?.buildImage;
      if (!qualifiedImage || imageContentDigest(definition.runtime.buildImage) !== qualifiedImage) {
        fail('state', `cannot freeze L${record.level}: runtime buildImage does not match the qualified build image`);
      }
    }
    const staleLevels = bindingRecords.filter(record => record.qualificationStaleness.length > 0);
    if (staleLevels.length > 0) {
      const summary = staleLevels.map(record =>
        `L${record.level} (${record.qualificationStaleness.length} artifacts)`).join(', ');
      fail('state', `cannot freeze with stale qualification evidence for ${summary}; regenerate the qualification evidence`);
    }
  }

  const stacks = definition.stacks.map(selection => {
    const adapter = STACK_ADAPTER_REGISTRY.get(selection.id);
    if (!adapter) fail('stacks', `has unknown adapter ${selection.id}`);
    if (adapter.version !== selection.adapterVersion) fail('stacks', `${adapter.id} adapter is ${adapter.version}, not ${selection.adapterVersion}`);
    return identityForStack(adapter);
  });
  const agents = definition.agents.map(selection => {
    const adapter = AGENT_ADAPTER_REGISTRY.get(selection.adapter);
    if (!adapter) fail('agents', `has unknown adapter ${selection.adapter}`);
    if (adapter.version !== selection.adapterVersion) fail('agents', `${adapter.id} adapter is ${adapter.version}, not ${selection.adapterVersion}`);
    return { ...selection, costLimit: adapter.costLimit, identity: agentAdapterIdentity(adapter) };
  });
  if (definition.state === 'frozen' && agents.some(agent => agent.costLimit === 'unsupported')) {
    fail('state', 'cannot freeze an agent adapter that does not enforce maxCostUsdPerAttempt');
  }
  const legacyRequested = modularLevels.size ? null : { track: definition.track, levels: bindings.map(binding => ({
    level: binding.level,
    recipe: {
      id: binding.recipe.id, version: binding.recipe.version,
      contentSha256: binding.recipe.contentSha256,
      meaningSha256: binding.recipe.meaningSha256,
      executionSha256: binding.recipe.executionSha256,
      state: binding.recipe.state,
    },
      selection: {
      sha256: binding.selection!.sha256,
      completeness: binding.selection!.completeness,
      scoredPoints: binding.selection!.scoredPoints,
      taskPacks: binding.selection!.taskPacks,
      requested: binding.selection!.requested,
    },
    task: binding.task!,
  })) };
  const requestedForCondition = (ref: ConditionReference): unknown => {
    if (!modularLevels.size) {
      if (ref.specifications !== undefined) {
        fail('conditions', 'legacy selection cannot declare modular specifications');
      }
      return legacyRequested;
    }
    if (progressionSelections && ref.specifications !== undefined) {
      fail('conditions', 'progression graph owns specification scope');
    }
    if (!progressionSelections && !ref.specifications) {
      fail('conditions', 'modular selection requires specifications by level');
    }
    const declared = ref.specifications;
    const specifications: Map<number, {
      level: number; requested: string[]; expected: string[]; observed: string[];
    }> | null = progressionSelections ? null
      : new Map(declared!.levels.map(entry => [entry.level, entry]));
    if (specifications && (specifications.size !== bindingRecords.length
      || bindingRecords.some(record => !specifications.has(record.level)))) {
      fail('conditions', 'specifications must bind every selected level exactly once');
    }
    return { track: definition.track, levels: bindingRecords.map(record => {
      const declaredSpecifications = specifications?.get(record.level) ?? null;
      const selected: RecipeTaskRequestResult = progressionSelections?.get(record.level)?.grader
        ?? createBoundRecipeTaskRequest(record.binding, {
          featureIds: record.modular!.features,
          checkKeys: record.modular!.checks,
          requestedSpecifications: declaredSpecifications!.requested,
          expectedSpecifications: declaredSpecifications!.expected,
          observedSpecifications: declaredSpecifications!.observed,
        });
      return {
        level: record.level,
        recipe: {
          id: record.publicBinding.recipe.id, version: record.publicBinding.recipe.version,
          contentSha256: record.publicBinding.recipe.contentSha256,
          meaningSha256: record.publicBinding.recipe.meaningSha256,
          executionSha256: record.publicBinding.recipe.executionSha256,
          state: record.publicBinding.recipe.state,
        },
        selection: {
          schemaVersion: 3,
          sha256: selected.selection.sha256,
          scoredPoints: selected.selection.scoredPoints,
          requested: selected.selection.requested,
          promptPacks: selected.selection.promptPacks,
          features: selected.selection.features,
          specifications: selected.selection.specifications,
          scoredChecks: selected.selection.scoredChecks.map(check => ({
            stableKey: check.stableKey, points: check.points, treatment: check.treatment,
          })),
          observedChecks: selected.selection.observedChecks.map(check => ({
            stableKey: check.stableKey, points: check.points, treatment: check.treatment,
          })),
        },
        task: {
          ...(selected.request.task.mode === undefined
            ? {} : { mode: selected.request.task.mode }),
          sha256: selected.task.sha256,
          requirementSha256: selected.task.requirementSha256,
          contractSha256: selected.task.contractSha256,
          requirementIds: selected.task.requirementIds,
          contractIds: selected.task.contractIds,
        },
      };
    }) };
  };
  const conditions = resolveStudyConditions(definition.conditions, stacks.map(stack => stack.id), {
    stackBenchRoot: resolve(stackBenchRoot), frozen: definition.state === 'frozen',
    requested: requestedForCondition,
  });
  const dependencyPolicy = definition.mode.id === 'dependency'
    ? compileDependencyPolicyInput(definition.mode.strikes, featureCatalog!,
      definition.levels) : null;
  return { bindings, stacks, agents, conditions, featureCatalog, dependencyPolicy };
}

export function compileCampaignFile(path: string, {
  stackBenchRoot = ROOT,
  calibrationResolver = resolveCalibrationForRelease,
}: CompilerOptions = {}): CompiledCampaignPlan {
  const absolute = exactSource(resolve(path));
  const sourceDefinition = validateCampaignDefinition(loadJson(absolute), {
    source: relative(process.cwd(), absolute).replaceAll('\\', '/'),
  });
  const requestedLevels = sourceDefinition.levels;
  const { bindings, stacks, agents, conditions, featureCatalog, dependencyPolicy } =
    resolveCampaignInputs(sourceDefinition, { stackBenchRoot, calibrationResolver });
  const resolvedDefinition: CampaignDefinition = featureCatalog
    ? { ...sourceDefinition, featureCatalog: featureCatalog.definition }
    : sourceDefinition;
  const definition = structuredClone(resolvedDefinition);
  if (featureCatalog) delete definition.featureCatalog;

  const sourceSha256 = sha256(readFileSync(absolute));
  const engine = currentEngineIdentity();
  const identityDocument = campaignIdentityDocument(definition, engine, featureCatalog,
    dependencyPolicy,
    bindings, stacks, agents, conditions);
  const contentSha256 = sha256(canonicalDefinitionJson(identityDocument));
  const attempts = expandAttempts(definition, requestedLevels, featureCatalog?.identity ?? null,
    dependencyPolicy?.identity ?? null, stacks, agents, conditions);
  const repetitionsByStack = Object.fromEntries(definition.stacks.map(stack =>
    [stack.id, stack.repetitions ?? definition.repetitions] as const).sort(([left], [right]) =>
    left.localeCompare(right)));
  return canonicalizeDefinition({
    campaignSchemaVersion: CAMPAIGN_SCHEMA_VERSION,
    id: definition.id,
    version: definition.version,
    state: definition.state,
    title: definition.title,
    source: { sha256: sourceSha256 },
    contentSha256,
    definition,
    identities: { engine },
    featureCatalog,
    dependencyPolicy,
    bindings,
    stacks,
    agents,
    conditions,
    attempts,
    summary: { attempts: attempts.length, stacks: stacks.length, agents: agents.length,
      conditions: conditions.length, repetitions: definition.repetitions,
      repetitionsByStack, parallelism: definition.parallelism ?? 1 },
  }) as unknown as CompiledCampaignPlan;
}

export function validateCompiledCampaignPlan(input: unknown, {
  requireCurrentInputs = true,
  calibrationResolver = resolveCalibrationForRelease,
}: ValidationOptions = {}): CompiledCampaignPlan {
  if (!object(input)) throw new Error('compiled campaign plan must be an object');
  const plan = canonicalizeDefinition(input) as unknown as CompiledCampaignPlan;
  const fields = new Set(['campaignSchemaVersion', 'id', 'version', 'state', 'title', 'source',
    'contentSha256', 'definition', 'identities', 'bindings', 'stacks', 'agents', 'conditions',
    'attempts', 'summary', 'featureCatalog', 'dependencyPolicy']);
  for (const key of Object.keys(plan)) if (!fields.has(key)) throw new Error(`compiled campaign plan.${key} is unknown`);
  if (plan.campaignSchemaVersion !== CAMPAIGN_SCHEMA_VERSION) throw new Error('compiled campaign schema is unsupported');
  const featureCatalog = plan.featureCatalog === null || plan.featureCatalog === undefined
    ? null : validateFeatureCatalogInput(plan.featureCatalog);
  const dependencyPolicy = plan.dependencyPolicy === null || plan.dependencyPolicy === undefined
    ? null : validateDependencyPolicyInput(plan.dependencyPolicy, featureCatalog);
  const definition = validateCampaignDefinition({ ...plan.definition,
    ...(featureCatalog ? { featureCatalog: featureCatalog.definition } : {}) },
  { source: 'compiled campaign definition' });
  if ((definition.mode.id === 'dependency') !== Boolean(dependencyPolicy)) {
    throw new Error('compiled campaign dependency policy does not match its mode');
  }
  const storedDefinition = structuredClone(definition);
  if (featureCatalog) delete storedDefinition.featureCatalog;
  if (canonicalDefinitionJson(storedDefinition) !== canonicalDefinitionJson(plan.definition)) {
    throw new Error('compiled campaign definition does not match its feature catalog');
  }
  for (const field of ['id', 'version', 'state', 'title'] as const) {
    if (plan[field] !== definition[field]) throw new Error(`compiled campaign ${field} does not match its definition`);
  }
  if (!object(plan.source) || Object.keys(plan.source).length !== 1 || !HASH.test(plan.source.sha256)) {
    throw new Error('compiled campaign source identity is invalid');
  }
  if (!object(plan.identities) || !object(plan.identities.engine)) {
    throw new Error('compiled campaign engine identity is missing');
  }
  if (!Array.isArray(plan.bindings) || plan.bindings.length !== definition.levels.length
    || !Array.isArray(plan.stacks) || plan.stacks.length !== definition.stacks.length
    || !Array.isArray(plan.agents) || plan.agents.length !== definition.agents.length
    || !Array.isArray(plan.conditions) || plan.conditions.length !== definition.conditions.length) {
    throw new Error('compiled campaign resolved inputs are incomplete');
  }
  if (requireCurrentInputs) {
    const currentEngine = currentEngineIdentity();
    if (canonicalDefinitionJson(plan.identities.engine) !== canonicalDefinitionJson(currentEngine)) {
      throw new Error('compiled campaign engine identity does not match this executable');
    }
    const resolved = resolveCampaignInputs(definition, { calibrationResolver });
    for (const field of ['bindings', 'stacks', 'agents', 'conditions', 'featureCatalog',
      'dependencyPolicy'] as const) {
      if (canonicalDefinitionJson(plan[field]) !== canonicalDefinitionJson(resolved[field])) {
        throw new Error(`compiled campaign ${field} do not match current resolved inputs`);
      }
    }
  }
  const expectedSha256 = sha256(canonicalDefinitionJson(campaignIdentityDocument(
    storedDefinition, plan.identities.engine, featureCatalog, dependencyPolicy,
    plan.bindings, plan.stacks,
    plan.agents, plan.conditions)));
  if (plan.contentSha256 !== expectedSha256) throw new Error('compiled campaign content identity does not match its inputs');
  const expectedAttempts = expandAttempts(plan.definition, definition.levels,
    featureCatalog?.identity ?? null, dependencyPolicy?.identity ?? null,
    plan.stacks, plan.agents, plan.conditions);
  if (canonicalDefinitionJson(plan.attempts) !== canonicalDefinitionJson(expectedAttempts)) {
    throw new Error('compiled campaign attempt schedule does not match its inputs');
  }
  const repetitionsByStack = Object.fromEntries(definition.stacks.map(stack =>
    [stack.id, stack.repetitions ?? definition.repetitions] as const).sort(([left], [right]) =>
    left.localeCompare(right)));
  const expectedSummary = { attempts: expectedAttempts.length, stacks: plan.stacks.length,
    agents: plan.agents.length, conditions: plan.conditions.length,
    repetitions: definition.repetitions, repetitionsByStack,
    parallelism: definition.parallelism ?? 1 };
  if (canonicalDefinitionJson(plan.summary) !== canonicalDefinitionJson(expectedSummary)) {
    throw new Error('compiled campaign summary does not match its inputs');
  }
  return plan;
}

export function campaignIdentity(plan: CompiledCampaignPlan, options: ValidationOptions = {}): {
  id: string; version: string; sha256: string; state: CampaignState;
} {
  const validated = validateCompiledCampaignPlan(plan, options);
  return { id: validated.id, version: validated.version, sha256: validated.contentSha256,
    state: validated.state };
}
