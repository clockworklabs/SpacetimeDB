import { pricingRatesEqual, validatePricingAuthority }
  from '../evidence/pricing-authority.mjs';

export const AGENT_ADAPTER_SCHEMA_VERSION = 4;
export const AGENT_COST_RECEIPT_TOLERANCE_USD = 0.0001;

const FIELDS = new Set(['schemaVersion', 'id', 'version', 'entrypoint', 'modes', 'deadlineMs',
  'defaultModel', 'apiKeyEnvironmentVariable', 'credentialEnvironmentVariables',
  'credentialFiles', 'outboundDestinations', 'requiredExecutables', 'credentialStatusCommand',
  'usesStackSkills', 'costLimit', 'sandboxProbe']);
const RESULT_FIELDS = new Set(['appDir', 'mode', 'level', 'track', 'backend', 'model', 'guidance',
  'stack', 'setup', 'costUsd', 'tokens', 'outputTokens', 'usage', 'provenance', 'turns',
  'promptBytes', 'tokensPerTurn', 'thinking', 'durationMs', 'sessionId', 'ok',
  'providerMetadata', 'transcript', 'costReceipts']);
const ID = /^[a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*$/;
const VERSION = /^\d+\.\d+\.\d+$/;
const MODES = new Set(['build', 'upgrade', 'resume', 'fix']);
const COST_LIMITS = new Set(['native', 'non-billable', 'unsupported']);
const SANDBOX_PROBES = new Set(['direct-cli', 'none']);
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const RECEIPT_USAGE_FIELDS = ['input', 'output', 'cacheWrite5m', 'cacheWrite1h', 'cacheRead'];

function validateCostReceipts(value, model) {
  if (!Array.isArray(value)) throw new Error('agent result costReceipts must be an array');
  value.forEach((entry, index) => {
    if (!object(entry) || !Number.isSafeInteger(entry.invocation) || entry.invocation !== index + 1
      || !object(entry.receipt)) {
      throw new Error(`agent result costReceipts[${index}] is invalid`);
    }
    const receipt = entry.receipt;
    const fields = new Set(['schemaVersion', 'source', 'model', 'maxBudgetUsd', 'costUsd',
      'cliCostUsd', 'calculatedCostUsd', 'usage', 'pricingRates', 'complete', 'reconciled',
      'error']);
    if (Object.keys(receipt).some(key => !fields.has(key)) || receipt.schemaVersion !== 2
      || receipt.source !== 'credential-broker' || receipt.model !== model
      || !Number.isFinite(receipt.maxBudgetUsd) || receipt.maxBudgetUsd <= 0
      || !Number.isFinite(receipt.costUsd) || receipt.costUsd < 0
      || ![receipt.cliCostUsd, receipt.calculatedCostUsd]
        .every(cost => cost === null || (Number.isFinite(cost) && cost >= 0))
      || typeof receipt.complete !== 'boolean' || typeof receipt.reconciled !== 'boolean'
      || (receipt.error !== null && (typeof receipt.error !== 'string' || !receipt.error))
      || receipt.reconciled !== (receipt.error === null)) {
      throw new Error(`agent result costReceipts[${index}].receipt is invalid`);
    }
    for (const field of ['usage', 'pricingRates']) {
      const values = receipt[field];
      if (values === null) continue;
      if (!object(values) || Object.keys(values).some(key => !RECEIPT_USAGE_FIELDS.includes(key))
        || RECEIPT_USAGE_FIELDS.some(key => !Number.isFinite(values[key]) || values[key] < 0)) {
        throw new Error(`agent result costReceipts[${index}].receipt.${field} is invalid`);
      }
    }
    if (receipt.reconciled && (!receipt.complete || receipt.error !== null
      || receipt.usage === null || receipt.pricingRates === null
      || receipt.cliCostUsd === null || receipt.calculatedCostUsd === null)) {
      throw new Error(`agent result costReceipts[${index}].receipt is incomplete`);
    }
  });
  return value;
}

export function defineAgentAdapter(value) {
  if (!object(value)) throw new Error('agent adapter must be an object');
  for (const key of Object.keys(value)) if (!FIELDS.has(key)) throw new Error(`agent adapter.${key} is unknown`);
  if (value.schemaVersion !== AGENT_ADAPTER_SCHEMA_VERSION) throw new Error('agent adapter schema is unsupported');
  if (typeof value.id !== 'string' || !ID.test(value.id)) throw new Error('agent adapter.id is invalid');
  if (typeof value.version !== 'string' || !VERSION.test(value.version)) {
    throw new Error(`agent adapter ${value.id}.version is invalid`);
  }
  if (typeof value.entrypoint !== 'string' || !value.entrypoint) {
    throw new Error(`agent adapter ${value.id}.entrypoint is required`);
  }
  if (!Array.isArray(value.modes) || value.modes.length === 0
    || value.modes.some(mode => !MODES.has(mode)) || new Set(value.modes).size !== value.modes.length) {
    throw new Error(`agent adapter ${value.id}.modes is invalid`);
  }
  if (!Number.isInteger(value.deadlineMs) || value.deadlineMs < 1_000) {
    throw new Error(`agent adapter ${value.id}.deadlineMs is invalid`);
  }
  if (typeof value.defaultModel !== 'string' || !value.defaultModel) {
    throw new Error(`agent adapter ${value.id}.defaultModel is required`);
  }
  if (!COST_LIMITS.has(value.costLimit)) {
    throw new Error(`agent adapter ${value.id}.costLimit is invalid`);
  }
  if (typeof value.usesStackSkills !== 'boolean') {
    throw new Error(`agent adapter ${value.id}.usesStackSkills is invalid`);
  }
  if (!SANDBOX_PROBES.has(value.sandboxProbe)) {
    throw new Error(`agent adapter ${value.id}.sandboxProbe is invalid`);
  }
  if (value.credentialStatusCommand !== null
    && (!Array.isArray(value.credentialStatusCommand) || value.credentialStatusCommand.length === 0
      || value.credentialStatusCommand.some(item => typeof item !== 'string' || !item
        || /[\r\n\0]/.test(item)))) {
    throw new Error(`agent adapter ${value.id}.credentialStatusCommand is invalid`);
  }
  if (value.apiKeyEnvironmentVariable !== null
    && (typeof value.apiKeyEnvironmentVariable !== 'string'
      || !/^[A-Z][A-Z0-9_]*$/.test(value.apiKeyEnvironmentVariable))) {
    throw new Error(`agent adapter ${value.id}.apiKeyEnvironmentVariable is invalid`);
  }
  const relativeCredential = item => typeof item === 'string' && item
    && !item.startsWith('/') && !/^[A-Za-z]:[\\/]/.test(item)
    && !item.split(/[\\/]/).includes('..');
  const secureDestination = item => {
    try { return new URL(item).protocol === 'https:'; } catch { return false; }
  };
  for (const [field, validate] of [
    ['credentialEnvironmentVariables', item => typeof item === 'string'
      && /^[A-Z][A-Z0-9_]*$/.test(item)],
    ['credentialFiles', relativeCredential],
    ['outboundDestinations', secureDestination],
    ['requiredExecutables', item => typeof item === 'string' && /^[a-zA-Z0-9][a-zA-Z0-9._+-]*$/.test(item)],
  ]) {
    if (!Array.isArray(value[field]) || value[field].some(item => !validate(item))
      || new Set(value[field]).size !== value[field].length) {
      throw new Error(`agent adapter ${value.id}.${field} is invalid`);
    }
  }
  return Object.freeze({ ...value, modes: Object.freeze([...value.modes].sort()),
    credentialEnvironmentVariables: Object.freeze([...value.credentialEnvironmentVariables].sort()),
    credentialFiles: Object.freeze([...value.credentialFiles].sort()),
    outboundDestinations: Object.freeze([...value.outboundDestinations].sort()),
    requiredExecutables: Object.freeze([...value.requiredExecutables].sort()),
    credentialStatusCommand: value.credentialStatusCommand === null ? null
      : Object.freeze([...value.credentialStatusCommand]) });
}

export function createAgentAdapterRegistry(adapters) {
  if (!Array.isArray(adapters)) throw new Error('agent adapter registry requires an array');
  const entries = new Map();
  for (const source of adapters) {
    const adapter = defineAgentAdapter(source);
    if (entries.has(adapter.id)) throw new Error(`duplicate agent adapter ${adapter.id}`);
    entries.set(adapter.id, adapter);
  }
  const ids = Object.freeze([...entries.keys()].sort());
  return Object.freeze({ ids, get(id) {
    const adapter = entries.get(id);
    if (!adapter) throw new Error(`unknown agent adapter ${JSON.stringify(id)}`);
    return adapter;
  } });
}

const finite = (value, at) => {
  if (!Number.isFinite(value) || value < 0) throw new Error(`agent result ${at} must be a non-negative number`);
  return value;
};

export function validateAgentResult(value, request) {
  if (!object(value)) throw new Error('agent result must be an object');
  for (const key of Object.keys(value)) {
    if (!RESULT_FIELDS.has(key)) throw new Error(`agent result ${key} is unknown`);
  }
  if (value.appDir !== request.app) throw new Error('agent result appDir does not match the request');
  if (value.mode !== request.mode) throw new Error('agent result mode does not match the request');
  if (value.level !== request.level) throw new Error('agent result level does not match the request');
  if (value.backend !== undefined && value.backend !== request.backend) {
    throw new Error('agent result backend does not match the request');
  }
  if (value.track !== undefined && value.track !== request.track) {
    throw new Error('agent result track does not match the request');
  }
  if (value.model !== undefined && value.model !== request.model) {
    throw new Error('agent result model does not match the request');
  }
  if (typeof value.ok !== 'boolean') throw new Error('agent result ok must be boolean');
  if (value.sessionId !== null && (typeof value.sessionId !== 'string' || !value.sessionId)) {
    throw new Error('agent result sessionId must be a non-empty string or null');
  }
  if (!object(value.usage)) throw new Error('agent result usage must be an object');
  for (const key of Object.keys(value.usage)) {
    if (!['input', 'output', 'cacheWrite', 'cacheRead'].includes(key)) {
      throw new Error(`agent result usage.${key} is unknown`);
    }
  }
  if (!object(value.setup)) throw new Error('agent result setup must be an object');
  if (value.provenance !== undefined && value.provenance !== null && !object(value.provenance)) {
    throw new Error('agent result provenance must be an object or null');
  }
  if (value.providerMetadata !== undefined && value.providerMetadata !== null
    && !object(value.providerMetadata)) throw new Error('agent result providerMetadata must be an object or null');
  if (value.transcript !== undefined && value.transcript !== null) {
    if (!object(value.transcript)) throw new Error('agent result transcript must be an object or null');
    for (const key of Object.keys(value.transcript)) {
      if (!['kind', 'id'].includes(key)) throw new Error(`agent result transcript.${key} is unknown`);
    }
    if (typeof value.transcript.kind !== 'string' || !value.transcript.kind
      || typeof value.transcript.id !== 'string' || !value.transcript.id) {
      throw new Error('agent result transcript requires non-empty kind and id');
    }
  }
  const usage = Object.fromEntries(['input', 'output', 'cacheWrite', 'cacheRead']
    .map(key => [key, finite(value.usage[key], `usage.${key}`)]));
  const costUsd = finite(value.costUsd, 'costUsd');
  const costReceipts = validateCostReceipts(
    value.costReceipts === undefined ? [] : value.costReceipts, request.model);
  const pricing = request.pricing == null ? null
    : validatePricingAuthority(request.pricing, { at: 'agent request pricing' });
  const cappedNative = request.adapterCostLimit === 'native' && request.maxBudgetUsd != null;
  if (cappedNative && pricing === null) {
    throw new Error('agent request pricing is required for a native cost limit');
  }
  const receiptCostUsd = costReceipts.reduce((sum, { receipt }) => sum + receipt.costUsd, 0);
  const costComplete = request.adapterCostLimit === 'non-billable'
    || (cappedNative && costReceipts.length > 0
      && costReceipts.every(({ receipt }) => receipt.complete && receipt.reconciled
        && receipt.error === null && pricingRatesEqual(receipt.pricingRates, pricing?.rates))
      && Math.abs(receiptCostUsd - costUsd) <= AGENT_COST_RECEIPT_TOLERANCE_USD);
  if (value.ok && cappedNative && !costComplete) {
    throw new Error('successful agent result requires complete reconciled broker cost proof');
  }
  if (request.maxBudgetUsd != null && request.adapterCostLimit === 'unsupported') {
    throw new Error('agent result came from an adapter that cannot enforce a cost limit');
  }
  return {
    ...value,
    backend: request.backend,
    track: request.track,
    model: request.model,
    guidance: value.guidance ?? request.guidance,
    costUsd,
    tokens: finite(value.tokens, 'tokens'),
    outputTokens: finite(value.outputTokens, 'outputTokens'),
    turns: finite(value.turns, 'turns'),
    promptBytes: finite(value.promptBytes, 'promptBytes'),
    durationMs: finite(value.durationMs, 'durationMs'),
    usage,
    costReceipts,
    costComplete,
    transcript: value.transcript ?? (value.sessionId
      ? { kind: 'provider-session', id: value.sessionId }
      : null),
  };
}

export function agentSessionFailure(result) {
  if (result.ok === true && result.sessionId) return null;
  const failureCode = result.providerMetadata?.failureCode;
  return { kind: 'harness_failure', phase: 'coding-session',
    reason: typeof failureCode === 'string' && failureCode ? failureCode
      : result.sessionId ? 'coding session reported failure' : 'coding session did not run',
    provider: result.providerMetadata?.failure ?? null,
    appFailures: [], inconclusive: [], harnessFailures: [] };
}

export function agentRecipeIdentity(explicitRecipe, recipeTask) {
  const bound = recipeTask?.recipe;
  if (!bound) return explicitRecipe ?? null;
  if (typeof bound.id !== 'string' || !bound.id
    || typeof bound.version !== 'string' || !bound.version) {
    throw new Error('recipe-bound agent task has an invalid recipe identity');
  }
  const identity = `${bound.id}@${bound.version}`;
  if (explicitRecipe && explicitRecipe !== identity) {
    throw new Error(`agent recipe ${explicitRecipe} does not match bound task ${identity}`);
  }
  return identity;
}

export function agentRequestArgv(adapter, request) {
  if (!adapter.modes.includes(request.mode)) {
    throw new Error(`agent adapter ${adapter.id} does not support mode ${request.mode}`);
  }
  if (request.maxBudgetUsd != null && adapter.costLimit === 'unsupported') {
    throw new Error(`agent adapter ${adapter.id} cannot enforce a cost limit`);
  }
  return [adapter.entrypoint, '--mode', request.mode, '--backend', request.backend,
    '--level', String(request.level), '--app', request.app, '--track', request.track,
    '--run-index', String(request.runIndex), '--model', request.model,
    '--guidance', request.guidance,
    ...(request.recipe ? ['--recipe', request.recipe] : []),
    ...(request.guidanceDocument
      ? ['--guidance-document-json', JSON.stringify(request.guidanceDocument)] : []),
    ...(Array.isArray(request.skills)
      ? ['--skills-json', JSON.stringify(request.skills)] : []),
    ...(request.recipeTask
      ? ['--recipe-task-json', JSON.stringify(request.recipeTask)] : []),
    ...(request.pricing
      ? ['--pricing-json', JSON.stringify(validatePricingAuthority(request.pricing,
        { at: 'agent request pricing' }))] : []),
    ...(request.maxBudgetUsd != null && adapter.costLimit === 'native'
      ? ['--max-budget-usd', String(request.maxBudgetUsd)] : [])];
}
