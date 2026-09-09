import { setImmediate as yieldTurn } from 'node:timers/promises';
import { randomUUID } from 'node:crypto';
import { existsSync, readdirSync } from 'node:fs';
import { join, posix, resolve, win32 } from 'node:path';
import { z } from 'zod';

import { AGENT_ADAPTER_REGISTRY } from '../agents/agent-adapters.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { DEFAULT_BUILD_IMAGE } from '../composition/product-config.js';
import { loadTrack, portsFor, RUN_INDEX_CAP } from '../composition/tracks.js';
import { emptyArtifactIdentities, readArtifact, writeArtifact } from '../evidence/artifacts.js';
import { claimBackendResources, createBackendLease, runResourceLockKeys,
  readBackendLease, writeBackendLease, releaseResourceLocks, verifyResourceLocks, resourceLockScope,
  loopbackHttpUri, existingResourceLockKeys } from '../runtime/backend-lease.js';
import type { BackendLease } from '../runtime/backend-lease.js';
import { releaseBackendLease } from '../runtime/backend-teardown.js';
import { probeLoopbackPort, runPreflight } from '../runtime/preflight.js';
import type { PreflightReport } from '../runtime/preflight.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import type { CampaignAttemptPlan, CompiledCampaignPlan } from './campaign-compiler.js';
import type { RequestedScope } from './condition-compiler.js';
import { campaignChildPath as contained } from './campaign-path.js';
import { campaignExecutionEnvironment, campaignSlotEnvironment } from './campaign-runtime.js';
import { formatZodError } from '../zod-error.js';

type UnknownRecord = Record<string, unknown>;
type AdmissionStatus = 'pass' | 'warn' | 'fail';

interface CampaignAdmissionCheck {
  id: string;
  status: AdmissionStatus;
  summary: string;
}

interface CampaignAdmissionReportRequest extends UnknownRecord {
  agentAdapter: string;
  providerRoute?: string;
  maxOutputTokens?: number;
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
  attemptId?: string;
}

export interface CampaignAdmissionPreflightRequest extends UnknownRecord {
  backends: string[];
  track: string;
  levels: string;
  levelList: number[];
  runIndex: number;
  parallelism: number;
  agentAdapter: string;
  providerRoute?: string;
  maxOutputTokens?: number;
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
  reservation?: CampaignReservation;
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
  agents: Array<{ adapter: string; model: string; providerRoute?: string; maxOutputTokens?: number; identity: unknown }>;
  conditions: unknown;
  stacks: Array<{ id: string }>;
  summary: { parallelism: number };
  attempts?: CampaignAttemptPlan[];
}

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
    providerRoute: z.string().optional(),
    maxOutputTokens: z.number().int().positive().optional(),
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
  attemptId: z.string().optional(),
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

function admissionSelections(agents: CampaignAdmissionPlan['agents']) {
  return [...new Map(agents.map(({ adapter, providerRoute, maxOutputTokens }) => {
    const selection = { adapter, ...(providerRoute ? { providerRoute } : {}),
      ...(maxOutputTokens ? { maxOutputTokens } : {}) };
    return [canonicalDefinitionJson(selection), selection] as const;
  })).entries()].sort(([left], [right]) => left.localeCompare(right)).map(([, selection]) => selection);
}

export function validateCampaignAdmission(
  input: unknown,
  plan: CampaignAdmissionPlan,
  directory: string,
  { allowRelocatedEvidence = false }: { allowRelocatedEvidence?: boolean } = {},
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
    ...(agent.providerRoute ? { providerRoute: agent.providerRoute } : {}),
    ...(agent.maxOutputTokens ? { maxOutputTokens: agent.maxOutputTokens } : {}), identity: agent.identity }));
  if (canonicalDefinitionJson(admission.agents) !== canonicalDefinitionJson(expectedAgents)) {
    throw new Error('campaign admission agents do not match the compiled plan');
  }
  if (canonicalDefinitionJson(admission.conditions) !== canonicalDefinitionJson(plan.conditions)) {
    throw new Error('campaign admission conditions do not match the compiled plan');
  }
  const attempt = admission.attemptId === undefined ? undefined
    : plan.attempts?.find(candidate => candidate.id === admission.attemptId);
  if (admission.attemptId !== undefined && !attempt) throw new Error('campaign admission attempt is invalid');
  const selectedAgents = attempt ? plan.agents.filter(agent => agent.adapter === attempt.agentAdapter
    && agent.model === attempt.model && agent.providerRoute === attempt.providerRoute
    && agent.maxOutputTokens === attempt.maxOutputTokens) : plan.agents;
  const selections = admissionSelections(selectedAgents);
  const workerCount = attempt ? 1 : plan.summary.parallelism;
  const runIndices = [...new Set(admission.reports.map(report => report.request.runIndex))]
    .sort((a, b) => a - b);
  if (runIndices.length !== workerCount
    || runIndices.some(index => !Number.isInteger(index) || index < 0 || index > RUN_INDEX_CAP)) {
    throw new Error('campaign admission run slots are incomplete or invalid');
  }
  if (admission.reports.length !== selections.length * workerCount) {
    throw new Error('campaign admission reports are incomplete');
  }
  const expectedBackends = attempt ? [attempt.stack] : plan.stacks.map(stack => stack.id);
  const expectedResultsDir = resolve(directory);
  const recordedResultsDir = admission.reports[0]?.request.resultsDir;
  for (const { adapter, providerRoute, maxOutputTokens } of selections) {
    for (const runIndex of runIndices) {
      const matches = admission.reports.filter(report => report.request.agentAdapter === adapter
        && report.request.providerRoute === providerRoute
        && report.request.maxOutputTokens === maxOutputTokens
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
        || request.smoke !== false
        || (plan.definition.runtime.buildImage !== null
          && request.image !== plan.definition.runtime.buildImage)
        || typeof request.resultsDir !== 'string'
        || (allowRelocatedEvidence
          ? request.resultsDir !== recordedResultsDir
            || !(posix.isAbsolute(request.resultsDir) || win32.isAbsolute(request.resultsDir))
          : resolve(request.resultsDir) !== expectedResultsDir)) {
        throw new Error(`campaign admission report for ${adapter} does not match the compiled scope`);
      }
    }
  }
  if (admission.ok !== admission.reports.every(report => report.ok)) {
    throw new Error('campaign admission verdict does not match its reports');
  }
  return admission as CampaignAdmission;
}

export class CampaignResourceUnavailable extends Error {}

export interface CampaignReservation { path: string; token: string }

interface ReservationLease extends BackendLease {
  campaign: { sha256: string; admissionId: string; runIndices: number[] };
}
interface DelegationLease extends BackendLease {
  delegation: { parent: CampaignReservation; campaignSha256: string; admissionId: string;
    executionId: string; output: string; backend: string; runIndex: number };
  childLeasePath?: string;
  childRunId?: string;
  childOwnershipToken?: string;
}

function reservationLease(authority: CampaignReservation): ReservationLease {
  const lease = readBackendLease(authority.path, { token: authority.token });
  if (!('campaign' in lease) || !object(lease.campaign)
    || typeof lease.campaign.sha256 !== 'string'
    || typeof lease.campaign.admissionId !== 'string'
    || !Array.isArray(lease.campaign.runIndices)) throw new Error('invalid campaign reservation');
  if (lease.state === 'released') throw new Error('campaign reservation was released');
  return lease as ReservationLease;
}

async function reserveRunIndices(plan: CompiledCampaignPlan, directory: string, id: string,
  env: NodeJS.ProcessEnv, probePort: (port: number | string) => { free: boolean },
  excludedRunIndices: readonly number[] = [], signal?: AbortSignal,
): Promise<{ runIndices: number[]; reservation: CampaignReservation }> {
  const track = loadTrack(plan.definition.track);
  const scope = resourceLockScope(env);
  const path = contained(directory, join('.private', `${id}.reservation.json`), 'campaign reservation');
  // Selection is a hint. The kernel-locked claim below admits the complete set
  // or none of it. A competing campaign can win between selection and claim.
  for (let retry = 0; retry <= RUN_INDEX_CAP; retry += 1) {
    const selected: number[] = [];
    const keys: string[] = [];
    const selectedKeys = new Set<string>();
    for (let runIndex = 0; runIndex <= RUN_INDEX_CAP; runIndex += 1) {
      // Yield bounded scan batches so cancellation and other campaign workers can run.
      if (runIndex % 16 === 0) await yieldTurn(undefined, { signal });
      if (excludedRunIndices.includes(runIndex)) continue;
      let candidateKeys: string[];
      let ports: Set<number>;
      try {
        const slotEnv = campaignSlotEnvironment(env, 'spacetime', runIndex);
        ports = new Set(plan.stacks.flatMap(stack => {
          const assigned = portsFor(track, stack.id, runIndex);
          return [assigned.vite, assigned.express].filter((port): port is number => typeof port === 'number');
        }));
        if (plan.stacks.some(stack => stack.id === 'spacetime')) {
          ports.add(Number(loopbackHttpUri(slotEnv.STACK_BENCH_STDB_URI!).port));
        }
        candidateKeys = plan.stacks.flatMap(stack => runResourceLockKeys({
          track: plan.definition.track, backend: stack.id, runIndex,
          ports: portsFor(track, stack.id, runIndex),
          serverUri: stack.id === 'spacetime' ? slotEnv.STACK_BENCH_STDB_URI : null,
        }));
      } catch (error) {
        if (error instanceof RangeError) continue;
        throw error;
      }
      if (candidateKeys.some(key => selectedKeys.has(key))
        || existingResourceLockKeys({ ...scope, keys: candidateKeys }).length
        || ![...ports].every(port => probePort(port).free)) continue;
      selected.push(runIndex);
      keys.push(...candidateKeys);
      for (const key of candidateKeys) selectedKeys.add(key);
      if (selected.length === plan.summary.parallelism) break;
    }
    if (selected.length !== plan.summary.parallelism) {
      throw new CampaignResourceUnavailable(`only ${selected.length} of ${plan.summary.parallelism} required run slots are free`);
    }
    const lease: ReservationLease = { ...createBackendLease({ runId: id, backend: 'stub',
      track: plan.definition.track, runIndex: selected[0]! }),
      campaign: { sha256: plan.contentSha256, admissionId: id, runIndices: selected } };
    try {
      claimBackendResources(path, lease, { ...scope, keys });
      return { runIndices: selected, reservation: { path, token: lease.ownershipToken } };
    } catch (error) {
      // Do not overwrite private intent until every possible claim is released.
      releaseResourceLocks(lease);
      lease.state = 'released';
      writeBackendLease(path, lease);
      if (!existingResourceLockKeys({ ...scope, keys }).length) throw error;
    }
  }
  throw new CampaignResourceUnavailable('runner slots changed during admission; retry when capacity is available');
}

export function delegateCampaignReservation(authority: CampaignReservation, directory: string,
  input: { campaignSha256: string; admissionId: string; executionId: string; output: string;
    backend: string; runIndex: number }): NodeJS.ProcessEnv {
  const parent = reservationLease(authority);
  verifyResourceLocks(parent);
  if (parent.campaign.sha256 !== input.campaignSha256
    || parent.campaign.admissionId !== input.admissionId
    || !parent.campaign.runIndices.includes(input.runIndex)
    || !parent.resources.locks.some(lock => lock.key ===
      `slot:${parent.track}:${input.backend}:run${input.runIndex}`)) {
    throw new Error('campaign delegation does not match its reservation');
  }
  const privateDirectory = contained(directory, '.private', 'campaign authority');
  for (const name of readdirSync(privateDirectory).filter(name => name.endsWith('.delegation.json'))) {
    const existing = readBackendLease(join(privateDirectory, name)) as DelegationLease;
    if (existing.delegation?.parent.path === authority.path
      && existing.delegation.runIndex === input.runIndex && existing.state !== 'released') {
      throw new Error(`campaign worker ${input.runIndex} already has an active delegation`);
    }
  }
  const path = contained(directory, join('.private', `${input.executionId}.delegation.json`),
    'campaign delegation');
  if (existsSync(path)) throw new Error('campaign execution delegation already exists');
  const delegation: DelegationLease = { ...createBackendLease({ runId: input.executionId,
    backend: 'stub', track: parent.track, runIndex: input.runIndex }),
    delegation: { ...input, parent: authority } };
  writeBackendLease(path, delegation, { exclusive: true });
  return { STACK_BENCH_CAMPAIGN_DELEGATION: path,
    STACK_BENCH_CAMPAIGN_DELEGATION_TOKEN: delegation.ownershipToken,
    STACK_BENCH_CAMPAIGN_EXECUTION: input.executionId };
}

export function borrowCampaignReservation(input: {
  env: NodeJS.ProcessEnv; campaignSha256: string; admissionId: string; executionId: string; output: string;
  leasePath: string; lease: BackendLease; keys: string[];
}): boolean {
  const path = input.env.STACK_BENCH_CAMPAIGN_DELEGATION;
  const token = input.env.STACK_BENCH_CAMPAIGN_DELEGATION_TOKEN;
  if (!path && !token) return false;
  if (!path || !token) throw new Error('campaign delegation path and token are required');
  const document = readBackendLease(path, { token }) as DelegationLease;
  const delegated = document.delegation;
  if (!delegated || delegated.campaignSha256 !== input.campaignSha256
    || delegated.admissionId !== input.admissionId || delegated.executionId !== input.executionId
    || delegated.output !== resolve(input.output)
    || delegated.backend !== input.lease.backend || delegated.runIndex !== input.lease.runIndex
    || document.state !== 'created') throw new Error('campaign delegation identity does not match');
  const parent = reservationLease(delegated.parent);
  if (parent.campaign.sha256 !== input.campaignSha256
    || parent.campaign.admissionId !== input.admissionId
    || parent.track !== input.lease.track
    || input.keys.some(key => !parent.resources.locks.some(lock => lock.key === key))) {
    throw new Error('campaign reservation does not cover child resources');
  }
  verifyResourceLocks(parent);
  // The child never owns parent locks. Persist its recovery target before consume.
  input.lease.resources.locks = [];
  input.lease.campaignDelegation = { path, token };
  writeBackendLease(input.leasePath, input.lease);
  try { writeBackendLease(`${path}.used`, { ...document, childLeasePath: input.leasePath,
    childRunId: input.lease.runId, childOwnershipToken: input.lease.ownershipToken }, { exclusive: true }); }
  catch (error) { throw new Error('campaign delegation was already consumed or cannot be claimed', { cause: error }); }
  return true;
}

export function closeCampaignDelegation(directory: string, executionId: string): void {
  const path = contained(directory, join('.private', `${executionId}.delegation.json`),
    'campaign delegation');
  if (!existsSync(path)) return;
  const document = readBackendLease(path) as DelegationLease;
  if (existsSync(`${path}.used`)) {
    const consumed = readBackendLease(`${path}.used`, { token: document.ownershipToken }) as DelegationLease;
    if (!consumed.childLeasePath || !consumed.childRunId || !consumed.childOwnershipToken
      || readBackendLease(consumed.childLeasePath, { token: consumed.childOwnershipToken,
        runId: consumed.childRunId }).state !== 'released') {
      throw new Error(`campaign child ${executionId} cleanup is not proven`);
    }
  }
  document.state = 'released';
  writeBackendLease(path, document);
}

export function releaseCampaignReservation(authority: CampaignReservation): void {
  const lease = reservationLease(authority);
  const directory = resolve(authority.path, '..');
  for (const name of readdirSync(directory).filter(name => name.endsWith('.delegation.json'))) {
    const delegation = readBackendLease(join(directory, name)) as DelegationLease;
    if (delegation.delegation?.parent.path === authority.path && delegation.state !== 'released') {
      throw new Error(`campaign reservation retains unresolved child ${delegation.runId}`);
    }
  }
  releaseResourceLocks(lease);
  lease.state = 'released';
  writeBackendLease(authority.path, lease);
}

/** Called only while reconciliation owns the campaign lock. */
export function recoverCampaignReservations(directory: string, plan: CompiledCampaignPlan): number {
  const privateDirectory = contained(directory, '.private', 'campaign authority');
  if (!existsSync(privateDirectory)) return 0;
  let recovered = 0;
  const names = readdirSync(privateDirectory);
  for (const name of names.filter(name => name.endsWith('.reservation.json'))) {
    const path = join(privateDirectory, name);
    const lease = readBackendLease(path) as ReservationLease;
    if (lease.state === 'released') continue;
    if (lease.campaign?.sha256 !== plan.contentSha256) {
      throw new Error('retained campaign reservation has a different plan identity');
    }
    for (const childName of names.filter(name => name.endsWith('.delegation.json'))) {
      const childPath = join(privateDirectory, childName);
      const child = readBackendLease(childPath) as DelegationLease;
      if (child.delegation?.parent.path !== path || child.state === 'released') continue;
      if (existsSync(`${childPath}.used`)) {
        const consumed = readBackendLease(`${childPath}.used`, { token: child.ownershipToken }) as DelegationLease;
        if (!consumed.childLeasePath || !consumed.childRunId || !consumed.childOwnershipToken) {
          throw new Error('consumed campaign delegation has no child recovery authority');
        }
        readBackendLease(consumed.childLeasePath,
          { token: consumed.childOwnershipToken, runId: consumed.childRunId });
        if (!releaseBackendLease(consumed.childLeasePath, consumed.childOwnershipToken)) {
          throw new Error(`campaign child ${child.runId} cleanup is incomplete`);
        }
      }
      closeCampaignDelegation(directory, child.runId);
    }
    releaseCampaignReservation({ path, token: lease.ownershipToken });
    recovered += 1;
  }
  return recovered;
}

export function readCampaignAdmission(
  directory: string,
  id: string,
  plan: CampaignAdmissionPlan,
  options: { allowRelocatedEvidence?: boolean } = {},
): CampaignAdmission {
  const path = contained(directory, join('admissions', `${id}.json`), 'campaign admission');
  const artifact = readArtifact(path, { expectedKind: 'campaign_admission', expectedId: id });
  const experiment = artifact.identities.experiment;
  if (!object(experiment) || experiment.sha256 !== plan.contentSha256) {
    throw new Error(`campaign admission ${id} has the wrong experiment identity`);
  }
  return validateCampaignAdmission(artifact.payload, plan, directory, options);
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
      ...(request.providerRoute ? { providerRoute: request.providerRoute } : {}),
      ...(request.maxOutputTokens ? { maxOutputTokens: request.maxOutputTokens } : {}),
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

export async function runCampaignAdmission(plan: CompiledCampaignPlan, directory: string,
  { env = process.env, preflight = runPreflight, now = new Date().toISOString(),
    uuid = randomUUID, probePort = probeLoopbackPort, attempt, excludedRunIndices = [], signal }: {
      env?: NodeJS.ProcessEnv;
      attempt?: CampaignAttemptPlan;
      excludedRunIndices?: readonly number[];
      signal?: AbortSignal;
      preflight?: (request: CampaignAdmissionPreflightRequest,
        options?: { env: NodeJS.ProcessEnv }) => PreflightReport;
      now?: string;
      uuid?: () => string;
      probePort?: (port: number | string) => { free: boolean };
    } = {}): Promise<CampaignAdmissionResult> {
  signal?.throwIfAborted();
  const executionEnv = campaignExecutionEnvironment(plan, env);
  const reports: PreflightReport[] = [];
  const id = `${plan.id}-admission-${now.replace(/[-:.TZ]/g, '').slice(0, 14)}-${uuid()}`;
  if (attempt && !plan.attempts.some(candidate => canonicalDefinitionJson(candidate) === canonicalDefinitionJson(attempt))) {
    throw new Error('admission attempt does not match the compiled plan');
  }
  const scoped: CompiledCampaignPlan = attempt ? { ...plan,
    stacks: plan.stacks.filter(stack => stack.id === attempt.stack),
    agents: plan.agents.filter(agent => agent.adapter === attempt.agentAdapter && agent.model === attempt.model
      && agent.providerRoute === attempt.providerRoute && agent.maxOutputTokens === attempt.maxOutputTokens),
    attempts: [attempt], conditions: [attempt.condition], summary: { ...plan.summary, parallelism: 1 } } : plan;
  const resourceFree = campaignUsesNoExternalResources(scoped);
  const reserved = resourceFree ? null
    : await reserveRunIndices(scoped, directory, id, executionEnv, probePort, excludedRunIndices, signal);
  const runIndices = reserved?.runIndices
    ?? Array.from({ length: scoped.summary.parallelism + excludedRunIndices.length }, (_, index) => index)
      .filter(index => !excludedRunIndices.includes(index)).slice(0, scoped.summary.parallelism);
  try {
  await yieldTurn(undefined, { signal });
  const guidanceModes = [...new Set(scoped.conditions.map(condition => condition.guidance.mode))];
  const agentSkills = [...new Set(scoped.attempts.flatMap(attempt => attempt.skills))].sort();
  for (const { adapter, providerRoute, maxOutputTokens } of admissionSelections(scoped.agents)) {
    for (const runIndex of runIndices) {
      await yieldTurn(undefined, { signal });
      const request: CampaignAdmissionPreflightRequest = {
        backends: scoped.stacks.map(stack => stack.id),
        track: plan.definition.track,
        levels: `${Math.min(...plan.definition.levels)}-${Math.max(...plan.definition.levels)}`,
        levelList: plan.definition.levels,
        runIndex,
        parallelism: plan.summary.parallelism,
        agentAdapter: adapter,
        ...(providerRoute ? { providerRoute } : {}),
        ...(maxOutputTokens ? { maxOutputTokens } : {}),
        guidance: guidanceModes.length === 1 ? guidanceModes[0]! : 'mixed',
        agentSkills,
        packIds: plan.definition.selection.packs ?? [],
        checkKeys: plan.definition.selection.checks ?? [],
        requestedScopes: scoped.conditions.map(condition => condition.requested),
        featureCatalog: plan.featureCatalog,
        mode: plan.definition.mode,
        smoke: false,
        image: plan.definition.runtime.buildImage ?? executionEnv.STACK_BENCH_IMAGE
          ?? DEFAULT_BUILD_IMAGE,
        resultsDir: resolve(directory),
      };
      reports.push(resourceFree ? resourceFreeAdmissionReport(request, now) : preflight(request,
        { env: campaignSlotEnvironment(executionEnv,
          scoped.stacks.some(stack => stack.id === 'spacetime') ? 'spacetime' : null, runIndex) }));
    }
  }
  await yieldTurn(undefined, { signal });
  const payload = validateCampaignAdmission({ schemaVersion: 1, campaignId: plan.id,
    campaignSha256: plan.contentSha256, createdAt: now,
    ok: reports.every(report => report.ok),
    runtime: plan.definition.runtime,
    agents: plan.agents.map(agent => ({ adapter: agent.adapter, model: agent.model,
      ...(agent.providerRoute ? { providerRoute: agent.providerRoute } : {}),
      ...(agent.maxOutputTokens ? { maxOutputTokens: agent.maxOutputTokens } : {}), identity: agent.identity })),
    conditions: plan.conditions, ...(attempt ? { attemptId: attempt.id } : {}),
    reports }, plan, directory);
  const path = contained(directory, join('admissions', `${id}.json`), 'campaign admission');
  writeArtifact(path, { kind: 'campaign_admission', id,
    identities: emptyArtifactIdentities({ experiment: {
      id: plan.id, version: plan.version, sha256: plan.contentSha256, state: plan.state,
    } }), payload });
  if (!payload.ok && reserved) releaseCampaignReservation(reserved.reservation);
  return { id, path, payload, runIndices, ...(payload.ok && reserved
    ? { reservation: reserved.reservation } : {}) };
  } catch (error) {
    if (reserved) {
      try { releaseCampaignReservation(reserved.reservation); }
      catch (cleanupError) { throw new AggregateError([error, cleanupError],
        'admission failed; reservation retained for authenticated recovery'); }
    }
    throw error;
  }
}
