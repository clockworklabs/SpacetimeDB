import { executeStackCapability, STACK_ADAPTER_SCHEMA_VERSION,
  STACK_CAPABILITY_SCHEMA_VERSION } from './stack-adapter-contract.mjs';
import { controlHosted } from './stack-lifecycle-operations.mjs';

const PORT_BASES = Object.freeze({
  spacetime: Object.freeze({ vite: 6173 }),
  postgres: Object.freeze({ vite: 6273, express: 6001, db: 6532 }),
  mongodb: Object.freeze({ vite: 6373, express: 6101, db: 6537 }),
  stub: Object.freeze({ vite: 7000 }),
});

export function capability(id, operations, execute) {
  return { schemaVersion: STACK_CAPABILITY_SCHEMA_VERSION, id, version: '1.0.0', operations, execute };
}

function portsProvider(adapterId) {
  const allocations = PORT_BASES[adapterId];
  return capability(`${adapterId}.ports`, ['allocations', 'for-run'], (operation, input) => {
    if (operation === 'allocations') return { ...allocations };
    if (!Number.isInteger(input.trackOffset) || input.trackOffset < 0
      || !Number.isInteger(input.runIndex) || input.runIndex < 0) {
      throw new Error(`${adapterId} ports require non-negative integer trackOffset and runIndex`);
    }
    const offset = Number(input.trackOffset) + Number(input.runIndex);
    return {
      vite: allocations.vite + offset,
      express: allocations.express ? allocations.express + offset : null,
      dbPort: allocations.db ?? null,
    };
  });
}

export function operationProvider(adapterId, name, operations) {
  return capability(`${adapterId}.${name}`, Object.keys(operations), (operation, input) =>
    operations[operation](input));
}

export function runPolicyProvider(adapterId, values) {
  return operationProvider(adapterId, 'run-policy', Object.fromEntries(
    Object.entries(values).map(([name, value]) => [name, typeof value === 'function' ? value : () => value])));
}

export function defineStackAdapter(id, lease, capabilities = {}, { version }) {
  return {
    schemaVersion: STACK_ADAPTER_SCHEMA_VERSION,
    id, version,
    capabilities: { ports: portsProvider(id), lease, ...capabilities },
  };
}

export function leasedDatabaseEnvironment(adapter, { database, networkMode }) {
  const dbPort = executeStackCapability(adapter, 'ports', 'allocations').db;
  if (!dbPort) return {};
  const databaseUrl = executeStackCapability(adapter, 'agent', 'connection-url', {
    dbPort, database,
    hostUrl: url => url.replace(/127\.0\.0\.1|localhost/g,
      networkMode === 'host' ? '127.0.0.1' : 'host.docker.internal'),
  });
  if (typeof databaseUrl !== 'string' || !databaseUrl) {
    throw new Error(`stack adapter ${adapter.id} did not provide its leased database URL`);
  }
  return { DATABASE_URL: databaseUrl };
}

export function controlHostedFor(adapter, input) {
  return controlHosted({ ...input, environment: leasedDatabaseEnvironment(adapter, {
    database: input.lease.resources.database,
    networkMode: input.lease.resources.buildContainer?.networkMode,
  }) });
}
