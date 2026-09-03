import { randomUUID } from 'node:crypto';
import { join, resolve } from 'node:path';
import { z } from 'zod';

import { AGENT_ADAPTER_REGISTRY } from '../agents/agent-adapters.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { DEFAULT_BUILD_IMAGE } from '../composition/product-config.js';
import { loadTrack, portsFor, RUN_INDEX_CAP } from '../composition/tracks.js';
import { emptyArtifactIdentities, readArtifact, writeArtifact } from '../evidence/artifacts.js';
import { existingResourceLockKeys, resourceLockScope,
  DEFAULT_SPACETIME_SERVER_URI, loopbackHttpUri } from '../runtime/backend-lease.js';
import { probeLoopbackPort, runPreflight } from '../runtime/preflight.js';
import type { PreflightReport } from '../runtime/preflight.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import type { CompiledCampaignPlan } from './campaign-compiler.js';
import type { RequestedScope } from './condition-compiler.js';
import { campaignChildPath as contained } from './campaign-path.js';
import { campaignExecutionEnvironment, campaignSlotEnvironment } from './campaign-runtime.js';
import { formatZodError } from '../zod-error.js';
import { listRunningCodingContainers } from '../../container/reconcile-build-container.js';

const SMOKE_REUSE_MS = 15 * 60_000;
type UnknownRecord = Record<string, unknown>;
type AdmissionStatus = 'pass' | 'warn' | 'fail';

interface CampaignAdmissionCheck {
  id: string;
  status: AdmissionStatus;
  summary: string;
}

interface CampaignAdmissionReportRequest extends UnknownRecord {
  agentAdapter: string;
  runIndex: number;
  backends: string[];
  image: unknown;
}

interface CampaignAdmissionReport extends UnknownRecord {
  schemaVersion: number;
  ok: boolean;
  request: CampaignAdmissionReportRequest;
  checks: CampaignAdmissionCheck[];
  summary: { passed: number; failed: number; warnings: number };
}

export interface CampaignAdmission extends UnknownRecord {
  schemaVersion: number;
  campaignId: string;
  campaignSha256: string;
  createdAt: string;
  ok: boolean;
  runtime: unknown;
  agents: unknown;
  conditions: unknown;
  reports: CampaignAdmissionReport[];
}

export interface CampaignAdmissionPreflightRequest extends UnknownRecord {
  backends: string[];
  track: string;
  levels: string;
  levelList: number[];
  runIndex: number;
  parallelism: number;
  agentAdapter: string;
  guidance: string;
  agentSkills: string[];
  packIds: string[];
  checkKeys: string[];
  requestedScopes: RequestedScope[];
  featureCatalog: CompiledCampaignPlan['featureCatalog'];
  mode: CompiledCampaignPlan['definition']['mode'];
  smoke: boolean;
  image: string;
  resultsDir: string;
}

export interface CampaignAdmissionResult {
  id: string;
  path: string;
  payload: CampaignAdmission;
  runIndices: number[];
}

interface CampaignAdmissionPlan {
  id: string;
  contentSha256: string;
  definition: {
    runtime: { buildImage: string | null };
    track: string;
    levels: unknown;
    selection: { packs?: unknown; checks?: unknown };
  };
  agents: Array<{ adapter: string; model: string; identity: unknown }>;
  conditions: unknown;
  stacks: Array<{ id: string }>;
  summary: { parallelism: number };
}

export interface CampaignAdmissionSmokeInput {
  ok: boolean;
  createdAt: string;
  reports: Array<{
    ok: boolean;
    request: {
      agentAdapter: string;
      runIndex: number;
      backends: string[];
      image: unknown;
    };
    checks: Array<{ id: string; status: string; summary?: string }>;
  }>;
}

export interface CampaignAdmissionSmokeRequest {
  agentAdapter: string;
  runIndex: number;
  backend: string;
  image: unknown;
}

export type CampaignAdmissionSmokeReuse =
  | { reusable: false; reason: string }
  | { reusable: true; reason: null; createdAt: string };

const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const admissionCheckSchema = z.looseObject({
  id: z.string(),
  status: z.enum(['pass', 'warn', 'fail']),
  summary: z.string(),
});
const admissionReportSchema = z.looseObject({
  schemaVersion: z.literal(1),
  ok: z.boolean(),
  request: z.looseObject({
    agentAdapter: z.string(),
    runIndex: z.number(),
    backends: z.array(z.string()),
    image: z.unknown(),
  }),
  checks: z.array(admissionCheckSchema),
  summary: z.looseObject({ passed: z.number(), failed: z.number(), warnings: z.number() }),
});
const campaignAdmissionSchema = z.strictObject({
  schemaVersion: z.literal(1),
  campaignId: z.string(),
  campaignSha256: z.string(),
  createdAt: z.iso.datetime(),
  ok: z.boolean(),
  runtime: z.unknown(),
  agents: z.unknown(),
  conditions: z.unknown(),
  reports: z.array(admissionReportSchema),
});

function validReport(value: unknown): value is CampaignAdmissionReport {
  const parsed = admissionReportSchema.safeParse(value);
  if (!parsed.success) return false;
  const { checks, summary } = parsed.data;
  return summary.passed === checks.filter(check => check.status === 'pass').length
    && summary.failed === checks.filter(check => check.status === 'fail').length
    && summary.warnings === checks.filter(check => check.status === 'warn').length
    && parsed.data.ok === !checks.some(check => check.status === 'fail');
}

export function validateCampaignAdmission(
  input: unknown,
  plan: CampaignAdmissionPlan,
  directory: string,
): CampaignAdmission {
  const parsed = campaignAdmissionSchema.safeParse(input);
  if (!parsed.success) throw new Error(formatZodError(parsed.error, 'campaign admission'));
  const admission = parsed.data;
  if (admission.campaignId !== plan.id || admission.campaignSha256 !== plan.contentSha256) {
    throw new Error('campaign admission identity or metadata is invalid');
  }
  if (canonicalDefinitionJson(admission.runtime) !== canonicalDefinitionJson(plan.definition.runtime)) {
    throw new Error('campaign admission runtime does not match the compiled plan');
  }
  const expectedAgents = plan.agents.map(agent => ({ adapter: agent.adapter, model: agent.model,
    identity: agent.identity }));
  if (canonicalDefinitionJson(admission.agents) !== canonicalDefinitionJson(expectedAgents)) {
    throw new Error('campaign admission agents do not match the compiled plan');
  }
  if (canonicalDefinitionJson(admission.conditions) !== canonicalDefinitionJson(plan.conditions)) {
    throw new Error('campaign admission conditions do not match the compiled plan');
  }
  const adapters = [...new Set(plan.agents.map(agent => agent.adapter))].sort();
  const runIndices = [...new Set(admission.reports.map(report => report.request.runIndex))]
    .sort((a, b) => a - b);
  if (runIndices.length !== plan.summary.parallelism
    || runIndices.some(index => !Number.isInteger(index) || index < 0 || index > RUN_INDEX_CAP)) {
    throw new Error('campaign admission run slots are incomplete or invalid');
  }
  if (admission.reports.length !== adapters.length * plan.summary.parallelism) {
    throw new Error('campaign admission reports are incomplete');
  }
  const expectedBackends = plan.stacks.map(stack => stack.id);
  const expectedResultsDir = resolve(directory);
  for (const adapter of adapters) {
    for (const runIndex of runIndices) {
      const matches = admission.reports.filter(report => report.request.agentAdapter === adapter
        && report.request.runIndex === runIndex);
      if (matches.length !== 1) {
        throw new Error(`campaign admission must contain one ${adapter} report for run slot ${runIndex}`);
      }
      const report = matches[0]!;
      if (!validReport(report)) throw new Error(`campaign admission report for ${adapter} is malformed`);
      const request = report.request;
      if (canonicalDefinitionJson(request.backends) !== canonicalDefinitionJson(expectedBackends)
        || request.track !== plan.definition.track
        || canonicalDefinitionJson(request.levels) !== canonicalDefinitionJson(plan.definition.levels)
        || request.runIndex !== runIndex
        || request.parallelism !== plan.summary.parallelism
        || canonicalDefinitionJson(request.packs)
          !== canonicalDefinitionJson(plan.definition.selection.packs ?? [])
        || canonicalDefinitionJson(request.checks)
          !== canonicalDefinitionJson(plan.definition.selection.checks ?? [])
        || request.smoke !== true
        || (plan.definition.runtime.buildImage !== null
          && request.image !== plan.definition.runtime.buildImage)
        || typeof request.resultsDir !== 'string'
        || resolve(request.resultsDir) !== expectedResultsDir) {
        throw new Error(`campaign admission report for ${adapter} does not match the compiled scope`);
      }
    }
  }
  if (admission.ok !== admission.reports.every(report => report.ok)) {
    throw new Error('campaign admission verdict does not match its reports');
  }
  return admission as CampaignAdmission;
}

// Until the cross-run isolation test passes, coding containers share one
// network: one coding run at a time on the runner, across every campaign,
// for the whole attempt from build through final grade. A stack-bench
// coding container that is still running belongs to such an attempt.
function refuseConcurrentCodingRuns(plan: CompiledCampaignPlan,
  running: readonly string[]): void {
  if (plan.summary.parallelism > 1) {
    throw new Error(`parallelism ${plan.summary.parallelism} is refused until the cross-run `
      + 'isolation test passes; compile the campaign with parallelism 1');
  }
  if (running.length) {
    throw new Error(`another coding run is active on this runner (${running.join(', ')}); `
      + 'one coding run at a time until the cross-run isolation test passes');
  }
}

function availableRunIndices(plan: CompiledCampaignPlan, env: NodeJS.ProcessEnv,
  probePort: (port: number | string) => { free: boolean }): number[] {
  const track = loadTrack(plan.definition.track);
  const lockRoot = resourceLockScope(env).root;
  const selected: number[] = [];
  for (let runIndex = 0; runIndex <= RUN_INDEX_CAP; runIndex += 1) {
    const lockKeys = plan.stacks.map(stack =>
      `slot:${plan.definition.track}:${stack.id}:run${runIndex}`);
    if (existingResourceLockKeys({ root: lockRoot, keys: lockKeys }).length) continue;
    const ports = new Set(plan.stacks.flatMap(stack => {
      const assigned = portsFor(track, stack.id, runIndex);
      return [assigned.vite, assigned.express].filter((port): port is number =>
        typeof port === 'number');
    }));
    if (plan.stacks.some(stack => stack.id === 'spacetime')) {
      const base = loopbackHttpUri(env.STACK_BENCH_STDB_URI ?? DEFAULT_SPACETIME_SERVER_URI);
      ports.add(Number(base.port) + runIndex);
    }
    if ([...ports].every(port => probePort(port).free)) selected.push(runIndex);
    if (selected.length === plan.summary.parallelism) return selected;
  }
  throw new Error(`only ${selected.length} of ${plan.summary.parallelism} required run slots are free`);
}

export function readCampaignAdmission(
  directory: string,
  id: string,
  plan: CampaignAdmissionPlan,
): CampaignAdmission {
  const path = contained(directory, join('admissions', `${id}.json`), 'campaign admission');
  const artifact = readArtifact(path, { expectedKind: 'campaign_admission', expectedId: id });
  const experiment = artifact.identities.experiment;
  if (!object(experiment) || experiment.sha256 !== plan.contentSha256) {
    throw new Error(`campaign admission ${id} has the wrong experiment identity`);
  }
  return validateCampaignAdmission(artifact.payload, plan, directory);
}

const RESOURCE_FREE_REQUIREMENTS = Object.freeze({
  docker: false,
  services: false,
  ports: false,
  credentials: false,
  providerAccess: false,
});

type AgentAdapter = ReturnType<typeof AGENT_ADAPTER_REGISTRY.get>;

function hasNoAgentResources(agent: NonNullable<AgentAdapter>): boolean {
  return agent.costLimit === 'non-billable'
    && agent.apiKeyEnvironmentVariable === null
    && agent.credentialEnvironmentVariables.length === 0
    && agent.credentialFiles.length === 0
    && agent.outboundDestinations.length === 0
    && agent.requiredExecutables.length === 0
    && agent.credentialStatusCommand === null;
}

function hasNoStackResources(stack: CompiledCampaignPlan['stacks'][number]): boolean {
  const adapter = STACK_ADAPTER_REGISTRY.get(stack.id);
  if (!('admission' in adapter)) return false;
  return canonicalDefinitionJson(adapter.admission.requirements)
    === canonicalDefinitionJson(RESOURCE_FREE_REQUIREMENTS);
}

export function campaignUsesNoExternalResources(plan: CompiledCampaignPlan): boolean {
  return plan.stacks.every(hasNoStackResources)
    && plan.agents.every(agent => {
      const adapter = AGENT_ADAPTER_REGISTRY.get(agent.adapter);
      return adapter ? hasNoAgentResources(adapter) : false;
    });
}

function resourceFreeAdmissionReport(request: CampaignAdmissionPreflightRequest,
  generatedAt: string): PreflightReport {
  return {
    schemaVersion: 1,
    generatedAt,
    request: {
      backends: request.backends,
      track: request.track,
      levels: request.levelList,
      runIndex: request.runIndex,
      parallelism: request.parallelism,
      agentAdapter: request.agentAdapter,
      guidance: request.guidance,
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

export function runCampaignAdmission(plan: CompiledCampaignPlan, directory: string,
  { env = process.env, preflight = runPreflight, now = new Date().toISOString(),
    uuid = randomUUID, probePort = probeLoopbackPort,
    codingContainers = listRunningCodingContainers }: {
      env?: NodeJS.ProcessEnv;
      codingContainers?: () => string[];
      preflight?: (request: CampaignAdmissionPreflightRequest,
        options?: { env: NodeJS.ProcessEnv }) => PreflightReport;
      now?: string;
      uuid?: () => string;
      probePort?: (port: number | string) => { free: boolean };
    } = {}): CampaignAdmissionResult {
  const executionEnv = campaignExecutionEnvironment(plan, env);
  const reports: PreflightReport[] = [];
  const resourceFree = campaignUsesNoExternalResources(plan);
  if (!resourceFree) refuseConcurrentCodingRuns(plan, codingContainers());
  const runIndices = resourceFree
    ? Array.from({ length: plan.summary.parallelism }, (_, index) => index)
    : availableRunIndices(plan, executionEnv, probePort);
  const guidanceModes = [...new Set(plan.conditions.map(condition => condition.guidance.mode))];
  const agentSkills = [...new Set(plan.attempts.flatMap(attempt => attempt.skills))].sort();
  for (const adapter of [...new Set(plan.agents.map(agent => agent.adapter))].sort()) {
    for (const runIndex of runIndices) {
      const request: CampaignAdmissionPreflightRequest = {
        backends: plan.stacks.map(stack => stack.id),
        track: plan.definition.track,
        levels: `${Math.min(...plan.definition.levels)}-${Math.max(...plan.definition.levels)}`,
        levelList: plan.definition.levels,
        runIndex,
        parallelism: plan.summary.parallelism,
        agentAdapter: adapter,
        guidance: guidanceModes.length === 1 ? guidanceModes[0]! : 'mixed',
        agentSkills,
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
      reports.push(resourceFree ? resourceFreeAdmissionReport(request, now) : preflight(request,
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
  return { id, path, payload, runIndices };
}

export function campaignAdmissionSmokeReuse(
  admission: CampaignAdmissionSmokeInput,
  request: CampaignAdmissionSmokeRequest,
  { now = Date.now(), maxAgeMs = SMOKE_REUSE_MS }: { now?: number; maxAgeMs?: number } = {},
): CampaignAdmissionSmokeReuse {
  if (admission.ok !== true) throw new Error('campaign admission did not pass');
  const reports = admission.reports.filter(report =>
    report.request.agentAdapter === request.agentAdapter
    && report.request.runIndex === request.runIndex);
  if (reports.length !== 1) throw new Error('campaign admission has no exact attempt report');
  const report = reports[0]!;
  if (report.ok !== true) throw new Error('campaign admission attempt report did not pass');
  if (!report.request.backends.includes(request.backend)) {
    throw new Error(`campaign admission does not cover stack ${request.backend}`);
  }
  if (report.request.image !== request.image) {
    return { reusable: false, reason: 'build image changed after campaign admission' };
  }
  const createdAt = Date.parse(admission.createdAt);
  const ageMs = now - createdAt;
  if (!Number.isFinite(ageMs) || ageMs < -60_000 || ageMs > maxAgeMs) {
    return { reusable: false, reason: 'campaign admission is not recent' };
  }
  const smoke = report.checks.find(check => check.id === 'smoke.container');
  if (smoke?.status !== 'pass') {
    return { reusable: false, reason: 'campaign admission has no passing container smoke check' };
  }
  return { reusable: true, reason: null, createdAt: admission.createdAt };
}
