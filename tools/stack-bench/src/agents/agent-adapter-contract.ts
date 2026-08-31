import { validatePricingAuthority } from '../evidence/pricing-authority.js';

export const AGENT_ADAPTER_SCHEMA_VERSION = 5;

const FIELDS = new Set(['schemaVersion', 'id', 'version', 'entrypoint', 'modes', 'deadlineMs',
  'defaultModel', 'apiKeyEnvironmentVariable', 'credentialEnvironmentVariables',
  'credentialFiles', 'outboundDestinations', 'requiredExecutables', 'credentialStatusCommand',
  'usesStackSkills', 'costLimit']);
const ID = /^[a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*$/;
const VERSION = /^\d+\.\d+\.\d+$/;
const MODES = new Set(['build', 'upgrade', 'resume', 'fix']);
const COST_LIMITS = new Set(['native', 'non-billable', 'unsupported']);
type UnknownRecord = Record<string, unknown>;
export type AgentMode = 'build' | 'upgrade' | 'resume' | 'fix';
export type AgentCostLimit = 'native' | 'non-billable' | 'unsupported';

export interface AgentAdapter {
  readonly schemaVersion: 5;
  readonly id: string;
  readonly version: string;
  readonly entrypoint: string;
  readonly modes: readonly AgentMode[];
  readonly deadlineMs: number;
  readonly defaultModel: string;
  readonly apiKeyEnvironmentVariable: string | null;
  readonly credentialEnvironmentVariables: readonly string[];
  readonly credentialFiles: readonly string[];
  readonly outboundDestinations: readonly string[];
  readonly requiredExecutables: readonly string[];
  readonly credentialStatusCommand: readonly string[] | null;
  readonly usesStackSkills: boolean;
  readonly costLimit: AgentCostLimit;
}

export interface AgentRequest {
  app: string;
  mode: AgentMode;
  level: number;
  backend: string;
  track: string;
  runIndex: number;
  model: string;
  guidance: string;
  adapterCostLimit?: AgentCostLimit;
  maxBudgetUsd?: number | null;
  pricing?: unknown;
  recipe?: string | null;
  guidanceDocument?: unknown;
  credentialAliases?: Record<string, unknown> | null;
  skills?: unknown[] | null;
  skillIdentity?: unknown;
  recipeTask?: unknown;
}

export interface AgentAdapterRegistry {
  readonly ids: readonly string[];
  get(id: string): AgentAdapter;
}

const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

export function defineAgentAdapter(value: unknown): AgentAdapter {
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
  if (typeof value.deadlineMs !== 'number'
    || !Number.isInteger(value.deadlineMs) || value.deadlineMs < 1_000) {
    throw new Error(`agent adapter ${value.id}.deadlineMs is invalid`);
  }
  if (typeof value.defaultModel !== 'string' || !value.defaultModel) {
    throw new Error(`agent adapter ${value.id}.defaultModel is required`);
  }
  if (typeof value.costLimit !== 'string' || !COST_LIMITS.has(value.costLimit)) {
    throw new Error(`agent adapter ${value.id}.costLimit is invalid`);
  }
  if (typeof value.usesStackSkills !== 'boolean') {
    throw new Error(`agent adapter ${value.id}.usesStackSkills is invalid`);
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
  const relativeCredential = (item: unknown): boolean => typeof item === 'string' && Boolean(item)
    && !item.startsWith('/') && !/^[A-Za-z]:[\\/]/.test(item)
    && !item.split(/[\\/]/).includes('..');
  const secureDestination = (item: unknown): boolean => {
    if (typeof item !== 'string') return false;
    try { return new URL(item).protocol === 'https:'; } catch { return false; }
  };
  const arrayFields: Array<[string, (item: unknown) => boolean]> = [
    ['credentialEnvironmentVariables', (item: unknown) => typeof item === 'string'
      && /^[A-Z][A-Z0-9_]*$/.test(item)],
    ['credentialFiles', relativeCredential],
    ['outboundDestinations', secureDestination],
    ['requiredExecutables', (item: unknown) => typeof item === 'string'
      && /^[a-zA-Z0-9][a-zA-Z0-9._+-]*$/.test(item)],
  ];
  for (const [field, validate] of arrayFields) {
    const items = value[field];
    if (!Array.isArray(items) || items.some(item => !validate(item))
      || new Set(items).size !== items.length) {
      throw new Error(`agent adapter ${value.id}.${field} is invalid`);
    }
  }
  return Object.freeze({ ...value, modes: Object.freeze([...(value.modes as string[])].sort()),
    credentialEnvironmentVariables: Object.freeze(
      [...value.credentialEnvironmentVariables as string[]].sort()),
    credentialFiles: Object.freeze([...value.credentialFiles as string[]].sort()),
    outboundDestinations: Object.freeze([...value.outboundDestinations as string[]].sort()),
    requiredExecutables: Object.freeze([...value.requiredExecutables as string[]].sort()),
    credentialStatusCommand: value.credentialStatusCommand === null ? null
      : Object.freeze([...value.credentialStatusCommand as string[]]) }) as unknown as AgentAdapter;
}

export function createAgentAdapterRegistry(adapters: unknown): AgentAdapterRegistry {
  if (!Array.isArray(adapters)) throw new Error('agent adapter registry requires an array');
  const entries = new Map<string, AgentAdapter>();
  for (const source of adapters) {
    const adapter = defineAgentAdapter(source);
    if (entries.has(adapter.id)) throw new Error(`duplicate agent adapter ${adapter.id}`);
    entries.set(adapter.id, adapter);
  }
  const ids = Object.freeze([...entries.keys()].sort());
  return Object.freeze({ ids, get(id: string) {
    const adapter = entries.get(id);
    if (!adapter) throw new Error(`unknown agent adapter ${JSON.stringify(id)}`);
    return adapter;
  } });
}

export function agentRecipeIdentity(
  explicitRecipe: string | null | undefined,
  recipeTask: unknown,
): string | null {
  const task = object(recipeTask) ? recipeTask : null;
  const bound = task?.recipe;
  if (!bound) return explicitRecipe ?? null;
  if (!object(bound) || typeof bound.id !== 'string' || !bound.id
    || typeof bound.version !== 'string' || !bound.version) {
    throw new Error('recipe-bound agent task has an invalid recipe identity');
  }
  const identity = `${bound.id}@${bound.version}`;
  if (explicitRecipe && explicitRecipe !== identity) {
    throw new Error(`agent recipe ${explicitRecipe} does not match bound task ${identity}`);
  }
  return identity;
}

export function agentRequestArgv(adapter: AgentAdapter, request: AgentRequest): string[] {
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
    ...(request.credentialAliases && Object.keys(request.credentialAliases).length
      ? ['--credential-aliases-json', JSON.stringify(request.credentialAliases)] : []),
    ...(request.skillIdentity
      ? ['--skill-identity-json', JSON.stringify(request.skillIdentity)] : []),
    ...(!request.skillIdentity && Array.isArray(request.skills)
      ? ['--skills-json', JSON.stringify(request.skills)] : []),
    ...(request.recipeTask
      ? ['--recipe-task-json', JSON.stringify(request.recipeTask)] : []),
    ...(request.pricing
      ? ['--pricing-json', JSON.stringify(validatePricingAuthority(request.pricing,
        { at: 'agent request pricing' }))] : []),
    ...(request.maxBudgetUsd != null && adapter.costLimit === 'native'
      ? ['--max-budget-usd', String(request.maxBudgetUsd)] : [])];
}
