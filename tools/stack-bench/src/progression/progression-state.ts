import { existsSync, lstatSync, readdirSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { z } from 'zod';

import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { currentEngineIdentity, emptyArtifactIdentities, readArtifact,
  writeArtifact } from '../evidence/artifacts.js';
import { sha256 } from '../evidence/provenance.js';
import { acquireCampaignLock, releaseCampaignLock } from '../campaigns/campaign-lock.js';
import type { CampaignLock } from '../campaigns/campaign-lock.js';
import { hashAppSource } from '../runtime/source-snapshot.js';
import { formatZodError } from '../zod-error.js';
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
  exhaustionReason: 'repair-budget-exhausted' | 'repeated-findings' | null;
  unchangedFailure: { fingerprint: string | null; count: number };
  repairs: { used: number };
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
  repairRegression?: ProgressionRepairRegression;
  repair?: { nodeIds: string[]; depth: number; grantId?: string };
}

export interface ProgressionRepairRegression extends Record<string, unknown> {
  ownerNodeIds: string[];
  report: string;
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
  result?: { attemptId: string; repairRegression?: ProgressionRepairRegression;
    [key: string]: unknown };
  grant?: Record<string, unknown>;
}

export interface ProgressionRepairGrant extends Record<string, unknown> {
  grantId: string;
  level: number;
  nodeIds: string[];
  repairs: number;
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
  grants: ProgressionRepairGrant[];
  events: ProgressionEvent[];
}

interface WorkspaceProgressionOwner extends ProgressionOwner {
  workspace: { appDirectory: string };
}

export interface ProgressionStatePayload extends Record<string, unknown> {
  schemaVersion: 4;
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
  grant: ProgressionRepairGrant;
  expectedStateSha256: unknown;
}

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const HASH = /^[a-f0-9]{64}$/;
const hashSchema = z.string().regex(HASH);
const boundIdentitySchema = z.strictObject({
  id: z.string().min(1),
  version: z.string().min(1).nullable().optional(),
  sha256: hashSchema,
  state: z.string().optional(),
});
const progressionOwnerSchema = z.strictObject({
  schemaVersion: z.literal(1),
  campaign: z.strictObject({ id: z.string().min(1), version: z.string().min(1), sha256: hashSchema }),
  attempt: z.strictObject({
    id: z.string().min(1),
    track: z.string().min(1),
    stack: z.string().min(1),
    agentAdapter: z.string().min(1),
    model: z.string().min(1),
    conditionSha256: hashSchema,
  }),
  workspace: z.strictObject({ appDirectory: z.string().min(1) }).optional(),
});
const eventHistorySchema = z.looseObject({ events: z.array(z.unknown()) });
const storedProgressionSchema = z.looseObject({
  schemaVersion: z.literal(4),
  featureCatalog: boundIdentitySchema,
  dependencyPolicy: boundIdentitySchema,
  owner: progressionOwnerSchema,
  events: z.array(z.unknown()),
  resume: z.unknown().optional(),
  stateSha256: hashSchema,
});

function validateBoundIdentity(input: unknown, label: string): BoundIdentity {
  const parsed = boundIdentitySchema.safeParse(input);
  if (!parsed.success) throw new Error(formatZodError(parsed.error, `${label} identity`));
  return parsed.data;
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
  const parsed = progressionOwnerSchema.safeParse(input);
  if (!parsed.success) throw new Error(formatZodError(parsed.error, 'progression state owner'));
  const typedOwner = parsed.data;
  if (requireWorkspace && !typedOwner.workspace) throw new Error('progression state owner is incomplete');
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

function storedPayload(progressionInput: unknown, featureCatalogIdentity: unknown,
  dependencyPolicyIdentity: unknown, ownerInput: unknown, stateInput: unknown,
  resume: unknown | null = null): ProgressionStatePayload {
  const progression = validateProgressionInput(progressionInput);
  const identities = boundIdentities(featureCatalogIdentity, dependencyPolicyIdentity);
  const owner = validateWorkspaceOwner(ownerInput);
  const parsedState = eventHistorySchema.safeParse(stateInput);
  if (!parsedState.success) {
    throw new Error('progression state must contain an event history');
  }
  const state = progressionEngine.replay(progression.definition, parsedState.data.events);
  if (!isDeepStrictEqual(stateInput, state)) {
    throw new Error('progression state contradicts its event history');
  }
  const content = { owner, ...identities, events: state.events,
    ...(resume === null ? {} : { resume: structuredClone(resume) }) };
  return { schemaVersion: 4, ...content,
    stateSha256: sha256(canonicalDefinitionJson(content)) } as ProgressionStatePayload;
}

function restorePayload(payload: unknown, progressionInput: unknown,
  featureCatalogIdentity: unknown, dependencyPolicyIdentity: unknown,
  ownerInput: unknown): RestoredProgressionState {
  const progression = validateProgressionInput(progressionInput);
  const identities = boundIdentities(featureCatalogIdentity, dependencyPolicyIdentity);
  const owner = validateWorkspaceOwner(ownerInput);
  const parsedPayload = storedProgressionSchema.safeParse(payload);
  if (!parsedPayload.success) {
    throw new Error('progression state artifact is incomplete');
  }
  const stored = parsedPayload.data;
  if (canonicalDefinitionJson(stored.featureCatalog)
      !== canonicalDefinitionJson(identities.featureCatalog)
    || canonicalDefinitionJson(stored.dependencyPolicy)
      !== canonicalDefinitionJson(identities.dependencyPolicy)) {
    throw new Error('progression state artifact has the wrong feature catalog or dependency policy');
  }
  if (canonicalDefinitionJson(stored.owner) !== canonicalDefinitionJson(owner)) {
    throw new Error('progression state artifact has the wrong campaign attempt owner');
  }
  const content = { owner: stored.owner, featureCatalog: stored.featureCatalog,
    dependencyPolicy: stored.dependencyPolicy, events: stored.events,
    ...(stored.resume === undefined ? {} : { resume: stored.resume }) };
  if (sha256(canonicalDefinitionJson(content)) !== stored.stateSha256) {
    throw new Error('progression state identity does not match its contents');
  }
  const state = progressionEngine.replay(progression.definition, stored.events);
  return { state, stateSha256: stored.stateSha256,
    resume: stored.resume === undefined ? null : structuredClone(stored.resume) };
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

function validateGrantSource(state: ProgressionState, owner: WorkspaceProgressionOwner,
  resume: unknown, workspaceRoot: string): void {
  if (!object(resume) || typeof resume.actionSha256 !== 'string'
    || !HASH.test(resume.actionSha256) || !object(resume.source)
    || resume.source.directory !== owner.workspace.appDirectory
    || typeof resume.source.sha256 !== 'string' || !HASH.test(resume.source.sha256)
    || !Number.isSafeInteger(resume.source.files) || Number(resume.source.files) < 0) {
    throw new Error('continuation grant requires the current accepted source binding');
  }
  if (resume.actionSha256 !== sha256(canonicalDefinitionJson(progressionEngine.nextAction(state)))) {
    throw new Error('continuation source binding does not match the terminal progression action');
  }
  const sourcePath = ownedPath(workspaceRoot, resume.source.directory,
    'progression application');
  rejectTreeSymlinks(sourcePath, 'progression application');
  const source = hashAppSource(sourcePath);
  if (source.sha256 !== resume.source.sha256 || source.files.length !== resume.source.files) {
    throw new Error('current accepted source does not match the progression state');
  }
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
  expectedStateSha256 }: Partial<GrantProgressionStateOptions> = {}): StateResult {
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
    const workspaceRoot = dirname(resolve(path));
    validateGrantSource(current.state, validatedOwner, current.resume, workspaceRoot);
    const state = progressionEngine.grantRepairs(current.state, grant);
    return writeProgressionState(path, { progression, featureCatalogIdentity,
      dependencyPolicyIdentity, owner: validatedOwner, state,
      resume: grantResumeBinding(state, validatedOwner, workspaceRoot),
      id: current.artifact.id });
  } finally {
    releaseCampaignLock(lock);
  }
}
