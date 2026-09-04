import { existsSync, readFileSync, realpathSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, isAbsolute, join, relative, resolve } from 'node:path';

import { ARTIFACT_FILE, emptyArtifactIdentities, readArtifact, readArtifactPayload,
  writeArtifact } from '../evidence/artifacts.js';
import { publicBackendLease, readBackendLease } from './backend-lease.js';
import { releaseBackendLease } from './backend-teardown.js';
import type { BackendLease, PublicBackendLease } from './backend-lease.js';

export const SUPERVISOR_STATE_VERSION = 2;

export interface SupervisorState {
  version: typeof SUPERVISOR_STATE_VERSION;
  runId: string;
  backend: string;
  runtimeDir: string;
  leasePath: string;
  ownershipToken: string;
  output: string;
}

export interface RecoveryPlan {
  schemaVersion: 1;
  status: 'clean' | 'retained' | 'quarantined';
  runId: string;
  backend: string;
  reason: string | null;
  cleanup: { succeeded: boolean; retained: boolean };
  resources: {
    backendState: string;
    buildContainer: { id: string; name: string; running: boolean } | null;
    listenerProcesses: Array<{ pid: number; startMarker: string }>;
    locks: Array<{ key: string; released: boolean }>;
  };
  instructions: string[];
}

export interface RecoveryResult {
  ok: boolean;
  state: 'clean' | 'quarantined';
  runId: string;
  recoveryPath: string;
}

export function rescueSupervisedLease(path: string, output: string): void {
  if (!existsSync(path)) return;
  const state = validateSupervisorState(JSON.parse(readFileSync(path, 'utf8')), { source: path });
  if (resolve(output) !== resolve(state.output)) {
    throw new Error(`supervisor output does not match requested output: ${state.runId}`);
  }
  if (!existsSync(state.leasePath)) {
    const runPath = join(output, ARTIFACT_FILE.run);
    if (!existsSync(runPath)) {
      throw new Error(`backend lease disappeared without released run evidence: ${state.runId}`);
    }
    const runArtifact = readArtifact(runPath);
    if (!['benchmark_run', 'repair_continuation'].includes(runArtifact.kind)) {
      throw new Error(`backend lease disappeared with unexpected run artifact ${runArtifact.kind}`);
    }
    const payload = runArtifact.payload as { backendLease?: PublicBackendLease | null };
    const lease = payload.backendLease;
    const released = lease?.runId === state.runId && lease.state === 'released'
      && lease.resources.buildContainer?.running === false
      && lease.resources.locks.length > 0
      && lease.resources.locks.every(lock => Boolean(lock.releasedAt));
    if (!released) {
      throw new Error(`backend lease disappeared without released run evidence: ${state.runId}`);
    }
    return;
  }
  const result = recoverSupervisedRun(path, { removeState: false });
  if (!result.ok) throw new Error(`supervisor could not release backend lease ${state.runId}`);
}

interface RecoveryOptions {
  cleanupSucceeded?: boolean;
  retained?: boolean;
  reason?: string | null;
}

interface RecoveryRuntimeOptions {
  runtimeRoot?: string;
}

function trustedRuntimeRoot(runtimeRoot = process.env.STACK_BENCH_RUNTIME_DIR
  ?? join(tmpdir(), 'stack-bench-runtime')): string {
  const root = resolve(runtimeRoot);
  return existsSync(root) ? realpathSync(root) : root;
}

function authorizedRuntimeDirectory(runtimeDir: string, runtimeRoot?: string): string {
  const directory = realpathSync(runtimeDir);
  if (dirname(directory) !== trustedRuntimeRoot(runtimeRoot)) {
    throw new Error('runtime directory is not a direct child of the configured Stack Bench runtime root');
  }
  return directory;
}

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function requiredString(value: Record<string, unknown>, key: string, source: string): string {
  const field = value[key];
  if (typeof field !== 'string' || !field) throw new Error(`${source}.${key} is required`);
  return field;
}

export function validateSupervisorState(
  value: unknown,
  { source = 'supervisor state' }: { source?: string } = {},
): SupervisorState {
  if (!object(value)) throw new Error(`${source} must be an object`);
  const fields = new Set(['version', 'runId', 'backend', 'runtimeDir', 'leasePath',
    'ownershipToken', 'output']);
  for (const key of Object.keys(value)) if (!fields.has(key)) throw new Error(`${source}.${key} is unknown`);
  if (value.version !== SUPERVISOR_STATE_VERSION) throw new Error(`${source}.version is unsupported`);
  const runId = requiredString(value, 'runId', source);
  const backend = requiredString(value, 'backend', source);
  const runtimeDirValue = requiredString(value, 'runtimeDir', source);
  const leasePath = requiredString(value, 'leasePath', source);
  const ownershipToken = requiredString(value, 'ownershipToken', source);
  const output = requiredString(value, 'output', source);
  if (!isAbsolute(leasePath)) throw new Error(`${source}.leasePath must be absolute`);
  if (!isAbsolute(runtimeDirValue)) throw new Error(`${source}.runtimeDir must be absolute`);
  if (!isAbsolute(output)) throw new Error(`${source}.output must be absolute`);
  const runtimeDir = resolve(runtimeDirValue);
  if (resolve(leasePath) !== join(runtimeDir, ARTIFACT_FILE.backendLease)) {
    throw new Error(`${source}.leasePath must be the exact lease below runtimeDir`);
  }
  return {
    version: SUPERVISOR_STATE_VERSION,
    runId,
    backend,
    runtimeDir: runtimeDirValue,
    leasePath,
    ownershipToken,
    output,
  };
}

export function recoveryPlan(
  lease: BackendLease | PublicBackendLease,
  { cleanupSucceeded = false, retained = false, reason = null }: RecoveryOptions = {},
): RecoveryPlan {
  const publicLease = 'ownershipToken' in lease
    ? publicBackendLease(lease)
    : structuredClone(lease);
  const status = cleanupSucceeded ? (retained ? 'retained' : 'clean') : 'quarantined';
  const resources = {
    backendState: publicLease.state,
    buildContainer: publicLease.resources.buildContainer ? {
      id: publicLease.resources.buildContainer.id,
      name: publicLease.resources.buildContainer.name,
      running: publicLease.resources.buildContainer.running !== false,
    } : null,
    listenerProcesses: [...publicLease.resources.listenerProcesses]
      .sort((a, b) => a.pid - b.pid),
    locks: (publicLease.resources.locks ?? []).map(lock => ({ key: lock.key,
      released: Boolean(lock.releasedAt) })).sort((a, b) => a.key.localeCompare(b.key)),
  };
  const instructions = status === 'quarantined' ? [
    'Do not start another run that uses any listed lock key.',
    'Preserve the result directory and private supervisor state; do not publish this attempt.',
    'Run the controller recovery command with the private supervisor-state path.',
    'If recovery still refuses, compare the live container ID and listener PIDs with this record before manual action.',
  ] : status === 'retained' ? [
    'The backend was intentionally retained; do not reuse its lock keys for another run.',
    'Use the private supervisor state to perform authenticated cleanup when inspection is complete.',
  ] : ['No recovery action is required.'];
  return { schemaVersion: 1, status, runId: publicLease.runId, backend: publicLease.backend,
    reason: reason ? String(reason).split(/\r?\n/, 1).join('').slice(0, 1024) : null,
    cleanup: { succeeded: Boolean(cleanupSucceeded), retained: Boolean(retained) },
    resources, instructions };
}

export function writeRecoveryArtifact(
  path: string,
  lease: BackendLease | PublicBackendLease,
  options: RecoveryOptions = {},
) {
  const plan = recoveryPlan(lease, options);
  return writeArtifact(path, { kind: 'recovery', id: `${lease.runId}-recovery`,
    attempt: { id: `${lease.runId}-recovery`, parentId: lease.runId },
    identities: emptyArtifactIdentities({ stackAdapter: { id: lease.backend } }),
    payload: plan });
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function recoverAuthorizedLease(
  state: SupervisorState,
  { statePath = null, removeState = true, runtimeRoot }: {
    statePath?: string | null;
    removeState?: boolean;
    runtimeRoot?: string;
  } = {},
): RecoveryResult {
  const runtimeDir = authorizedRuntimeDirectory(state.runtimeDir, runtimeRoot);
  let lease: BackendLease;
  let cleanupSucceeded = false;
  let reason = null;
  if (existsSync(state.leasePath)) {
    lease = readBackendLease(state.leasePath, { token: state.ownershipToken,
      backend: state.backend, runId: state.runId });
    try { cleanupSucceeded = releaseBackendLease(state.leasePath, state.ownershipToken); }
    catch (error) { reason = errorMessage(error); }
    lease = readBackendLease(state.leasePath, { token: state.ownershipToken,
      backend: state.backend, runId: state.runId });
  } else {
    const evidencePath = join(state.output, ARTIFACT_FILE.backendLease);
    if (!existsSync(evidencePath)) throw new Error('private lease is missing without public lease evidence');
    const evidence = readArtifactPayload<PublicBackendLease>(evidencePath,
      { expectedKind: 'backend_lease_evidence' });
    if (evidence.runId !== state.runId || evidence.backend !== state.backend
      || evidence.state !== 'released') throw new Error('public lease evidence does not prove release');
    cleanupSucceeded = true;
    lease = { ...evidence, ownershipToken: state.ownershipToken };
  }
  writeRecoveryArtifact(join(state.output, ARTIFACT_FILE.recovery), lease,
    { cleanupSucceeded, reason: reason ?? (cleanupSucceeded ? null : 'authenticated cleanup refused') });
  if (cleanupSucceeded) {
    if (existsSync(runtimeDir)) {
      if (existsSync(state.leasePath) && dirname(realpathSync(state.leasePath)) !== runtimeDir) {
        throw new Error('refusing to remove an unexpected runtime directory');
      }
      rmSync(runtimeDir, { recursive: true, force: true });
    }
    if (removeState && statePath) rmSync(statePath, { force: true });
  }
  return { ok: cleanupSucceeded, state: cleanupSucceeded ? 'clean' : 'quarantined',
    runId: state.runId, recoveryPath: join(state.output, ARTIFACT_FILE.recovery) };
}

export function recoverSupervisedRun(
  statePath: string,
  { removeState = true, runtimeRoot }: { removeState?: boolean } & RecoveryRuntimeOptions = {},
): RecoveryResult {
  const absoluteState = realpathSync(statePath);
  if (!statSync(absoluteState).isFile()) throw new Error('supervisor state must be a regular file');
  const state = validateSupervisorState(JSON.parse(readFileSync(absoluteState, 'utf8')),
    { source: absoluteState });
  return recoverAuthorizedLease(state, { statePath: absoluteState, removeState, runtimeRoot });
}

export function recoverBackendLease(leasePath: string, output: string,
  { runtimeRoot }: RecoveryRuntimeOptions = {}): RecoveryResult {
  const absoluteLease = realpathSync(leasePath);
  if (!statSync(absoluteLease).isFile()) throw new Error('backend lease must be a regular file');
  if (basename(absoluteLease) !== ARTIFACT_FILE.backendLease) {
    throw new Error(`backend lease path must end in ${ARTIFACT_FILE.backendLease}`);
  }
  if (!isAbsolute(output)) throw new Error('recovery output must be absolute');
  const runtimeDir = dirname(absoluteLease);
  const absoluteOutput = resolve(output);
  const outputFromRuntime = relative(runtimeDir, absoluteOutput);
  if (!outputFromRuntime || (!outputFromRuntime.startsWith('..') && !isAbsolute(outputFromRuntime))) {
    throw new Error('recovery output must be outside the private runtime directory');
  }
  const lease = readBackendLease(absoluteLease);
  const state = validateSupervisorState({ version: SUPERVISOR_STATE_VERSION,
    runId: lease.runId, backend: lease.backend, runtimeDir, leasePath: absoluteLease,
    ownershipToken: lease.ownershipToken, output: absoluteOutput }, { source: absoluteLease });
  return recoverAuthorizedLease(state, { runtimeRoot });
}
