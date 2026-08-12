#!/usr/bin/env node

import { existsSync, readFileSync, realpathSync, rmSync, statSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { emptyArtifactIdentities, readArtifactPayload, writeArtifact } from './artifacts.mjs';
import { publicBackendLease, readBackendLease } from './backend-lease.mjs';
import { releaseBackendLease } from './backend-teardown.mjs';

export const SUPERVISOR_STATE_VERSION = 2;

function object(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function validateSupervisorState(value, { source = 'supervisor state' } = {}) {
  if (!object(value)) throw new Error(`${source} must be an object`);
  const fields = new Set(['version', 'runId', 'backend', 'runtimeDir', 'leasePath',
    'ownershipToken', 'output']);
  for (const key of Object.keys(value)) if (!fields.has(key)) throw new Error(`${source}.${key} is unknown`);
  if (value.version !== SUPERVISOR_STATE_VERSION) throw new Error(`${source}.version is unsupported`);
  for (const key of ['runId', 'backend', 'runtimeDir', 'leasePath', 'ownershipToken', 'output']) {
    if (typeof value[key] !== 'string' || !value[key]) throw new Error(`${source}.${key} is required`);
  }
  if (!isAbsolute(value.leasePath)) throw new Error(`${source}.leasePath must be absolute`);
  if (!isAbsolute(value.runtimeDir)) throw new Error(`${source}.runtimeDir must be absolute`);
  if (!isAbsolute(value.output)) throw new Error(`${source}.output must be absolute`);
  const runtimeDir = resolve(value.runtimeDir);
  if (resolve(value.leasePath) !== join(runtimeDir, 'backend-lease.json')) {
    throw new Error(`${source}.leasePath must be the exact lease below runtimeDir`);
  }
  return structuredClone(value);
}

export function recoveryPlan(lease, { cleanupSucceeded, retained = false, reason = null } = {}) {
  const publicLease = lease.ownershipToken ? publicBackendLease(lease) : structuredClone(lease);
  const status = cleanupSucceeded ? (retained ? 'retained' : 'clean') : 'quarantined';
  const resources = {
    backendState: publicLease.state,
    buildContainer: publicLease.resources.buildContainer ? {
      id: publicLease.resources.buildContainer.id,
      name: publicLease.resources.buildContainer.name,
      running: publicLease.resources.buildContainer.running !== false,
    } : null,
    listenerPids: [...(publicLease.resources.listenerPids ?? [])].map(String).sort(),
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
    reason: reason ? String(reason).split(/\r?\n/)[0].slice(0, 1024) : null,
    cleanup: { succeeded: Boolean(cleanupSucceeded), retained: Boolean(retained) },
    resources, instructions };
}

export function writeRecoveryArtifact(path, lease, options = {}) {
  const plan = recoveryPlan(lease, options);
  return writeArtifact(path, { kind: 'recovery', id: `${lease.runId}-recovery`,
    attempt: { id: `${lease.runId}-recovery`, parentId: lease.runId },
    identities: emptyArtifactIdentities({ stackAdapter: { id: lease.backend } }),
    payload: plan });
}

export function recoverSupervisedRun(statePath, { removeState = true } = {}) {
  const absoluteState = realpathSync(statePath);
  if (!statSync(absoluteState).isFile()) throw new Error('supervisor state must be a regular file');
  const state = validateSupervisorState(JSON.parse(readFileSync(absoluteState, 'utf8')),
    { source: absoluteState });
  let lease;
  let cleanupSucceeded = false;
  let reason = null;
  if (existsSync(state.leasePath)) {
    lease = readBackendLease(state.leasePath, { token: state.ownershipToken,
      backend: state.backend, runId: state.runId });
    try { cleanupSucceeded = releaseBackendLease(state.leasePath, state.ownershipToken); }
    catch (error) { reason = error.message; }
    lease = readBackendLease(state.leasePath, { token: state.ownershipToken,
      backend: state.backend, runId: state.runId });
  } else {
    const evidencePath = join(state.output, 'backend-lease.json');
    if (!existsSync(evidencePath)) throw new Error('private lease is missing without public lease evidence');
    const evidence = readArtifactPayload(evidencePath, { expectedKind: 'backend_lease_evidence' });
    if (evidence.runId !== state.runId || evidence.backend !== state.backend
      || evidence.state !== 'released') throw new Error('public lease evidence does not prove release');
    cleanupSucceeded = true;
    lease = { ...evidence, ownershipToken: state.ownershipToken };
  }
  writeRecoveryArtifact(join(state.output, 'recovery.json'), lease,
    { cleanupSucceeded, reason: reason ?? (cleanupSucceeded ? null : 'authenticated cleanup refused') });
  if (cleanupSucceeded) {
    if (existsSync(state.runtimeDir)) {
      const runtimeDir = realpathSync(state.runtimeDir);
      if (dirname(runtimeDir) === runtimeDir) {
        throw new Error('refusing to remove a filesystem root as a runtime directory');
      }
      if (existsSync(state.leasePath) && dirname(realpathSync(state.leasePath)) !== runtimeDir) {
        throw new Error('refusing to remove an unexpected runtime directory');
      }
      rmSync(runtimeDir, { recursive: true, force: true });
    }
    if (removeState) rmSync(absoluteState, { force: true });
  }
  return { ok: cleanupSucceeded, state: cleanupSucceeded ? 'clean' : 'quarantined',
    runId: state.runId, recoveryPath: join(state.output, 'recovery.json') };
}

function main() {
  const [command, statePath] = process.argv.slice(2);
  if (command !== 'recover' || !statePath || process.argv.length !== 4) {
    console.error('Usage: node recovery.mjs recover <private-supervisor-state.json>');
    process.exit(2);
  }
  const result = recoverSupervisedRun(statePath);
  console.log(JSON.stringify(result, null, 2));
  process.exitCode = result.ok ? 0 : 1;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try { main(); }
  catch (error) { console.error(`recovery: ${error.message}`); process.exitCode = 2; }
}
