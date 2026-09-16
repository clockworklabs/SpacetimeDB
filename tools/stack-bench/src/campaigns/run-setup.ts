import { homedir } from 'node:os';
import { credentialReady } from '../runtime/preflight.js';
import { AGENT_ADAPTER_REGISTRY } from '../agents/agent-adapters.js';
import { existsSync, mkdirSync, readdirSync, readFileSync, realpathSync } from 'node:fs';
import { isAbsolute, join, relative, sep } from 'node:path';
import { z } from 'zod';
import type { CampaignDefinition } from './campaign-compiler.js';
import { compileCampaignFile, campaignGradingQualification, validateCampaignDefinition } from './campaign-compiler.js';
import { writeCampaignRecord } from './campaign-lock.js';
import { submitExecutionJob } from './execution-jobs.js';
import { executionCredentialsSchema, listCredentialProfiles, resolveExecutionCredentials, validateExecutionCredentialTargets } from '../agents/credential-profiles.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { sha256 } from '../evidence/provenance.js';
import { resolveGuidanceProfile } from './condition-compiler.js';

const name = z.string().regex(/^[a-z0-9][a-z0-9.-]{2,119}$/);
const requestSchema = z.strictObject({
  key: name, workload: name, workloadSha256: z.string().regex(/^[a-f0-9]{64}$/), level: z.number().int().positive(),
  stacks: z.array(z.string()).min(1),
  agents: z.array(z.strictObject({ index: z.number().int().nonnegative(),
    effort: z.enum(['low', 'medium', 'high', 'xhigh', 'max']) })).min(1),
  conditions: z.array(z.string()).min(1),
  repetitions: z.number().int().positive(), parallelism: z.number().int().positive(),
  repairs: z.number().int().nonnegative(), timeoutMinutes: z.number().int().positive(),
  maxCostUsd: z.number().positive().finite(),
  productionQuality: z.boolean().default(true),
  pauseAfterDepth: z.number().int().positive().nullable(),
  credentials: executionCredentialsSchema.omit({ attempts: true }),
});
export type RunSetupRequest = z.infer<typeof requestSchema>;

// Presets are normal campaign manifests. They own workload, pricing, and runtime
// policy; this form only selects dimensions and limits within those definitions.
function presets(results: string) {
  const root = join(results, 'run-presets');
  const entries: { definition: CampaignDefinition }[] = [];
  const errors: string[] = [];
  if (!existsSync(root)) return { entries, errors };
  const seen = new Set<string>();
  for (const file of readdirSync(root).filter(file => file.endsWith('.json')).sort()) {
    try {
      const path = realpathSync(join(root, file));
      const child = relative(realpathSync(root), path);
      if (isAbsolute(child) || child === '..' || child.startsWith(`..${sep}`)) throw new Error('preset leaves its directory');
      const definition = validateCampaignDefinition(JSON.parse(readFileSync(path, 'utf8')));
      if (definition.state !== 'frozen') throw new Error('preset must have a frozen runtime and pricing');
      if (seen.has(definition.id)) {
        const previous = entries.findIndex(entry => entry.definition.id === definition.id);
        if (previous >= 0) entries.splice(previous, 1);
        throw new Error(`duplicate workload ${definition.id}`);
      }
      seen.add(definition.id);
      entries.push({ definition });
    } catch (error) { errors.push(`${file}: ${error instanceof Error ? error.message : String(error)}`); }
  }
  return { entries, errors };
}

export function runSetupCatalog(results: string, env: NodeJS.ProcessEnv = process.env) {
  const { entries, errors } = presets(results);
  return { workloads: entries.map(({ definition: d }) => ({
    id: d.id, sha256: sha256(canonicalDefinitionJson(d)), title: d.title, track: d.track, mode: d.mode.id,
    workSelection: d.mode.workSelection, levels: d.levels,
    stacks: d.stacks.map(stack => stack.id), agents: d.agents.map(a => ({ ...a, provider: AGENT_ADAPTER_REGISTRY.get(a.adapter).provider })),
    conditions: d.conditions.map(condition => {
      const skills = resolveGuidanceProfile(condition.guidanceProfile, d.stacks.map(s => s.id)).skills.spacetime?.ids ?? [];
      return { id: condition.id, guidance: condition.guidanceProfile,
        sdkSkills: skills.some(id => ['typescript-server', 'typescript-client', 'cli'].includes(id)),
        devWorkflow: skills.some(id => ['spacetime-dev', 'spacetime-managed-dev'].includes(id)) };
    }),
    defaults: { repetitions: d.repetitions, parallelism: d.parallelism,
      repairs: d.repair.budget.total ?? 0, timeoutMinutes: d.budgets.attemptTimeoutMinutes,
      maxCostUsd: d.budgets.maxCostUsdPerAttempt, pauseAfterDepth: d.mode.pauseAfterDepth ?? null },
  })), profiles: listCredentialProfiles(env), errors };
}
export type RunSetupCatalog = ReturnType<typeof runSetupCatalog>;

function unique<T>(items: T[], label: string): void {
  if (new Set(items).size !== items.length) throw new Error(`${label} contains duplicates`);
}

function writeOnce(path: string, value: unknown): void {
  try { writeCampaignRecord(path, value); }
  catch (error) {
    if (!(error instanceof Error) || !('code' in error) || error.code !== 'EEXIST') throw error;
    if (canonicalDefinitionJson(JSON.parse(readFileSync(path, 'utf8'))) !== canonicalDefinitionJson(value)) {
      throw new Error('saved setup differs from this request');
    }
  }
}

/** Shared by the CLI and dashboard. Preparing never starts a worker or calls a model. */
export function prepareRun(results: string, input: unknown, env: NodeJS.ProcessEnv = process.env) {
  const request = requestSchema.parse(input);
  unique(request.stacks, 'Stacks'); unique(request.agents.map(a => a.index), 'Models');
  unique(request.conditions, 'Guidance');
  const matches = presets(results).entries.filter(entry => entry.definition.id === request.workload);
  if (matches.length !== 1) throw new Error('Workload is unavailable. Reload setup.');
  const d = structuredClone(matches[0]!.definition);
  if (request.workloadSha256 !== sha256(canonicalDefinitionJson(d))) {
    throw new Error('Workload changed. Reload setup and review the new choices.');
  }
  if (!d.levels.includes(request.level)) throw new Error('Target level is not in this workload');
  const select = <T>(values: T[], keys: string[], key: (value: T) => string) => keys.map(id => {
    const value = values.find(value => key(value) === id);
    if (!value) throw new Error(`Selection is not in this workload: ${id}`);
    return value;
  });
  for (const [field, variable] of [['controllerImage', 'STACK_BENCH_CONTROLLER_IMAGE'], ['buildImage', 'STACK_BENCH_BUILD_IMAGE']] as const) {
    if (env[variable] && env[variable] !== d.runtime[field]) {
      throw new Error('Workload runtime does not match this appliance release. Update its preset before starting.');
    }
  }
  d.id = request.key; d.title = request.key;
  d.levels = d.levels.filter(level => level <= request.level);
  if (d.selection.levels) d.selection.levels = d.selection.levels.filter(level => d.levels.includes(level.level));
  d.stacks = select(d.stacks, request.stacks, stack => stack.id).map(stack => ({ ...stack, repetitions: request.repetitions }));
  d.agents = request.agents.map(({ index, effort }) => {
    const agent = d.agents[index];
    if (!agent) throw new Error('Model is not in this workload');
    return { ...agent, effort };
  });
  d.conditions = select(d.conditions, request.conditions, condition => condition.id);
  for (const condition of d.conditions) {
    condition.productionQuality = request.productionQuality;
    if (condition.specifications?.levels) {
      condition.specifications.levels = condition.specifications.levels.filter(level => d.levels.includes(level.level));
    }
  }
  d.repetitions = request.repetitions; d.parallelism = request.parallelism;
  d.repair = { ...d.repair, budget: { total: request.repairs } };
  d.budgets = { attemptTimeoutMinutes: request.timeoutMinutes, maxCostUsdPerAttempt: request.maxCostUsd };
  delete d.mode.pauseAfterDepth;
  if (request.pauseAfterDepth !== null) d.mode.pauseAfterDepth = request.pauseAfterDepth;
  d.ordering.seed = request.key;
  const plans = join(results, 'plans'); mkdirSync(plans, { recursive: true });
  const sourceId = sha256(canonicalDefinitionJson(d));
  const planFile = join(plans, `setup-${sourceId}.json`);
  writeOnce(planFile, d);
  const plan = compileCampaignFile(planFile);
  // Resolve profile metadata now, before a job can spend money. Secrets stay server-side.
  const profiles = listCredentialProfiles(env);
  for (const { adapter } of plan.agents) {
    if (request.credentials.default || request.credentials.adapters?.[adapter]) continue;
    const matches = profiles.filter(p => p.provider === AGENT_ADAPTER_REGISTRY.get(adapter).provider);
    if (matches.length > 1) throw new Error(`Choose an account for ${adapter} under Run name and accounts.`);
    if (matches.length === 1) {
      (request.credentials.adapters ??= {})[adapter] = matches[0]!.id;
    }
  }
  validateExecutionCredentialTargets(request.credentials, plan.agents.map(a => a.adapter), []);
  const authentication = plan.agents.map(agent => {
    const resolved = resolveExecutionCredentials(agent.adapter, '', request.credentials, env);
    const ready = credentialReady(AGENT_ADAPTER_REGISTRY.get(agent.adapter), resolved.env, homedir(), existsSync);
    if (!ready.ok) throw new Error(`${agent.adapter}: ${ready.reason ?? 'No configured account is available'}`);
    return { adapter: agent.adapter, profile: resolved.assignment, source: ready.kind ?? 'model-free' };
  });
  if (!Number.isFinite(request.maxCostUsd * plan.attempts.length)) throw new Error('Total cost cap is too large');
  const reviewId = sha256(canonicalDefinitionJson({ request, planSha256: plan.contentSha256, authentication }));
  return { request, reviewId, planFile, planSha256: plan.contentSha256,
    attempts: plan.attempts.length, parallelism: plan.summary.parallelism,
    maxCostUsd: request.maxCostUsd * plan.attempts.length,
    qualification: campaignGradingQualification(plan).status, authentication,
    mode: plan.definition.mode, runtime: plan.definition.runtime, pricing: plan.definition.pricing };
}
export type RunSetupReview = ReturnType<typeof prepareRun>;

export function submitPreparedRun(results: string, input: unknown, env: NodeJS.ProcessEnv = process.env) {
  const accepted = z.object({ request: requestSchema, reviewId: z.string().regex(/^[a-f0-9]{64}$/) }).parse(input);
  const review = prepareRun(results, accepted.request, env);
  if (review.reviewId !== accepted.reviewId) throw new Error('Setup changed. Review it again before starting.');
  return submitExecutionJob(results, { key: review.request.key, planFile: review.planFile,
    credentials: review.request.credentials, capacityPolicy: 'wait' });
}
