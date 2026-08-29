import { join } from 'node:path';

import { executeStackCapability, STACK_CAPABILITY_SCHEMA_VERSION } from './stack-adapter-contract.js';

import type { StackCapability } from './stack-adapter-contract.js';
import type { BackendLease } from '../runtime/backend-lease.js';
import type { Track } from '../composition/tracks.js';

// Preparing and validating a lease is dispatched through the capability
// contract, so each provider states the input it reads for itself.
interface LeaseHelpers {
  moduleName: (track: Track, runIndex: number) => string;
  dbName: (track: Track, runIndex: number) => string;
  containerIdentity: (name: string) => { id: string; name: string };
  loopbackHttpUri: (uri: unknown) => unknown;
  requireString: (value: unknown, label: string) => unknown;
  pidsOnPort: (port: number) => unknown[];
  updateBackendLease: (path: string, expected: { token: string; backend: string; runId: string },
    update: (lease: BackendLease) => BackendLease) => unknown;
}

interface LeaseInput {
  serverUri?: string;
  track?: Track;
  runIndex?: number;
  runtimeDir?: string;
  env?: NodeJS.ProcessEnv;
  path?: string;
  lease?: BackendLease;
  resources?: BackendLease['resources'];
  helpers?: LeaseHelpers;
}

function leaseInput(input: unknown): LeaseInput {
  if (input === null || typeof input !== 'object') {
    throw new Error('stack lease capability requires an input object');
  }
  return input;
}

function requireTrack(input: LeaseInput): Track {
  const { track } = input;
  if (!track) throw new Error('stack lease capability requires a track');
  return track;
}

function requireRunIndex(input: LeaseInput): number {
  const { runIndex } = input;
  if (typeof runIndex !== 'number') throw new Error('stack lease capability requires a run index');
  return runIndex;
}

function requireLease(input: LeaseInput): BackendLease {
  const { lease } = input;
  if (!lease) throw new Error('stack lease capability requires a lease');
  return lease;
}

function leaseHelpers(input: LeaseInput): LeaseHelpers {
  const { helpers } = input;
  if (!helpers) throw new Error('stack lease capability requires its helpers');
  return helpers;
}


function capability(id: string, operations: readonly string[],
  execute: (operation: string, input: unknown) => unknown): StackCapability {
  return { schemaVersion: STACK_CAPABILITY_SCHEMA_VERSION, id, version: '1.0.0', operations, execute };
}

function spacetimeLeaseProvider(): StackCapability {
  return capability('spacetime.lease', ['prepare', 'validate-resources', 'listener-pid', 'capture-listener'], (operation, rawInput) => {
    const input = leaseInput(rawInput);
    const helpers = leaseHelpers(input);
    if (operation === 'prepare') return {
      lease: {
        serverUri: input.serverUri,
        database: null,
        module: helpers.moduleName(requireTrack(input), requireRunIndex(input)),
        dataDir: join(String(input.runtimeDir), 'spacetime-data'),
        container: null,
      },
      lockKeys: [`listener:${input.serverUri}`],
    };
    if (operation === 'validate-resources') {
      const { resources } = input;
      helpers.loopbackHttpUri(resources?.serverUri);
      helpers.requireString(resources?.module, 'module');
      helpers.requireString(resources?.dataDir, 'dataDir');
      if (!Array.isArray(resources?.listenerPids)) throw new Error('listenerPids must be an array');
      return;
    }
    const { path } = input;
    const lease = requireLease(input);
    const port = Number(new URL(String(lease.resources.serverUri)).port);
    const actual = helpers.pidsOnPort(port);
    if (actual.length !== 1) throw new Error(`expected one listener on :${port}, found ${actual.length}`);
    if (operation === 'listener-pid') {
      if (!(lease.resources.listenerPids ?? []).includes(Number(actual[0]))) {
        throw new Error(`listener PID ${actual[0]} is not owned by lease ${lease.runId}`);
      }
      return actual[0];
    }
    helpers.updateBackendLease(String(path),
      { token: lease.ownershipToken, backend: lease.backend, runId: lease.runId }, next => {
        next.resources.listenerPids = actual.map(Number);
        next.state = 'active';
        return next;
      });
    return actual[0];
  });
}

function hostedLeaseProvider(adapterId: string): StackCapability {
  const envKey = adapterId === 'postgres' ? 'POSTGRES_CONTAINER' : 'MONGO_CONTAINER';
  const defaultContainer = `stack-bench-${adapterId}`;
  return capability(`${adapterId}.lease`, ['prepare', 'validate-resources'], (operation, rawInput) => {
    const input = leaseInput(rawInput);
    const helpers = leaseHelpers(input);
    if (operation === 'prepare') return {
      lease: {
        serverUri: null,
        database: helpers.dbName(requireTrack(input), requireRunIndex(input)),
        module: null,
        dataDir: null,
        container: helpers.containerIdentity(input.env?.[envKey] ?? defaultContainer),
      },
      lockKeys: [],
    };
    const { resources } = input;
    helpers.requireString(resources?.database, 'database');
    helpers.requireString(resources?.container?.name, 'container.name');
    helpers.requireString(resources?.container?.id, 'container.id');
  });
}

function resourceFreeLeaseProvider(adapterId: string): StackCapability {
  return capability(`${adapterId}.lease`, ['prepare', 'validate-resources'], (operation: string) => operation === 'prepare'
    ? { lease: { serverUri: null, database: null, module: null, dataDir: null, container: null },
      lockKeys: [] }
    : undefined);
}

export const STACK_LEASE_CAPABILITIES = new Map([
  ['spacetime', spacetimeLeaseProvider()],
  ['postgres', hostedLeaseProvider('postgres')],
  ['mongodb', hostedLeaseProvider('mongodb')],
  ['stub', resourceFreeLeaseProvider('stub')],
]);

export function stackLeaseCapability(backend: string): StackCapability {
  const lease = STACK_LEASE_CAPABILITIES.get(backend);
  if (!lease) throw new Error(`unknown stack adapter ${JSON.stringify(backend)}`);
  return lease;
}

export function executeStackLeaseCapability(backend: string, operation: string,
  input: Record<string, unknown> = {}): unknown {
  return executeStackCapability({ id: backend,
    capabilities: { lease: stackLeaseCapability(backend) } }, 'lease', operation, input);
}
