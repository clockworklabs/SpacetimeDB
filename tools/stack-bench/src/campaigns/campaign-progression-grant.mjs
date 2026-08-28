import { cpSync, existsSync, lstatSync, mkdirSync, readdirSync, renameSync, rmSync }
  from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.mjs';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../progression/progression-definition.mjs';
import { grantProgressionState, readProgressionState }
  from '../progression/progression-state.mjs';
import { acquireCampaignLock, releaseCampaignLock } from './campaign-lock.mjs';
import { inspectCampaign } from './campaign-runner.mjs';
import { scheduleDependencyContinuation, writeCampaignState }
  from './campaign-scheduler.mjs';

function childPath(root, input, label) {
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

function rejectSymlinks(path, label) {
  if (lstatSync(path).isSymbolicLink()) throw new Error(`${label} cannot be a symbolic link`);
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`${label} contains a symbolic link`);
    if (entry.isDirectory()) rejectSymlinks(child, label);
  }
}

export function prepareGrantWorkspace(root, executionDirectory, attemptId, grantId) {
  if (!SAFE_ID.test(attemptId) || !SAFE_ID.test(grantId)) {
    throw new Error('grant workspace requires safe attempt and grant ids');
  }
  rejectSymlinks(executionDirectory, 'completed campaign execution');
  const relativePath = `continuations/${attemptId}/${grantId}`;
  const target = childPath(root, relativePath, 'grant workspace');
  if (existsSync(target)) {
    rejectSymlinks(target, 'grant workspace');
    return { directory: target, relativePath, created: false };
  }
  mkdirSync(dirname(target), { recursive: true });
  const temporary = `${target}.tmp-${process.pid}-${Date.now()}`;
  try {
    cpSync(executionDirectory, temporary, { recursive: true, errorOnExist: true });
    renameSync(temporary, target);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
  rejectSymlinks(target, 'grant workspace');
  return { directory: target, relativePath, created: true };
}

function progressionOwner(plan, attempt) {
  return {
    schemaVersion: 1,
    campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
    attempt: {
      id: attempt.id,
      track: plan.definition.track,
      stack: attempt.stack,
      agentAdapter: attempt.agentAdapter,
      model: attempt.model,
      conditionSha256: attempt.condition.sha256,
    },
    workspace: { appDirectory: 'source' },
  };
}

function request(input) {
  const value = structuredClone(input);
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
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
  if (!Number.isSafeInteger(value.level) || value.level < 1) {
    throw new Error('dependency strike grant.level must be a positive integer');
  }
  if (!Number.isSafeInteger(value.strikes) || value.strikes < 1 || value.strikes > 20) {
    throw new Error('dependency strike grant.strikes must be from 1 through 20');
  }
  if (!Array.isArray(value.nodeIds) || value.nodeIds.length === 0
    || value.nodeIds.some(nodeId => typeof nodeId !== 'string' || !FEATURE_ID.test(nodeId))) {
    throw new Error('dependency strike grant.nodeIds must be a non-empty feature list');
  }
  value.nodeIds = [...new Set(value.nodeIds)].sort();
  if (value.nodeIds.length !== input.nodeIds.length) {
    throw new Error('dependency strike grant.nodeIds cannot contain duplicates');
  }
  return value;
}

function sameGrant(left, right) {
  return canonicalDefinitionJson(left) === canonicalDefinitionJson(right);
}

export function grantCampaignDependencyStrikes(directory, input, {
  inspect = inspectCampaign,
  readState = readProgressionState,
  grantState = grantProgressionState,
  schedule = scheduleDependencyContinuation,
  writeState = writeCampaignState,
  acquire = acquireCampaignLock,
  release = releaseCampaignLock,
  prepareWorkspace = prepareGrantWorkspace,
  now = new Date().toISOString(),
} = {}) {
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
      ? prepareWorkspace(root, executionDirectory, desired.attemptId, desired.grantId)
      : {
          directory: childPath(root, marker.resumeFrom, 'grant workspace'),
          relativePath: marker.resumeFrom,
          created: false,
        };
    const progression = compileProgressionInput(dependencyRuntimeDefinition(
      campaign.plan.featureCatalog, campaign.plan.dependencyPolicy));
    const owner = progressionOwner(campaign.plan, attempt.plan);
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
