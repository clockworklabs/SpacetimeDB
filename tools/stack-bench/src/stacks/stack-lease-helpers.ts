import { databaseContainerName } from './database-containers.js';
import type { Track } from '../composition/tracks.js';

// The lease shape a stack prepares and validates. Each stack declares its own in
// its identity module; stack-identities.ts registers it.
export interface StackLeasePrepareInput {
  serverUri: string | null;
  track: Track;
  runIndex: number;
  runtimeDir: string;
  env?: NodeJS.ProcessEnv;
  helpers: {
    moduleName(track: Track, runIndex: number): string;
    dbName(track: Track, runIndex: number): string;
    containerIdentity(name: string): { id: string; name: string };
  };
}

export interface StackLeaseValidationInput {
  resources: unknown;
  helpers: {
    loopbackHttpUri(uri: unknown): unknown;
    requireString(value: unknown, label: string): unknown;
  };
}

export interface StackLeasePreparation {
  lease: {
    serverUri: string | null;
    database: string | null;
    module: string | null;
    dataDir: string | null;
    container: { id: string; name: string } | null;
  };
  lockKeys: string[];
}

export interface StackLeaseCapability {
  prepare(input: StackLeasePrepareInput): StackLeasePreparation;
  validateResources(input: StackLeaseValidationInput): void;
}

export function leaseResources(input: StackLeaseValidationInput): Record<string, unknown> {
  if (!input.resources || typeof input.resources !== 'object' || Array.isArray(input.resources)) {
    throw new Error('stack lease resources must be an object');
  }
  return input.resources as Record<string, unknown>;
}

export function leaseContainer(input: StackLeaseValidationInput): Record<string, unknown> {
  const value = leaseResources(input).container;
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('stack lease container must be an object');
  }
  return value as Record<string, unknown>;
}

function validateHostedResources(input: StackLeaseValidationInput): void {
  input.helpers.requireString(leaseResources(input).database, 'database');
  if (leaseResources(input).container === null) return;
  input.helpers.requireString(leaseContainer(input).name, 'container.name');
  input.helpers.requireString(leaseContainer(input).id, 'container.id');
}

// A database stack served by a shared development container or, in the
// appliance, by an attempt-owned container created at activation.
export function hostedLease(adapterId: string): StackLeaseCapability {
  return {
    prepare(input: StackLeasePrepareInput): StackLeasePreparation {
      return {
        lease: {
          serverUri: null,
          database: input.helpers.dbName(input.track, input.runIndex),
          module: null,
          dataDir: null,
          container: input.env?.STACK_BENCH_APPLIANCE === '1' ? null
            : input.helpers.containerIdentity(databaseContainerName(adapterId, input.env)),
        },
        lockKeys: [],
      };
    },
    validateResources: validateHostedResources,
  };
}

// A platform served at serverUri by an attempt-owned container, created at activation.
export function ownedPlatformLease({ label, database = null }: { label: string; database?: string | null }):
  StackLeaseCapability {
  return {
    prepare(input) {
      return { lease: { serverUri: input.serverUri, database, module: null,
        dataDir: null, container: null }, lockKeys: [] };
    },
    validateResources(input) {
      const value = leaseResources(input);
      input.helpers.loopbackHttpUri(value.serverUri);
      if (database !== null && value.database !== database) throw new Error(`${label} leases the ${database} database`);
      if (value.container !== null) {
        const owned = leaseContainer(input);
        if (owned.owned !== true) throw new Error(`${label} requires an owned container`);
        input.helpers.requireString(owned.name, 'container.name');
        input.helpers.requireString(owned.id, 'container.id');
      }
    },
  };
}
