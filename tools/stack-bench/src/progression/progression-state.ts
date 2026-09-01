import { existsSync, lstatSync, mkdtempSync, readdirSync, realpathSync, rmSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { currentEngineIdentity, emptyArtifactIdentities, readArtifact,
  writeArtifact } from '../evidence/artifacts.js';
import { hashDirectory, sha256 } from '../evidence/provenance.js';
import { acquireCampaignLock, releaseCampaignLock } from '../campaigns/campaign-lock.js';
import type { CampaignLock } from '../campaigns/campaign-lock.js';
import { hashAppSource, restoreAppSource, snapshotAppSource } from '../runtime/source-snapshot.js';
import {
  progressionEngine,
} from './progression-engine.js';
import {
  validateProgressionInput,
  type CompiledProgressionDefinition,
} from './progression-definition.js';

interface BoundIdentity extends Record<string, unknown> {
  id: string;
  version?: string | null;
  sha256: string;
  state?: string;
}

export interface ProgressionOwner {
  schemaVersion: 1;
  campaign: { id: string; version: string; sha256: string };
  attempt: {
    id: string;
    track: string;
    stack: string;
    agentAdapter: string;
    model: string;
    conditionSha256: string;
  };
  workspace?: { appDirectory: string };
}

export type ProgressionNodeStatus =
  | 'locked'
  | 'active'
  | 'working'
  | 'passed'
  | 'failed'
  | 'blocked';

export interface ProgressionNodeState {
  status: ProgressionNodeStatus;
  exhaustedAtLevel: number | null;
  exhaustionReason: 'strikes-exhausted' | 'repeated-findings' | null;
  unchangedFailure: { fingerprint: string | null; count: number };
  strikes: { initialBudget: number; granted: number; budget: number; used: number };
  checks: Record<string, 'pass' | 'fail' | 'test-system' | null>;
}

export interface ProgressionAttempt {
  attemptId: string;
  level: number;
  outcome: 'conclusive' | 'inconclusive';
  category?: string;
  reason?: string;
  runId?: string;
  sourceSha256?: string;
  selectionSha256?: string;
  evidence?: ProgressionEvidence;
  applicationFailure?: { phase: string; reason: string };
}

export interface ProgressionEvidence extends Record<string, unknown> {
  kind: 'grade_bundle';
  id: string;
  sha256: string;
}

export interface ProgressionTerminalOutcome {
  kind: 'passed' | 'partial' | 'failed';
  reason: 'graph-complete' | 'no-unlocked-nodes';
  level: number;
  blockedLevel?: number;
}

export interface ProgressionEvent extends Record<string, unknown> {
  type: string;
  result?: { attemptId: string; [key: string]: unknown };
  grant?: Record<string, unknown>;
}

export interface ProgressionGrant extends Record<string, unknown> {
  grantId: string;
  level: number;
  nodeIds: string[];
  strikes: number;
}

export interface ProgressionResumeBinding extends Record<string, unknown> {
  actionSha256: string;
  source: { directory: string; sha256: string; files: number };
}

export interface ProgressionState extends Record<string, unknown> {
  schemaVersion: number;
  policy: string;
  definition: CompiledProgressionDefinition;
  phase: 'active' | 'terminal';
  terminalOutcome: ProgressionTerminalOutcome | null;
  level: number;
  nodes: Record<string, ProgressionNodeState>;
  attempts: ProgressionAttempt[];
  grants: ProgressionGrant[];
  events: ProgressionEvent[];
}

interface WorkspaceProgressionOwner extends ProgressionOwner {
  workspace: { appDirectory: string };
}

export interface ProgressionStatePayload extends Record<string, unknown> {
  schemaVersion: 3;
  owner: WorkspaceProgressionOwner;
  featureCatalog: BoundIdentity;
  dependencyPolicy: BoundIdentity;
  events: ProgressionEvent[];
  resume?: unknown;
  stateSha256: string;
}

interface StateArtifact extends Record<string, unknown> {
  id: string;
  attempt: { id: string; parentId?: string | null };
  identities: {
    experiment?: { sha256?: string };
    engine: { sha256: string };
    agentAdapter?: { id?: string };
    stackAdapter?: { id?: string };
  };
  payload: ProgressionStatePayload;
}

interface RestoredProgressionState {
  state: ProgressionState;
  stateSha256: string;
  resume: unknown | null;
}

interface StateResult extends RestoredProgressionState {
  artifact: StateArtifact;
}

interface WriteProgressionStateOptions {
  progression: unknown;
  featureCatalogIdentity: unknown;
  dependencyPolicyIdentity: unknown;
  state: unknown;
  owner: unknown;
  resume?: unknown | null;
  id?: string;
}

interface ReadProgressionStateOptions {
  progression: unknown;
  featureCatalogIdentity: unknown;
  dependencyPolicyIdentity: unknown;
  owner: unknown;
  requireCurrentEngine?: boolean;
}

interface GrantProgressionStateOptions {
  progression: unknown;
  featureCatalogIdentity: unknown;
  dependencyPolicyIdentity: unknown;
  owner: unknown;
  grant: ProgressionGrant;
  checkpoint: unknown;
  expectedStateSha256: unknown;
}

interface SourceCheckpointPayload extends Record<string, unknown> {
  track: string;
  backend: string;
  level: number;
  selectionSha256: string;
  source: { directory: string; sha256: string; files: number };
}

interface SourceCheckpointArtifact extends Record<string, unknown> {
  attempt: { parentId?: string | null };
  identities: {
    engine?: { sha256?: string };
    agentAdapter?: { id?: string };
    stackAdapter?: { id?: string };
  };
  payload: SourceCheckpointPayload;
}

interface DirectoryHash {
  sha256: string;
  files: unknown[];
}

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const HASH = /^[a-f0-9]{64}$/;

function validateBoundIdentity(input: unknown, label: string): BoundIdentity {
  if (!object(input)) throw new Error(`${label} identity must be an object`);
  const fields = new Set(['id', 'version', 'sha256', 'state']);
  for (const key of Object.keys(input)) {
    if (!fields.has(key)) throw new Error(`${label} identity.${key} is unknown`);
  }
  if (typeof input.id !== 'string' || !input.id
    || (input.version !== undefined && input.version !== null
      && (typeof input.version !== 'string' || !input.version))
    || !HASH.test(typeof input.sha256 === 'string' ? input.sha256 : '')) {
    throw new Error(`${label} identity is invalid`);
  }
  return structuredClone(input) as BoundIdentity;
}

function boundIdentities(featureCatalogIdentity: unknown, dependencyPolicyIdentity: unknown): {
  featureCatalog: BoundIdentity;
  dependencyPolicy: BoundIdentity;
} {
  return {
    featureCatalog: validateBoundIdentity(featureCatalogIdentity, 'feature catalog'),
    dependencyPolicy: validateBoundIdentity(dependencyPolicyIdentity, 'dependency policy'),
  };
}

export function validateProgressionOwner(input: unknown,
  { requireWorkspace = false }: { requireWorkspace?: boolean } = {}): ProgressionOwner {
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
  const typedOwner = owner as unknown as ProgressionOwner;
  const campaignFields = new Set(['id', 'version', 'sha256']);
  const attemptFields = new Set([
    'id', 'track', 'stack', 'agentAdapter', 'model', 'conditionSha256',
  ]);
  const workspaceFields = new Set(['appDirectory']);
  for (const key of Object.keys(typedOwner.campaign)) {
    if (!campaignFields.has(key)) throw new Error(`progression state owner.campaign.${key} is unknown`);
  }
  for (const key of Object.keys(typedOwner.attempt)) {
    if (!attemptFields.has(key)) throw new Error(`progression state owner.attempt.${key} is unknown`);
  }
  for (const key of Object.keys(typedOwner.workspace ?? {})) {
    if (!workspaceFields.has(key)) {
      throw new Error(`progression state owner.workspace.${key} is unknown`);
    }
  }
  for (const [at, value] of [
    ['campaign.id', typedOwner.campaign.id], ['campaign.version', typedOwner.campaign.version],
    ['attempt.id', typedOwner.attempt.id], ['attempt.track', typedOwner.attempt.track],
    ['attempt.stack', typedOwner.attempt.stack],
    ['attempt.agentAdapter', typedOwner.attempt.agentAdapter],
    ['attempt.model', typedOwner.attempt.model],
  ] satisfies Array<[string, unknown]>) {
    if (typeof value !== 'string' || !value) throw new Error(`progression state owner.${at} is invalid`);
  }
  for (const [at, value] of [
    ['campaign.sha256', typedOwner.campaign.sha256],
    ['attempt.conditionSha256', typedOwner.attempt.conditionSha256],
  ] satisfies Array<[string, unknown]>) {
    if (typeof value !== 'string' || !HASH.test(value)) {
      throw new Error(`progression state owner.${at} is invalid`);
    }
  }
  if (typedOwner.workspace) {
    if (typeof typedOwner.workspace.appDirectory !== 'string' || !typedOwner.workspace.appDirectory
      || isAbsolute(typedOwner.workspace.appDirectory)
      || typedOwner.workspace.appDirectory.includes('\\')
      || typedOwner.workspace.appDirectory.split('/').some(part =>
        !part || part === '.' || part === '..')) {
      throw new Error('progression state owner.workspace.appDirectory must be a normalized relative path');
    }
    const validationRoot = resolve('__stack_bench_workspace__');
    const appPath = resolve(validationRoot, typedOwner.workspace.appDirectory);
    const appRelative = relative(validationRoot, appPath);
    if (appRelative === '..' || appRelative.startsWith(`..${sep}`) || appRelative === '') {
      throw new Error('progression state owner workspace application escapes its root');
    }
  }
  return typedOwner;
}

function validateWorkspaceOwner(input: unknown): WorkspaceProgressionOwner {
  const owner = validateProgressionOwner(input, { requireWorkspace: true });
  if (!owner.workspace) throw new Error('progression state owner is incomplete');
  return owner as WorkspaceProgressionOwner;
}

function directoryHash(path: string): DirectoryHash {
  return hashDirectory(path) as unknown as DirectoryHash;
}

function storedPayload(progressionInput: unknown, featureCatalogIdentity: unknown,
  dependencyPolicyIdentity: unknown, ownerInput: unknown, stateInput: unknown,
  resume: unknown | null = null): ProgressionStatePayload {
  const progression = validateProgressionInput(progressionInput);
  const identities = boundIdentities(featureCatalogIdentity, dependencyPolicyIdentity);
  const owner = validateWorkspaceOwner(ownerInput);
  if (!object(stateInput) || !Array.isArray(stateInput.events)) {
    throw new Error('progression state must contain an event history');
  }
  const state = progressionEngine.replay(progression.definition, stateInput.events);
  if (!isDeepStrictEqual(stateInput, state)) {
    throw new Error('progression state contradicts its event history');
  }
  const content = { owner, ...identities, events: state.events,
    ...(resume === null ? {} : { resume: structuredClone(resume) }) };
  return { schemaVersion: 3, ...content,
    stateSha256: sha256(canonicalDefinitionJson(content)) } as ProgressionStatePayload;
}

function restorePayload(payload: unknown, progressionInput: unknown,
  featureCatalogIdentity: unknown, dependencyPolicyIdentity: unknown,
  ownerInput: unknown): RestoredProgressionState {
  const progression = validateProgressionInput(progressionInput);
  const identities = boundIdentities(featureCatalogIdentity, dependencyPolicyIdentity);
  const owner = validateWorkspaceOwner(ownerInput);
  if (!object(payload) || payload.schemaVersion !== 3 || !object(payload.featureCatalog)
    || !object(payload.dependencyPolicy)
    || !object(payload.owner) || !Array.isArray(payload.events)
    || typeof payload.stateSha256 !== 'string') {
    throw new Error('progression state artifact is incomplete');
  }
  if (canonicalDefinitionJson(payload.featureCatalog)
      !== canonicalDefinitionJson(identities.featureCatalog)
    || canonicalDefinitionJson(payload.dependencyPolicy)
      !== canonicalDefinitionJson(identities.dependencyPolicy)) {
    throw new Error('progression state artifact has the wrong feature catalog or dependency policy');
  }
  if (canonicalDefinitionJson(payload.owner) !== canonicalDefinitionJson(owner)) {
    throw new Error('progression state artifact has the wrong campaign attempt owner');
  }
  const content = { owner: payload.owner, featureCatalog: payload.featureCatalog,
    dependencyPolicy: payload.dependencyPolicy, events: payload.events,
    ...(payload.resume === undefined ? {} : { resume: payload.resume }) };
  if (sha256(canonicalDefinitionJson(content)) !== payload.stateSha256) {
    throw new Error('progression state identity does not match its contents');
  }
  const state = progressionEngine.replay(progression.definition, payload.events);
  return { state, stateSha256: payload.stateSha256,
    resume: payload.resume === undefined ? null : structuredClone(payload.resume) };
}

export function writeProgressionState(path: string, { progression, featureCatalogIdentity,
  dependencyPolicyIdentity, state,
  owner: ownerInput, resume = null, id = 'progression-state' }:
  Partial<WriteProgressionStateOptions> = {}): StateResult {
  const owner = validateWorkspaceOwner(ownerInput);
  const payload = storedPayload(progression, featureCatalogIdentity,
    dependencyPolicyIdentity, owner, state, resume);
  const artifact = writeArtifact(resolve(path), { kind: 'progression_state', id,
    attempt: { id: owner.attempt.id, parentId: owner.campaign.id },
    identities: emptyArtifactIdentities({ experiment: {
      id: owner.campaign.id, version: owner.campaign.version,
      sha256: owner.campaign.sha256,
    }, stackAdapter: { id: owner.attempt.stack } }), payload }) as unknown as StateArtifact;
  return { artifact, ...restorePayload(artifact.payload, progression,
    featureCatalogIdentity, dependencyPolicyIdentity, owner) };
}

export function readProgressionState(path: string, { progression, featureCatalogIdentity,
  dependencyPolicyIdentity, owner,
  requireCurrentEngine = false }: Partial<ReadProgressionStateOptions> = {}): StateResult {
  const validatedOwner = validateWorkspaceOwner(owner);
  const artifact = readArtifact(resolve(path), { expectedKind: 'progression_state' }) as unknown as StateArtifact;
  if (artifact.attempt.id !== validatedOwner.attempt.id
    || artifact.attempt.parentId !== validatedOwner.campaign.id
    || artifact.identities.experiment?.sha256 !== validatedOwner.campaign.sha256
    || artifact.identities.stackAdapter?.id !== validatedOwner.attempt.stack) {
    throw new Error('progression state artifact envelope has the wrong campaign attempt owner');
  }
  if (requireCurrentEngine
    && artifact.identities.engine.sha256 !== currentEngineIdentity().sha256) {
    throw new Error('progression state artifact uses a different harness executable');
  }
  return { artifact, ...restorePayload(artifact.payload, progression,
    featureCatalogIdentity, dependencyPolicyIdentity, validatedOwner) };
}

export function progressionStateExists(path: string): boolean {
  return existsSync(resolve(path));
}

function stateLock(path: string, progression: unknown, featureCatalogIdentity: unknown,
  dependencyPolicyIdentity: unknown, ownerInput: unknown): CampaignLock {
  validateProgressionInput(progression);
  const identities = boundIdentities(featureCatalogIdentity, dependencyPolicyIdentity);
  const owner = validateWorkspaceOwner(ownerInput);
  const lockSha256 = sha256(canonicalDefinitionJson({ ...identities, owner }));
  return acquireCampaignLock(`${resolve(path)}.control`, {
    id: `progression-${lockSha256.slice(0, 16)}`,
    contentSha256: lockSha256,
  });
}

function ownedPath(root: string, input: unknown, label: string): string {
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

function rejectTreeSymlinks(path: string, label: string): void {
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    if (entry.isSymbolicLink()) throw new Error(`${label} contains a symbolic link`);
    if (entry.isDirectory()) rejectTreeSymlinks(join(path, entry.name), label);
  }
}

function grantSource(state: ProgressionState, owner: WorkspaceProgressionOwner,
  grant: ProgressionGrant, checkpoint: unknown, workspaceRoot: string,
  engineSha256: string): { appDir: string; sourcePath: string; sha256: string } {
  if (!object(checkpoint) || typeof checkpoint.artifact !== 'string'
    || !checkpoint.artifact) {
    throw new Error('continuation grant requires an exact source checkpoint artifact');
  }
  const attempt: ProgressionAttempt | undefined = [...state.attempts].reverse().find(item =>
    item.level === grant?.level && item.outcome === 'conclusive');
  if (!attempt?.runId || !attempt.sourceSha256 || !attempt.selectionSha256) {
    throw new Error('progression level has no source-bound graded attempt to continue');
  }
  const checkpointPath = ownedPath(workspaceRoot, checkpoint.artifact,
    'continuation checkpoint artifact');
  const artifact = readArtifact(checkpointPath, { expectedKind: 'source_checkpoint',
    expectedId: `${attempt.runId}-l${grant.level}-checkpoint` }) as unknown as SourceCheckpointArtifact;
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
  const saved = directoryHash(sourcePath);
  if (saved.sha256 !== artifact.payload.source.sha256
    || saved.files.length !== artifact.payload.source.files) {
    throw new Error('continuation checkpoint bytes do not match the graded level source');
  }
  return { appDir, sourcePath, sha256: artifact.payload.source.sha256 };
}

function grantResumeBinding(state: ProgressionState, owner: WorkspaceProgressionOwner,
  workspaceRoot: string): ProgressionResumeBinding {
  const sourcePath = ownedPath(workspaceRoot, owner.workspace.appDirectory,
    'progression application');
  const source = hashAppSource(sourcePath);
  return {
    actionSha256: sha256(canonicalDefinitionJson(progressionEngine.nextAction(state))),
    source: {
      directory: owner.workspace.appDirectory,
      sha256: source.sha256,
      files: source.files.length,
    },
  };
}

export function grantProgressionState(path: string, { progression, featureCatalogIdentity,
  dependencyPolicyIdentity, owner, grant,
  checkpoint, expectedStateSha256 }: Partial<GrantProgressionStateOptions> = {}): StateResult {
  if (typeof expectedStateSha256 !== 'string' || !expectedStateSha256) {
    throw new Error('continuation grant requires the expected progression state identity');
  }
  const lock = stateLock(path, progression, featureCatalogIdentity,
    dependencyPolicyIdentity, owner);
  try {
    const current = readProgressionState(path, { progression, featureCatalogIdentity,
      dependencyPolicyIdentity, owner, requireCurrentEngine: true });
    if (current.stateSha256 !== expectedStateSha256) {
      throw new Error('progression state changed before the continuation grant');
    }
    const validatedOwner = validateWorkspaceOwner(owner);
    const state = progressionEngine.grantStrikes(current.state, grant);
    const workspaceRoot = dirname(resolve(path));
    const source = grantSource(current.state, validatedOwner, grant as ProgressionGrant,
      checkpoint, workspaceRoot,
      current.artifact.identities.engine.sha256);
    const previous = hashAppSource(source.appDir);
    const backupRoot = mkdtempSync(join(workspaceRoot, '.progression-grant-'));
    const backupSource = join(backupRoot, 'source');
    try {
      snapshotAppSource(source.appDir, backupSource);
      try {
        restoreAppSource(source.sourcePath, source.appDir);
        if (hashAppSource(source.appDir).sha256 !== source.sha256) {
          throw new Error('continuation source restoration did not reproduce the graded level source');
        }
        return writeProgressionState(path, { progression, featureCatalogIdentity,
          dependencyPolicyIdentity, owner: validatedOwner, state,
          resume: grantResumeBinding(state, validatedOwner, workspaceRoot),
          id: current.artifact.id });
      } catch (error) {
        try {
          restoreAppSource(backupSource, source.appDir);
          const restored = hashAppSource(source.appDir);
          if (restored.sha256 !== previous.sha256 || restored.files.length !== previous.files.length) {
            throw new Error('continuation grant rollback did not restore the prior application source');
          }
        } catch (rollbackError) {
          throw new AggregateError([error, rollbackError],
            'continuation grant failed and its source rollback also failed');
        }
        throw error;
      }
    } finally {
      rmSync(backupRoot, { recursive: true, force: true });
    }
  } finally {
    releaseCampaignLock(lock);
  }
}
