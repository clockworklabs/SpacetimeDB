import { join } from 'node:path';

import { databaseContainerName } from './database-containers.js';
import type { Track } from '../composition/tracks.js';

interface StackLeasePrepareInput {
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

interface StackLeaseValidationInput {
  resources: unknown;
  helpers: {
    loopbackHttpUri(uri: unknown): unknown;
    requireString(value: unknown, label: string): unknown;
  };
}

interface StackLeasePreparation {
  lease: {
    serverUri: string | null;
    database: string | null;
    module: string | null;
    dataDir: string | null;
    container: { id: string; name: string } | null;
  };
  lockKeys: string[];
}

function resources(input: StackLeaseValidationInput): Record<string, unknown> {
  if (!input.resources || typeof input.resources !== 'object' || Array.isArray(input.resources)) {
    throw new Error('stack lease resources must be an object');
  }
  return input.resources as Record<string, unknown>;
}

function container(input: StackLeaseValidationInput): Record<string, unknown> {
  const value = resources(input).container;
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('stack lease container must be an object');
  }
  return value as Record<string, unknown>;
}

function validateHostedResources(input: StackLeaseValidationInput): void {
  input.helpers.requireString(resources(input).database, 'database');
  input.helpers.requireString(container(input).name, 'container.name');
  input.helpers.requireString(container(input).id, 'container.id');
}

function hostedLease(adapterId: 'postgres' | 'mongodb') {
  return {
    prepare(input: StackLeasePrepareInput): StackLeasePreparation {
      return {
        lease: {
          serverUri: null,
          database: input.helpers.dbName(input.track, input.runIndex),
          module: null,
          dataDir: null,
          container: input.helpers.containerIdentity(databaseContainerName(adapterId, input.env)),
        },
        lockKeys: [],
      };
    },
    validateResources: validateHostedResources,
  };
}

function spacetimeLease() {
  return {
    prepare(input: StackLeasePrepareInput): StackLeasePreparation {
      return {
        lease: {
          serverUri: input.serverUri,
          database: null,
          module: input.helpers.moduleName(input.track, input.runIndex),
          dataDir: join(input.runtimeDir, 'spacetime-data'),
          container: null,
        },
        lockKeys: [`listener:${input.serverUri}`],
      };
    },
    validateResources(input: StackLeaseValidationInput): void {
      const value = resources(input);
      input.helpers.loopbackHttpUri(value.serverUri);
      input.helpers.requireString(value.module, 'module');
      input.helpers.requireString(value.dataDir, 'dataDir');
      if (!Array.isArray(value.listenerProcesses)) {
        throw new Error('listenerProcesses must be an array');
      }
    },
  };
}

const STACK_LEASES = {
  spacetime: spacetimeLease(),
  postgres: hostedLease('postgres'),
  mongodb: hostedLease('mongodb'),
  stub: {
    prepare: (_input: StackLeasePrepareInput): StackLeasePreparation => ({
      lease: { serverUri: null, database: null, module: null, dataDir: null, container: null },
      lockKeys: [],
    }),
    validateResources: (_input: StackLeaseValidationInput): void => undefined,
  },
} as const;

export function stackLeaseOperations(backend: keyof typeof STACK_LEASES) {
  return STACK_LEASES[backend];
}

export function validateStackLeaseResources(backend: string, input: StackLeaseValidationInput): void {
  if (!(backend in STACK_LEASES)) throw new Error(`unknown stack adapter ${JSON.stringify(backend)}`);
  STACK_LEASES[backend as keyof typeof STACK_LEASES].validateResources(input);
}
