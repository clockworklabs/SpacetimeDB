import { executeStackCapability, STACK_ADAPTER_SCHEMA_VERSION,
  STACK_CAPABILITY_SCHEMA_VERSION } from './stack-adapter-contract.js';
import type { StackAdapter, StackCapability, StackOperationHandler } from './stack-adapter-contract.js';
import { controlHostedAppServer } from './stack-lifecycle-operations.js';
import type { TextCommandExecutor } from '../runtime/command-executor.js';

type AdapterId = keyof typeof PORT_BASES;
type OperationMap = Readonly<Record<string, StackOperationHandler>>;
type UnknownRecord = Record<string, unknown>;

const record = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const operationHandler = (value: unknown): value is StackOperationHandler =>
  typeof value === 'function';
const commandExecutor = (value: unknown): value is TextCommandExecutor =>
  typeof value === 'function';

function requireRecord(value: unknown, at: string): UnknownRecord {
  if (!record(value)) throw new Error(`${at} must be an object`);
  return value;
}

const PORT_BASES = Object.freeze({
  spacetime: Object.freeze({ vite: 6173 }),
  postgres: Object.freeze({ vite: 6273, express: 6001, db: 6532 }),
  mongodb: Object.freeze({ vite: 6373, express: 6101, db: 6537 }),
  stub: Object.freeze({ vite: 7000 }),
});

export function capability(id: string, operations: readonly string[],
  execute: (operation: string, input: unknown) => unknown): StackCapability {
  return { schemaVersion: STACK_CAPABILITY_SCHEMA_VERSION, id, version: '1.0.0', operations, execute };
}

function portsProvider(adapterId: AdapterId): StackCapability {
  const allocations = PORT_BASES[adapterId];
  return capability(`${adapterId}.ports`, ['allocations', 'for-run'], (operation, input) => {
    if (operation === 'allocations') return { ...allocations };
    const request = requireRecord(input, `${adapterId} ports input`);
    if (!Number.isInteger(request.trackOffset) || Number(request.trackOffset) < 0
      || !Number.isInteger(request.runIndex) || Number(request.runIndex) < 0) {
      throw new Error(`${adapterId} ports require non-negative integer trackOffset and runIndex`);
    }
    const offset = Number(request.trackOffset) + Number(request.runIndex);
    return {
      vite: allocations.vite + offset,
      express: 'express' in allocations ? allocations.express + offset : null,
      dbPort: 'db' in allocations ? allocations.db : null,
    };
  });
}

export function operationProvider(adapterId: AdapterId, name: string,
  operations: OperationMap): StackCapability {
  return capability(`${adapterId}.${name}`, Object.keys(operations), (operation, input) => {
    const execute = operations[operation];
    if (!execute) throw new Error(`${adapterId}.${name} does not implement ${operation}`);
    return Reflect.apply(execute, operations, [input]);
  });
}

export function runPolicyProvider(adapterId: AdapterId,
  values: Readonly<Record<string, unknown | StackOperationHandler>>): StackCapability {
  const operations: Record<string, StackOperationHandler> = {};
  for (const [name, value] of Object.entries(values)) {
    operations[name] = operationHandler(value) ? value : () => value;
  }
  return operationProvider(adapterId, 'run-policy', operations);
}

export function defineStackAdapter(id: AdapterId, lease: StackCapability,
  capabilities: Readonly<Record<string, StackCapability>> = {},
  { version }: { version: string }): StackAdapter {
  return {
    schemaVersion: STACK_ADAPTER_SCHEMA_VERSION,
    id, version,
    capabilities: { ports: portsProvider(id), lease, ...capabilities },
  };
}

export function leasedDatabaseEnvironment(adapter: StackAdapter, { database, networkMode }: {
  database: string | null; networkMode: string | null | undefined;
}): Record<string, string> {
  const allocations = executeStackCapability(adapter, 'ports', 'allocations');
  const dbPort = record(allocations) ? allocations.db : null;
  if (!dbPort) return {};
  const databaseUrl = executeStackCapability(adapter, 'agent', 'connection-url', {
    dbPort, database,
    hostUrl: (url: string) => url.replace(/127\.0\.0\.1|localhost/g,
      networkMode === 'host' ? '127.0.0.1' : 'host.docker.internal'),
  });
  if (typeof databaseUrl !== 'string' || !databaseUrl) {
    throw new Error(`stack adapter ${adapter.id} did not provide its leased database URL`);
  }
  return { DATABASE_URL: databaseUrl };
}

export function controlHostedFor(adapter: StackAdapter, input: unknown): unknown {
  const request = requireRecord(input, 'hosted backend control input');
  const lease = requireRecord(request.lease, 'hosted backend control lease');
  const resources = requireRecord(lease.resources, 'hosted backend control resources');
  const buildContainer = record(resources.buildContainer) ? resources.buildContainer : null;
  const database = resources.database;
  if (database !== null && typeof database !== 'string') {
    throw new Error('hosted backend control database must be a string or null');
  }
  const networkMode = buildContainer?.networkMode;
  if (networkMode !== null && networkMode !== undefined && typeof networkMode !== 'string') {
    throw new Error('hosted backend control network mode must be a string or null');
  }
  const exec = request.exec;
  return controlHostedAppServer({
    adapterId: request.adapterId,
    app: request.app,
    port: request.port,
    probe: request.probe,
    mode: request.mode,
    signal: request.signal,
    // Tests inject a command double through the dispatch; the shape cannot be
    // proven structurally, so the boundary trusts any function here.
    ...(commandExecutor(exec) ? { exec } : {}),
    lease: { resources },
    environment: {
      ...leasedDatabaseEnvironment(adapter, { database, networkMode }),
      VITE_PORT: String(request.port),
    },
  });
}
