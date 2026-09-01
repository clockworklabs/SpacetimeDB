import { cpSync, existsSync, lstatSync, mkdirSync, readdirSync, renameSync, rmSync }
  from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../progression/progression-definition.js';
import type { CompiledDependencyPolicyDefinition, CompiledProgressionDefinition,
  ProgressionInput } from '../progression/progression-definition.js';
import { grantProgressionState, readProgressionState }
  from '../progression/progression-state.js';
import type { ProgressionOwner } from '../progression/progression-state.js';
import { acquireCampaignLock, releaseCampaignLock } from './campaign-lock.js';
import { campaignProgressionOwner } from './campaign-compiler.js';
import { inspectCampaign } from './campaign-runner.js';
import { scheduleDependencyContinuation, writeCampaignState }
  from './campaign-scheduler.js';

export interface CampaignDependencyStrikeGrant {
  attemptId: string;
  grantId: string;
  level: number;
  nodeIds: string[];
  strikes: number;
}

export interface GrantWorkspace {
  directory: string;
  relativePath: string;
  created: boolean;
}

interface GrantContinuation extends Omit<CampaignDependencyStrikeGrant, 'attemptId'> {
  snapshotSha256: string;
  resumeFrom: string;
  scheduledAt?: string;
}

interface GrantAttemptPlan {
  id: string;
  stack: string;
  agentAdapter: string;
  model: string;
  condition: { sha256: string };
  mode?: { id?: string };
}

interface GrantExecution {
  output: string;
  status: string;
  continuation?: GrantContinuation;
}

interface GrantCampaignSnapshot {
  plan: {
    id: string;
    version: string;
    contentSha256: string;
    definition: { mode?: { id?: string }; track: string };
    featureCatalog: ProgressionInput<CompiledProgressionDefinition> | null;
    dependencyPolicy: ProgressionInput<CompiledDependencyPolicyDefinition> | null;
  };
  state: {
    status: string;
    attempts: Array<{
      plan: GrantAttemptPlan;
      status: string;
      executions: GrantExecution[];
    }>;
  };
  paths: { state: string };
}

interface StoredProgressionState {
  state: {
    phase?: string;
    grants: Array<Omit<CampaignDependencyStrikeGrant, 'attemptId'>>;
  };
  snapshotSha256: string;
}

interface ProgressionContextOptions extends Record<string, unknown> {
  owner: ProgressionOwner;
}

interface ProgressionGrantOptions extends ProgressionContextOptions {
  grant: Omit<CampaignDependencyStrikeGrant, 'attemptId'>;
  checkpoint: { artifact: string };
  expectedSnapshotSha256: string;
}

interface ProgressionGrantResult {
  snapshotSha256: string;
}

interface GrantOptions {
  inspect?: (directory: string, options: { requireCurrentInputs: boolean }) =>
    GrantCampaignSnapshot;
  readState?: (path: string, options: ProgressionContextOptions) => StoredProgressionState;
  grantState?: (path: string, options: ProgressionGrantOptions) => ProgressionGrantResult;
  schedule?: (state: GrantCampaignSnapshot['state'], attemptId: string, continuation: GrantContinuation,
    options: { now: string }) => unknown;
  writeState?: (path: string, plan: unknown, state: unknown) => unknown;
  acquire?: (directory: string, plan: { id: string; contentSha256: string }) => unknown;
  release?: (lock: unknown) => unknown;
  prepareWorkspace?: typeof prepareGrantWorkspace;
  now?: string;
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function childPath(root: string, input: string, label: string): string {
  const absoluteRoot = resolve(root);
  const absolute = resolve(absoluteRoot, input);
  const relation = relative(absoluteRoot, absolute);
  if (!relation || relation === '..' || relation.startsWith(`..${sep}`)) {
    throw new Error(`${label} is outside the campaign directory`);
  }
  return absolute;
}

const SAFE_ID = /^[a-z0-9][a-z0-9.-]*$/;
const FEATURE_ID = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;

function rejectSymlinks(path: string, label: string): void {
  const stat = lstatSync(path);
  if (stat.isSymbolicLink()) throw new Error(`${label} cannot be a symbolic link`);
  if (!stat.isDirectory()) return;
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`${label} contains a symbolic link`);
    if (entry.isDirectory()) rejectSymlinks(child, label);
  }
}

export function prepareGrantWorkspace(root: string, executionDirectory: string,
  attemptId: string, grantId: string, level: number): GrantWorkspace {
  if (!SAFE_ID.test(attemptId) || !SAFE_ID.test(grantId)) {
    throw new Error('grant workspace requires safe attempt and grant ids');
  }
  if (!Number.isSafeInteger(level) || level < 1) {
    throw new Error('grant workspace requires a positive level');
  }
  const relativePath = `continuations/${attemptId}/${grantId}`;
  const target = childPath(root, relativePath, 'grant workspace');
  if (existsSync(target)) {
    rejectSymlinks(target, 'grant workspace');
    return { directory: target, relativePath, created: false };
  }
  mkdirSync(dirname(target), { recursive: true });
  const temporary = `${target}.tmp-${process.pid}-${Date.now()}`;
  const required = new Set(['source', 'progression-state.json', 'run.json', 'progression',
    `level-l${level}-checkpoint.json`, `level-l${level}-source`]);
  for (const name of required) {
    const source = join(executionDirectory, name);
    if (!existsSync(source)) throw new Error(`completed campaign execution has no ${name}`);
    rejectSymlinks(source, `completed campaign execution ${name}`);
  }
  try {
    cpSync(executionDirectory, temporary, { recursive: true, errorOnExist: true,
      filter: source => {
        const rel = relative(executionDirectory, source);
        if (!rel) return true;
        const [top] = rel.split(/[\\/]/);
        return required.has(top!) && !/[\\/]media([\\/]|$)/.test(rel);
      } });
    renameSync(temporary, target);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
  rejectSymlinks(target, 'grant workspace');
  return { directory: target, relativePath, created: true };
}

function request(input: unknown): CampaignDependencyStrikeGrant {
  const value = structuredClone(input);
  if (!isRecord(value)) {
    throw new Error('dependency strike grant must be an object');
  }
  const fields = new Set(['attemptId', 'grantId', 'level', 'nodeIds', 'strikes']);
  for (const key of Object.keys(value)) {
    if (!fields.has(key)) throw new Error(`dependency strike grant.${key} is unknown`);
  }
  for (const field of ['attemptId', 'grantId']) {
    if (typeof value[field] !== 'string' || !SAFE_ID.test(value[field])) {
      throw new Error(`dependency strike grant.${field} is required`);
    }
  }
  if (typeof value.level !== 'number' || !Number.isSafeInteger(value.level) || value.level < 1) {
    throw new Error('dependency strike grant.level must be a positive integer');
  }
  if (typeof value.strikes !== 'number' || !Number.isSafeInteger(value.strikes)
    || value.strikes < 1 || value.strikes > 20) {
    throw new Error('dependency strike grant.strikes must be from 1 through 20');
  }
  if (!Array.isArray(value.nodeIds) || value.nodeIds.length === 0
    || value.nodeIds.some(nodeId => typeof nodeId !== 'string' || !FEATURE_ID.test(nodeId))) {
    throw new Error('dependency strike grant.nodeIds must be a non-empty feature list');
  }
  const nodeIds = [...new Set(value.nodeIds as string[])].sort();
  if (nodeIds.length !== (value.nodeIds as string[]).length) {
    throw new Error('dependency strike grant.nodeIds cannot contain duplicates');
  }
  return { attemptId: value.attemptId as string, grantId: value.grantId as string,
    level: value.level, nodeIds, strikes: value.strikes };
}

function sameGrant(left: unknown, right: unknown): boolean {
  return canonicalDefinitionJson(left) === canonicalDefinitionJson(right);
}

export function grantCampaignDependencyStrikes(directory: string, input: unknown, {
  inspect = inspectCampaign,
  readState = (path: string, options: ProgressionContextOptions): StoredProgressionState =>
    readProgressionState(path, options),
  grantState = grantProgressionState,
  schedule = scheduleDependencyContinuation,
  writeState = writeCampaignState,
  acquire = acquireCampaignLock,
  release = releaseCampaignLock as (lock: unknown) => boolean,
  prepareWorkspace = prepareGrantWorkspace,
  now = new Date().toISOString(),
}: GrantOptions = {}): {
    attemptId: string;
    execution: string;
    grant: Omit<CampaignDependencyStrikeGrant, 'attemptId'>;
    grantWorkspace: string;
    snapshotSha256: string;
    scheduled: true;
  } {
  const root = resolve(directory);
  const desired = request(input);
  const initial = inspect(root, { requireCurrentInputs: false });
  const lock = acquire(root, initial.plan);
  try {
    const campaign = inspect(root, { requireCurrentInputs: false });
    if (campaign.plan.contentSha256 !== initial.plan.contentSha256) {
      throw new Error('campaign plan changed before the dependency strike grant');
    }
    if (campaign.plan.definition.mode?.id !== 'dependency'
      || !campaign.plan.featureCatalog || !campaign.plan.dependencyPolicy) {
      throw new Error('strike grants require one stored dependency campaign');
    }
    const attempt = campaign.state.attempts.find(item => item.plan.id === desired.attemptId);
    if (!attempt) throw new Error(`campaign attempt ${desired.attemptId} does not exist`);
    const execution = attempt.executions.at(-1);
    if (!execution) throw new Error(`campaign attempt ${desired.attemptId} has no execution`);
    const marker = execution.continuation;
    if (marker !== undefined) {
      const markedGrant = { grantId: marker.grantId, level: marker.level,
        nodeIds: marker.nodeIds, strikes: marker.strikes };
      if (!sameGrant(markedGrant, {
        grantId: desired.grantId, level: desired.level,
        nodeIds: desired.nodeIds, strikes: desired.strikes,
      })) throw new Error(`campaign attempt ${desired.attemptId} has a different continuation`);
    } else if (campaign.state.status !== 'completed' || attempt.status !== 'completed'
      || execution.status !== 'completed') {
      throw new Error('strike grants require one unextended completed campaign attempt');
    }

    const executionDirectory = childPath(root, execution.output, 'campaign execution');
    const workspace = marker === undefined
      ? prepareWorkspace(root, executionDirectory, desired.attemptId, desired.grantId,
        desired.level)
      : {
          directory: childPath(root, marker.resumeFrom, 'grant workspace'),
          relativePath: marker.resumeFrom,
          created: false,
        };
    const progression = compileProgressionInput(dependencyRuntimeDefinition(
      campaign.plan.featureCatalog, campaign.plan.dependencyPolicy));
    const owner = campaignProgressionOwner(campaign.plan, attempt.plan, { workspace: true });
    const statePath = join(workspace.directory, 'progression-state.json');
    const stored = readState(statePath, {
      progression,
      featureCatalogIdentity: campaign.plan.featureCatalog.identity,
      dependencyPolicyIdentity: campaign.plan.dependencyPolicy.identity,
      owner,
      requireCurrentEngine: true,
    });
    const grant = { grantId: desired.grantId, level: desired.level,
      nodeIds: desired.nodeIds, strikes: desired.strikes };
    const priorGrant = stored.state.grants.find(item => item.grantId === desired.grantId);
    if (marker !== undefined) {
      if (!priorGrant || !sameGrant(priorGrant, grant)
        || stored.snapshotSha256 !== marker.snapshotSha256) {
        throw new Error('campaign continuation marker does not match progression state');
      }
      return { attemptId: desired.attemptId, execution: execution.output,
        grant, grantWorkspace: workspace.relativePath,
        snapshotSha256: marker.snapshotSha256, scheduled: true };
    }
    let granted;
    if (stored.state.phase === 'terminal') {
      if (priorGrant) throw new Error(`duplicate dependency strike grant ${desired.grantId}`);
      granted = grantState(statePath, {
        progression,
        featureCatalogIdentity: campaign.plan.featureCatalog.identity,
        dependencyPolicyIdentity: campaign.plan.dependencyPolicy.identity,
        owner,
        grant,
        checkpoint: { artifact: `level-l${desired.level}-checkpoint.json` },
        expectedSnapshotSha256: stored.snapshotSha256,
      });
    } else {
      if (!priorGrant || !sameGrant(priorGrant, grant)) {
        throw new Error('active progression state does not match the requested continuation grant');
      }
      granted = stored;
    }

    const next = schedule(campaign.state, desired.attemptId, {
      ...grant, snapshotSha256: granted.snapshotSha256,
      resumeFrom: workspace.relativePath,
    }, { now });
    writeState(campaign.paths.state, campaign.plan, next);
    return { attemptId: desired.attemptId, execution: execution.output,
      grant, grantWorkspace: workspace.relativePath,
      snapshotSha256: granted.snapshotSha256, scheduled: true };
  } finally {
    release(lock);
  }
}
