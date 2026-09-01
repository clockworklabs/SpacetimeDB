import { validatePricingAuthority } from '../evidence/pricing-authority.js';
import { isExactSemanticVersion } from '../semantic-version.js';
import { formatZodError } from '../zod-error.js';
import { z } from 'zod';

export const AGENT_ADAPTER_SCHEMA_VERSION = 5;

const ID = /^[a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*$/;
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
const unique = (items: readonly string[]) => new Set(items).size === items.length;
const environmentVariableSchema = z.string().regex(/^[A-Z][A-Z0-9_]*$/);
const agentAdapterSchema = z.strictObject({
  schemaVersion: z.literal(AGENT_ADAPTER_SCHEMA_VERSION),
  id: z.string().regex(ID),
  version: z.string().refine(isExactSemanticVersion),
  entrypoint: z.string().min(1),
  modes: z.array(z.enum(['build', 'upgrade', 'resume', 'fix'])).min(1).refine(unique),
  deadlineMs: z.number().int().min(1_000),
  defaultModel: z.string().min(1),
  apiKeyEnvironmentVariable: environmentVariableSchema.nullable(),
  credentialEnvironmentVariables: z.array(environmentVariableSchema).refine(unique),
  credentialFiles: z.array(z.string().min(1).refine(item => !item.startsWith('/')
    && !/^[A-Za-z]:[\\/]/.test(item) && !item.split(/[\\/]/).includes('..'))).refine(unique),
  outboundDestinations: z.array(z.string().refine(item => {
    try { return new URL(item).protocol === 'https:'; } catch { return false; }
  })).refine(unique),
  requiredExecutables: z.array(z.string().regex(/^[a-zA-Z0-9][a-zA-Z0-9._+-]*$/)).refine(unique),
  credentialStatusCommand: z.array(z.string().min(1).refine(item => !/[\r\n\0]/.test(item)))
    .min(1).nullable(),
  usesStackSkills: z.boolean(),
  costLimit: z.enum(['native', 'non-billable', 'unsupported']),
});

export function defineAgentAdapter(value: unknown): AgentAdapter {
  const parsed = agentAdapterSchema.safeParse(value);
  if (!parsed.success) {
    throw new Error(formatZodError(parsed.error, 'agent adapter'));
  }
  const adapter = parsed.data;
  return Object.freeze({ ...adapter, modes: Object.freeze([...adapter.modes].sort()),
    credentialEnvironmentVariables: Object.freeze(
      [...adapter.credentialEnvironmentVariables].sort()),
    credentialFiles: Object.freeze([...adapter.credentialFiles].sort()),
    outboundDestinations: Object.freeze([...adapter.outboundDestinations].sort()),
    requiredExecutables: Object.freeze([...adapter.requiredExecutables].sort()),
    credentialStatusCommand: adapter.credentialStatusCommand === null ? null
      : Object.freeze([...adapter.credentialStatusCommand]) });
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
