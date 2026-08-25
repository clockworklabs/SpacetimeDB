import { existsSync, lstatSync, readdirSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.mjs';
import { currentEngineIdentity, emptyArtifactIdentities, readArtifact,
  writeArtifact } from '../evidence/artifacts.mjs';
import { hashDirectory, sha256 } from '../evidence/provenance.mjs';
import { acquireCampaignLock, releaseCampaignLock } from '../campaigns/campaign-lock.mjs';
import { hashAppSource, restoreAppSource } from '../runtime/source-snapshot.mjs';
import { progressionEngine } from './progression-engine.mjs';
import { validateProgressionInput } from './progression-definition.mjs';

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const HASH = /^[a-f0-9]{64}$/;

export function validateProgressionOwner(input, { requireWorkspace = false } = {}) {
  if (!object(input)) throw new Error('progression state owner must be an object');
  const owner = structuredClone(input);
  const fields = new Set(['schemaVersion', 'campaign', 'attempt', 'workspace']);
  for (const key of Object.keys(owner)) {
    if (!fields.has(key)) throw new Error(`progression state owner.${key} is unknown`);
  }
  if (owner.schemaVersion !== 1 || !object(owner.campaign) || !object(owner.attempt)
    || (requireWorkspace && !object(owner.workspace))) {
    throw new Error('progression state owner is incomplete');
  }
  const campaignFields = new Set(['id', 'version', 'sha256']);
  const attemptFields = new Set([
    'id', 'track', 'stack', 'agentAdapter', 'model', 'conditionSha256',
  ]);
  const workspaceFields = new Set(['appDirectory']);
  for (const key of Object.keys(owner.campaign)) {
    if (!campaignFields.has(key)) throw new Error(`progression state owner.campaign.${key} is unknown`);
  }
  for (const key of Object.keys(owner.attempt)) {
    if (!attemptFields.has(key)) throw new Error(`progression state owner.attempt.${key} is unknown`);
  }
  for (const key of Object.keys(owner.workspace ?? {})) {
    if (!workspaceFields.has(key)) {
      throw new Error(`progression state owner.workspace.${key} is unknown`);
    }
  }
  for (const [at, value] of [
    ['campaign.id', owner.campaign.id], ['campaign.version', owner.campaign.version],
    ['attempt.id', owner.attempt.id], ['attempt.track', owner.attempt.track],
    ['attempt.stack', owner.attempt.stack],
    ['attempt.agentAdapter', owner.attempt.agentAdapter], ['attempt.model', owner.attempt.model],
  ]) {
    if (typeof value !== 'string' || !value) throw new Error(`progression state owner.${at} is invalid`);
  }
  for (const [at, value] of [
    ['campaign.sha256', owner.campaign.sha256],
    ['attempt.conditionSha256', owner.attempt.conditionSha256],
  ]) {
    if (!HASH.test(value ?? '')) throw new Error(`progression state owner.${at} is invalid`);
  }
  if (owner.workspace) {
    if (typeof owner.workspace.appDirectory !== 'string' || !owner.workspace.appDirectory
      || isAbsolute(owner.workspace.appDirectory) || owner.workspace.appDirectory.includes('\\')
      || owner.workspace.appDirectory.split('/').some(part => !part || part === '.' || part === '..')) {
      throw new Error('progression state owner.workspace.appDirectory must be a normalized relative path');
    }
    const validationRoot = resolve('__stack_bench_workspace__');
    const appPath = resolve(validationRoot, owner.workspace.appDirectory);
    const appRelative = relative(validationRoot, appPath);
    if (appRelative === '..' || appRelative.startsWith(`..${sep}`) || appRelative === '') {
      throw new Error('progression state owner workspace application escapes its root');
    }
  }
  return owner;
}

function storedPayload(progression, owner, state, resume = null) {
  progression = validateProgressionInput(progression);
  owner = validateProgressionOwner(owner, { requireWorkspace: true });
  state = progressionEngine.resume(state);
  if (canonicalDefinitionJson(state.definition)
    !== canonicalDefinitionJson(progression.definition)) {
    throw new Error('progression state definition does not match its progression identity');
  }
  const snapshot = structuredClone(state);
  const events = snapshot.events;
  delete snapshot.definition;
  delete snapshot.events;
  const content = { owner, progression: progression.identity, events, snapshot,
    ...(resume === null ? {} : { resume: structuredClone(resume) }) };
  return { schemaVersion: 1, ...content,
    snapshotSha256: sha256(canonicalDefinitionJson(content)) };
}

function restorePayload(payload, progression, owner) {
  progression = validateProgressionInput(progression);
  owner = validateProgressionOwner(owner, { requireWorkspace: true });
  if (!object(payload) || payload.schemaVersion !== 1 || !object(payload.progression)
    || !object(payload.owner) || !Array.isArray(payload.events) || !object(payload.snapshot)
    || typeof payload.snapshotSha256 !== 'string') {
    throw new Error('progression state artifact is incomplete');
  }
  if (canonicalDefinitionJson(payload.progression)
    !== canonicalDefinitionJson(progression.identity)) {
    throw new Error('progression state artifact has the wrong progression identity');
  }
  if (canonicalDefinitionJson(payload.owner) !== canonicalDefinitionJson(owner)) {
    throw new Error('progression state artifact has the wrong campaign attempt owner');
  }
  const content = { owner: payload.owner, progression: payload.progression, events: payload.events,
    snapshot: payload.snapshot,
    ...(payload.resume === undefined ? {} : { resume: payload.resume }) };
  if (sha256(canonicalDefinitionJson(content)) !== payload.snapshotSha256) {
    throw new Error('progression state snapshot identity does not match its contents');
  }
  if (Object.hasOwn(payload.snapshot, 'definition') || Object.hasOwn(payload.snapshot, 'events')) {
    throw new Error('progression state snapshot duplicates stored definition or events');
  }
  const state = { ...structuredClone(payload.snapshot), definition: progression.definition,
    events: structuredClone(payload.events) };
  return { state: progressionEngine.resume(state), snapshotSha256: payload.snapshotSha256,
    resume: payload.resume === undefined ? null : structuredClone(payload.resume) };
}

export function writeProgressionState(path, { progression, state,
  owner, resume = null, id = 'progression-state' } = {}) {
  owner = validateProgressionOwner(owner, { requireWorkspace: true });
  const payload = storedPayload(progression, owner, state, resume);
  const artifact = writeArtifact(resolve(path), { kind: 'progression_state', id,
    attempt: { id: owner.attempt.id, parentId: owner.campaign.id },
    identities: emptyArtifactIdentities({ experiment: {
      id: owner.campaign.id, version: owner.campaign.version,
      sha256: owner.campaign.sha256,
    }, stackAdapter: { id: owner.attempt.stack } }), payload });
  return { artifact, ...restorePayload(artifact.payload, progression, owner) };
}

export function readProgressionState(path, { progression, owner,
  requireCurrentEngine = false } = {}) {
  owner = validateProgressionOwner(owner, { requireWorkspace: true });
  const artifact = readArtifact(resolve(path), { expectedKind: 'progression_state' });
  if (artifact.attempt.id !== owner.attempt.id
    || artifact.attempt.parentId !== owner.campaign.id
    || artifact.identities.experiment?.sha256 !== owner.campaign.sha256
    || artifact.identities.stackAdapter?.id !== owner.attempt.stack) {
    throw new Error('progression state artifact envelope has the wrong campaign attempt owner');
  }
  if (requireCurrentEngine
    && artifact.identities.engine.sha256 !== currentEngineIdentity().sha256) {
    throw new Error('progression state artifact uses a different harness executable');
  }
  return { artifact, ...restorePayload(artifact.payload, progression, owner) };
}

export function progressionStateExists(path) {
  return existsSync(resolve(path));
}

function stateLock(path, progression, owner) {
  const identity = validateProgressionInput(progression).identity;
  owner = validateProgressionOwner(owner, { requireWorkspace: true });
  const lockSha256 = sha256(canonicalDefinitionJson({ progression: identity, owner }));
  return acquireCampaignLock(`${resolve(path)}.control`, {
    id: `progression-${lockSha256.slice(0, 16)}`,
    contentSha256: lockSha256,
  });
}

function ownedPath(root, input, label) {
  if (typeof input !== 'string' || !input || isAbsolute(input)) {
    throw new Error(`${label} must be a relative path in the progression workspace`);
  }
  const absoluteRoot = resolve(root);
  if (lstatSync(absoluteRoot).isSymbolicLink()) {
    throw new Error('progression workspace root cannot be a symbolic link');
  }
  const absolute = resolve(absoluteRoot, input);
  const rel = relative(absoluteRoot, absolute);
  if (rel === '..' || rel.startsWith(`..${sep}`) || rel === '') {
    throw new Error(`${label} escapes the progression workspace`);
  }
  let current = absoluteRoot;
  for (const part of rel.split(/[\\/]/)) {
    current = join(current, part);
    if (lstatSync(current).isSymbolicLink()) {
      throw new Error(`${label} cannot traverse a symbolic link`);
    }
  }
  const realRoot = realpathSync(absoluteRoot);
  const real = realpathSync(absolute);
  const realRelative = relative(realRoot, real);
  if (realRelative === '..' || realRelative.startsWith(`..${sep}`) || realRelative === '') {
    throw new Error(`${label} resolves outside the progression workspace`);
  }
  return real;
}

function rejectTreeSymlinks(path, label) {
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    if (entry.isSymbolicLink()) throw new Error(`${label} contains a symbolic link`);
    if (entry.isDirectory()) rejectTreeSymlinks(join(path, entry.name), label);
  }
}

function restoreGrantSource(state, owner, grant, checkpoint, workspaceRoot, engineSha256) {
  if (!object(checkpoint) || typeof checkpoint.artifact !== 'string'
    || !checkpoint.artifact) {
    throw new Error('continuation grant requires an exact source checkpoint artifact');
  }
  const attempt = [...state.attempts].reverse().find(item =>
    item.level === grant?.level && item.outcome === 'conclusive');
  if (!attempt?.runId || !attempt.sourceSha256 || !attempt.selectionSha256) {
    throw new Error('progression level has no source-bound graded attempt to continue');
  }
  const checkpointPath = ownedPath(workspaceRoot, checkpoint.artifact,
    'continuation checkpoint artifact');
  const artifact = readArtifact(checkpointPath, { expectedKind: 'source_checkpoint',
    expectedId: `${attempt.runId}-l${grant.level}-checkpoint` });
  if (artifact.attempt.parentId !== attempt.runId
    || artifact.payload.track !== owner.attempt.track
    || artifact.payload.backend !== owner.attempt.stack
    || artifact.payload.level !== grant.level
    || artifact.payload.selectionSha256 !== attempt.selectionSha256
    || artifact.payload.source.sha256 !== attempt.sourceSha256
    || artifact.identities.engine?.sha256 !== engineSha256
    || artifact.identities.agentAdapter?.id !== owner.attempt.agentAdapter
    || artifact.identities.stackAdapter?.id !== owner.attempt.stack) {
    throw new Error('continuation checkpoint does not match the graded level attempt');
  }
  const artifactDirectory = relative(workspaceRoot, dirname(checkpointPath));
  const sourcePath = ownedPath(workspaceRoot,
    join(artifactDirectory, artifact.payload.source.directory),
    'continuation checkpoint source');
  const appDir = ownedPath(workspaceRoot, owner.workspace.appDirectory,
    'progression application');
  if (sourcePath === appDir) throw new Error('continuation checkpoint cannot be the live application');
  rejectTreeSymlinks(sourcePath, 'continuation checkpoint source');
  const saved = hashDirectory(sourcePath);
  if (saved.sha256 !== artifact.payload.source.sha256
    || saved.files.length !== artifact.payload.source.files) {
    throw new Error('continuation checkpoint bytes do not match the graded level source');
  }
  restoreAppSource(sourcePath, appDir);
  if (hashAppSource(appDir).sha256 !== artifact.payload.source.sha256) {
    throw new Error('continuation source restoration did not reproduce the graded level source');
  }
}

export function grantProgressionState(path, { progression, owner, grant,
  checkpoint, expectedSnapshotSha256 } = {}) {
  if (typeof expectedSnapshotSha256 !== 'string' || !expectedSnapshotSha256) {
    throw new Error('continuation grant requires the expected progression snapshot identity');
  }
  const lock = stateLock(path, progression, owner);
  try {
    const current = readProgressionState(path, { progression, owner, requireCurrentEngine: true });
    if (current.snapshotSha256 !== expectedSnapshotSha256) {
      throw new Error('progression state changed before the continuation grant');
    }
    const state = progressionEngine.grantStrikes(current.state, grant);
    restoreGrantSource(current.state, owner, grant, checkpoint, dirname(resolve(path)),
      current.artifact.identities.engine.sha256);
    return writeProgressionState(path, { progression, owner, state,
      id: current.artifact.id });
  } finally {
    releaseCampaignLock(lock);
  }
}

export function acquireProgressionStateLock(path, progression, owner) {
  return stateLock(path, progression, owner);
}

export function releaseProgressionStateLock(lock) {
  return releaseCampaignLock(lock);
}
