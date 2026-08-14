import { readFileSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { agentAdapterIdentity, AGENT_ADAPTER_REGISTRY } from './agent-adapters.mjs';
import { resolveCalibrationForRelease } from './calibration-compiler.mjs';
import { canonicalDefinitionJson, canonicalizeDefinition } from './definition-plan.mjs';
import { currentEngineIdentity } from './artifacts.mjs';
import { sha256 } from './provenance.mjs';
import { recipeReleaseIdentity, resolveLegacyRecipeRelease } from './recipe-release.mjs';
import { resolveRecipeSelection } from './recipe-selection.mjs';
import { executeStackCapability } from './stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.mjs';
import { listTracks, loadTrack } from './tracks.mjs';

export const CAMPAIGN_SCHEMA_VERSION = 1;
const ROOT = dirname(fileURLToPath(import.meta.url));
const HASH = /^[a-f0-9]{64}$/;
const ID = /^[a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*$/;
const VERSION = /^\d+\.\d+\.\d+$/;
const GUIDANCE = new Set(['prescribed', 'minimal']);
const ROOT_FIELDS = new Set(['schemaVersion', 'kind', 'id', 'version', 'state', 'title',
  'track', 'levels', 'selection', 'stacks', 'agents', 'repetitions', 'ordering',
  'budgets', 'attemptPolicy', 'pricing', 'analysis']);
ROOT_FIELDS.add('runtime');
const SELECTION_FIELDS = new Set(['packs', 'checks']);
const STACK_FIELDS = new Set(['id', 'adapterVersion']);
const AGENT_FIELDS = new Set(['adapter', 'adapterVersion', 'model', 'guidance', 'skills']);
const ORDERING_FIELDS = new Set(['method', 'seed']);
const BUDGET_FIELDS = new Set(['fixRounds', 'attemptTimeoutMinutes', 'maxCostUsdPerAttempt']);
const ATTEMPT_POLICY_FIELDS = new Set(['retries', 'retryOn', 'excludeFromAnalysis']);
const PRICING_FIELDS = new Set(['currency', 'capturedAt', 'source', 'models']);
const PRICE_MODEL_FIELDS = new Set(['inputPerMillion', 'outputPerMillion',
  'cacheWritePerMillion', 'cacheReadPerMillion']);
const ANALYSIS_FIELDS = new Set(['primaryMetric', 'secondaryMetrics', 'dispersion',
  'invalidAttempts', 'missingData', 'comparisonUnit']);
const RUNTIME_FIELDS = new Set(['releaseManifestSha256', 'controllerImage', 'buildImage', 'platform']);
const RETRY_CAUSES = new Set(['harness_failure']);
const EXCLUSION_CAUSES = new Set(['harness_failure', 'ungraded', 'contaminated']);
const OUTCOME_METRICS = new Set(['firstBuildScoreRate', 'finalScoreRate', 'totalCostUsd',
  'totalDurationMs', 'fixRounds', 'correctionSuccessRate', 'correctionCostUsd',
  'correctionSpendUsd', 'invalidAttemptRate']);
const DISPERSION = new Set(['median-iqr', 'mean-sd']);
const IMAGE_DIGEST = /^[^\s@]+@sha256:[a-f0-9]{64}$/;
const ISO = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/;

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const fail = (at, message) => { throw new Error(`invalid campaign at ${at}: ${message}`); };

function strict(value, at, fields) {
  if (!object(value)) fail(at, 'must be an object');
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`${at}.${key}`, 'is unknown');
}

function string(value, at) {
  if (typeof value !== 'string' || !value.trim()) fail(at, 'must be a non-empty string');
  return value;
}

function identifier(value, at) {
  if (typeof value !== 'string' || !ID.test(value)) fail(at, 'is invalid');
  return value;
}

function version(value, at) {
  if (typeof value !== 'string' || !VERSION.test(value)) fail(at, 'must be a semantic version');
  return value;
}

function integer(value, at, { min = 0, max = Number.MAX_SAFE_INTEGER } = {}) {
  if (!Number.isInteger(value) || value < min || value > max) fail(at, `must be an integer from ${min} through ${max}`);
  return value;
}

function finite(value, at, { min = 0 } = {}) {
  if (!Number.isFinite(value) || value < min) fail(at, `must be a number of at least ${min}`);
  return value;
}

function exactArray(value, at, validate, { nonEmpty = false, sort = false } = {}) {
  if (!Array.isArray(value) || (nonEmpty && value.length === 0)) fail(at, `must be ${nonEmpty ? 'a non-empty' : 'an'} array`);
  const normalized = value.map((item, index) => validate(item, `${at}[${index}]`));
  const keys = normalized.map(item => typeof item === 'string' ? item : canonicalDefinitionJson(item));
  if (new Set(keys).size !== keys.length) fail(at, 'must not contain duplicates');
  return sort ? normalized.sort((a, b) => canonicalDefinitionJson(a).localeCompare(canonicalDefinitionJson(b))) : normalized;
}

export function validateCampaignDefinition(input, { source = '<campaign>' } = {}) {
  const value = structuredClone(input);
  strict(value, source, ROOT_FIELDS);
  if (value.schemaVersion !== CAMPAIGN_SCHEMA_VERSION) fail(`${source}.schemaVersion`, 'is unsupported');
  if (value.kind !== 'campaign-manifest') fail(`${source}.kind`, 'must be campaign-manifest');
  identifier(value.id, `${source}.id`);
  version(value.version, `${source}.version`);
  if (!['draft', 'frozen'].includes(value.state)) fail(`${source}.state`, 'must be draft or frozen');
  string(value.title, `${source}.title`);
  identifier(value.track, `${source}.track`);
  value.levels = exactArray(value.levels, `${source}.levels`, (level, at) => integer(level, at, { min: 1 }),
    { nonEmpty: true });
  for (let index = 1; index < value.levels.length; index += 1) {
    if (value.levels[index] !== value.levels[index - 1] + 1) fail(`${source}.levels`, 'must be ascending and contiguous');
  }

  strict(value.selection, `${source}.selection`, SELECTION_FIELDS);
  value.selection.packs = exactArray(value.selection.packs, `${source}.selection.packs`, string, { sort: true });
  value.selection.checks = exactArray(value.selection.checks, `${source}.selection.checks`, string, { sort: true });

  value.stacks = exactArray(value.stacks, `${source}.stacks`, (stack, at) => {
    strict(stack, at, STACK_FIELDS);
    identifier(stack.id, `${at}.id`);
    version(stack.adapterVersion, `${at}.adapterVersion`);
    return stack;
  }, { nonEmpty: true, sort: true });
  if (new Set(value.stacks.map(stack => stack.id)).size !== value.stacks.length) {
    fail(`${source}.stacks`, 'must name each stack once');
  }
  value.agents = exactArray(value.agents, `${source}.agents`, (agent, at) => {
    strict(agent, at, AGENT_FIELDS);
    identifier(agent.adapter, `${at}.adapter`);
    version(agent.adapterVersion, `${at}.adapterVersion`);
    string(agent.model, `${at}.model`);
    if (!GUIDANCE.has(agent.guidance)) fail(`${at}.guidance`, 'must be prescribed or minimal');
    agent.skills = exactArray(agent.skills, `${at}.skills`, string, { sort: true });
    return agent;
  }, { nonEmpty: true, sort: true });
  const agentKeys = value.agents.map(agent => canonicalDefinitionJson(agent));
  if (new Set(agentKeys).size !== agentKeys.length) fail(`${source}.agents`, 'contains a duplicate configuration');
  integer(value.repetitions, `${source}.repetitions`, { min: 1, max: 100 });

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
      if (!RETRY_CAUSES.has(item)) fail(at, 'has an unsupported retry cause'); return item;
    }, { sort: true });
  value.attemptPolicy.excludeFromAnalysis = exactArray(value.attemptPolicy.excludeFromAnalysis,
    `${source}.attemptPolicy.excludeFromAnalysis`, (item, at) => {
      if (!EXCLUSION_CAUSES.has(item)) fail(at, 'has an unsupported exclusion cause'); return item;
    }, { sort: true });
  if (value.attemptPolicy.retries > 0 && value.attemptPolicy.retryOn.length === 0) {
    fail(`${source}.attemptPolicy.retryOn`, 'must name a cause when retries are enabled');
  }

  strict(value.runtime, `${source}.runtime`, RUNTIME_FIELDS);
  for (const field of ['releaseManifestSha256', 'controllerImage', 'buildImage']) {
    if (value.runtime[field] !== null && typeof value.runtime[field] !== 'string') {
      fail(`${source}.runtime.${field}`, 'must be a string or null');
    }
  }
  if (value.runtime.releaseManifestSha256 !== null && !HASH.test(value.runtime.releaseManifestSha256)) {
    fail(`${source}.runtime.releaseManifestSha256`, 'must be a SHA-256 digest or null');
  }
  for (const field of ['controllerImage', 'buildImage']) {
    if (value.runtime[field] !== null && !IMAGE_DIGEST.test(value.runtime[field])) {
      fail(`${source}.runtime.${field}`, 'must be an exact image digest reference or null');
    }
  }
  if (value.runtime.platform !== 'linux/amd64') fail(`${source}.runtime.platform`, 'must be linux/amd64');

  strict(value.pricing, `${source}.pricing`, PRICING_FIELDS);
  if (value.pricing.currency !== 'USD') fail(`${source}.pricing.currency`, 'must be USD');
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
    for (const field of PRICE_MODEL_FIELDS) finite(rates[field], `${source}.pricing.models.${model}.${field}`);
  }
  const selectedModels = new Set(value.agents.map(agent => agent.model));
  for (const model of selectedModels) {
    if (!Object.hasOwn(value.pricing.models, model)) fail(`${source}.pricing.models`, `has no rates for ${model}`);
  }

  strict(value.analysis, `${source}.analysis`, ANALYSIS_FIELDS);
  if (!OUTCOME_METRICS.has(value.analysis.primaryMetric)) fail(`${source}.analysis.primaryMetric`, 'is unsupported');
  value.analysis.secondaryMetrics = exactArray(value.analysis.secondaryMetrics,
    `${source}.analysis.secondaryMetrics`, (item, at) => {
      if (!OUTCOME_METRICS.has(item)) fail(at, 'is unsupported'); return item;
    });
  if (value.analysis.secondaryMetrics.includes(value.analysis.primaryMetric)) {
    fail(`${source}.analysis.secondaryMetrics`, 'must not repeat the primary metric');
  }
  if (!DISPERSION.has(value.analysis.dispersion)) fail(`${source}.analysis.dispersion`, 'is unsupported');
  if (value.analysis.invalidAttempts !== 'report-separately') {
    fail(`${source}.analysis.invalidAttempts`, 'must be report-separately');
  }
  if (value.analysis.missingData !== 'no-imputation') fail(`${source}.analysis.missingData`, 'must be no-imputation');
  if (value.analysis.comparisonUnit !== 'stack-agent-recipe') {
    fail(`${source}.analysis.comparisonUnit`, 'must be stack-agent-recipe');
  }
  if (value.state === 'frozen') {
    if (value.budgets.maxCostUsdPerAttempt === null) {
      fail(`${source}.budgets.maxCostUsdPerAttempt`, 'is required for a frozen campaign');
    }
    for (const field of ['controllerImage', 'buildImage']) {
      if (value.runtime[field] === null) fail(`${source}.runtime.${field}`, 'is required for a frozen campaign');
    }
  }
  return canonicalizeDefinition(value);
}

function loadJson(path) {
  try { return JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) { throw new Error(`cannot read campaign ${path}: ${error.message}`, { cause: error }); }
}

function exactSource(path) {
  if (!isAbsolute(path)) throw new Error('campaign path must be absolute after resolution');
  return realpathSync(path);
}

function rotate(values, offset) {
  return [...values.slice(offset), ...values.slice(0, offset)];
}

function identityForStack(adapter) {
  return { id: adapter.id, version: adapter.version, sha256: null, state: null };
}

function calibrationIdentity(value) {
  return value ? { id: value.id, version: value.version, sha256: value.contentSha256, state: value.state } : null;
}

function campaignIdentityDocument(definition, engine, bindings, stacks, agents) {
  return canonicalizeDefinition({ campaignSchemaVersion: CAMPAIGN_SCHEMA_VERSION,
    definition, engine, bindings, stacks, agents });
}

function expandAttempts(definition, requestedLevels, stacks, agents) {
  const conditions = agents.flatMap((agent, agentIndex) => stacks.map(stack => ({
    agent,
    agentIndex,
    stack,
    key: canonicalDefinitionJson({ agent: { adapter: agent.adapter, model: agent.model,
      guidance: agent.guidance, skills: agent.skills }, stack: stack.id }),
  }))).sort((left, right) => {
    const leftHash = sha256(`${definition.ordering.seed}\0${left.key}`);
    const rightHash = sha256(`${definition.ordering.seed}\0${right.key}`);
    return leftHash.localeCompare(rightHash) || left.key.localeCompare(right.key);
  });
  const attempts = [];
  for (let repetition = 1; repetition <= definition.repetitions; repetition += 1) {
    rotate(conditions, (repetition - 1) % conditions.length)
      .forEach(({ agent, agentIndex, stack }, order) => attempts.push({
        id: `${definition.id}-r${repetition}-a${agentIndex + 1}-${stack.id}`,
        repetition,
        order: order + 1,
        stack: stack.id,
        agentAdapter: agent.adapter,
        model: agent.model,
        guidance: agent.guidance,
        skills: agent.skills,
        levels: requestedLevels,
        parentAttemptId: definition.id,
      }));
  }
  return attempts;
}

function resolveCampaignInputs(definition, { stackBenchRoot = ROOT } = {}) {
  if (!listTracks({ includeInternal: true }).includes(definition.track)) {
    fail('track', `is unknown; available tracks: ${listTracks({ includeInternal: true }).join(', ')}`);
  }
  const track = loadTrack(definition.track);
  const bindings = definition.levels.map(level => {
    const binding = resolveLegacyRecipeRelease(track, level);
    if (!binding) fail('levels', `L${level} has no recipe release`);
    const selection = resolveRecipeSelection(binding.release, {
      packIds: definition.selection.packs, checkKeys: definition.selection.checks,
    });
    const calibration = resolveCalibrationForRelease(binding.release, {
      trackRoot: track.dir, stackBenchRoot: resolve(stackBenchRoot),
    });
    return {
      level,
      promotion: { alias: binding.alias, status: binding.status, catalog: binding.catalog },
      recipe: recipeReleaseIdentity(binding.release),
      fixture: binding.release.components.fixture,
      calibration: calibrationIdentity(calibration),
      qualifiedStacks: calibration?.qualification.stacks ?? [],
      selection,
    };
  });
  if (definition.state === 'frozen') {
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
  }

  const stacks = definition.stacks.map(selection => {
    const adapter = STACK_ADAPTER_REGISTRY.get(selection.id);
    if (adapter.version !== selection.adapterVersion) fail('stacks', `${adapter.id} adapter is ${adapter.version}, not ${selection.adapterVersion}`);
    return identityForStack(adapter);
  });
  const agents = definition.agents.map(selection => {
    const adapter = AGENT_ADAPTER_REGISTRY.get(selection.adapter);
    if (adapter.version !== selection.adapterVersion) fail('agents', `${adapter.id} adapter is ${adapter.version}, not ${selection.adapterVersion}`);
    return { ...selection, costLimit: adapter.costLimit, identity: agentAdapterIdentity(adapter) };
  });
  if (definition.state === 'frozen' && agents.some(agent => agent.costLimit === 'unsupported')) {
    fail('state', 'cannot freeze an agent adapter that does not enforce maxCostUsdPerAttempt');
  }
  for (const agent of agents) {
    for (const stack of stacks) {
      if (agent.guidance === 'minimal' && !executeStackCapability(
        STACK_ADAPTER_REGISTRY.get(stack.id), 'agent', 'minimal-guidance-supported')) {
        fail('agents', `${agent.adapter}/${agent.model} requests minimal guidance unsupported by ${stack.id}`);
      }
    }
  }
  return { bindings, stacks, agents };
}

export function compileCampaignFile(path, { stackBenchRoot = ROOT } = {}) {
  const absolute = exactSource(resolve(path));
  const definition = validateCampaignDefinition(loadJson(absolute), {
    source: relative(process.cwd(), absolute).replaceAll('\\', '/'),
  });
  const requestedLevels = definition.levels;
  const { bindings, stacks, agents } = resolveCampaignInputs(definition, { stackBenchRoot });

  const sourceSha256 = sha256(readFileSync(absolute));
  const engine = currentEngineIdentity();
  const identityDocument = campaignIdentityDocument(definition, engine, bindings, stacks, agents);
  const contentSha256 = sha256(canonicalDefinitionJson(identityDocument));
  const attempts = expandAttempts(definition, requestedLevels, stacks, agents);
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
    bindings,
    stacks,
    agents,
    attempts,
    summary: { attempts: attempts.length, stacks: stacks.length, agents: agents.length,
      repetitions: definition.repetitions },
  });
}

export function validateCompiledCampaignPlan(input) {
  if (!object(input)) throw new Error('compiled campaign plan must be an object');
  const plan = canonicalizeDefinition(input);
  const fields = new Set(['campaignSchemaVersion', 'id', 'version', 'state', 'title', 'source',
    'contentSha256', 'definition', 'identities', 'bindings', 'stacks', 'agents', 'attempts', 'summary']);
  for (const key of Object.keys(plan)) if (!fields.has(key)) throw new Error(`compiled campaign plan.${key} is unknown`);
  if (plan.campaignSchemaVersion !== CAMPAIGN_SCHEMA_VERSION) throw new Error('compiled campaign schema is unsupported');
  const definition = validateCampaignDefinition(plan.definition, { source: 'compiled campaign definition' });
  for (const field of ['id', 'version', 'state', 'title']) {
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
    || !Array.isArray(plan.agents) || plan.agents.length !== definition.agents.length) {
    throw new Error('compiled campaign resolved inputs are incomplete');
  }
  const currentEngine = currentEngineIdentity();
  if (canonicalDefinitionJson(plan.identities.engine) !== canonicalDefinitionJson(currentEngine)) {
    throw new Error('compiled campaign engine identity does not match this executable');
  }
  const resolved = resolveCampaignInputs(definition);
  for (const field of ['bindings', 'stacks', 'agents']) {
    if (canonicalDefinitionJson(plan[field]) !== canonicalDefinitionJson(resolved[field])) {
      throw new Error(`compiled campaign ${field} do not match current resolved inputs`);
    }
  }
  const expectedSha256 = sha256(canonicalDefinitionJson(campaignIdentityDocument(
    definition, plan.identities.engine, plan.bindings, plan.stacks, plan.agents)));
  if (plan.contentSha256 !== expectedSha256) throw new Error('compiled campaign content identity does not match its inputs');
  const expectedAttempts = expandAttempts(definition, definition.levels, plan.stacks, plan.agents);
  if (canonicalDefinitionJson(plan.attempts) !== canonicalDefinitionJson(expectedAttempts)) {
    throw new Error('compiled campaign attempt schedule does not match its inputs');
  }
  const expectedSummary = { attempts: expectedAttempts.length, stacks: plan.stacks.length,
    agents: plan.agents.length, repetitions: definition.repetitions };
  if (canonicalDefinitionJson(plan.summary) !== canonicalDefinitionJson(expectedSummary)) {
    throw new Error('compiled campaign summary does not match its inputs');
  }
  return plan;
}

export function campaignIdentity(plan) {
  const validated = validateCompiledCampaignPlan(plan);
  return { id: validated.id, version: validated.version, sha256: validated.contentSha256,
    state: validated.state };
}
