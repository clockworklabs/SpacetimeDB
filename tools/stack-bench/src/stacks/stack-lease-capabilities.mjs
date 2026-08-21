import { join } from 'node:path';

import { executeStackCapability, STACK_CAPABILITY_SCHEMA_VERSION } from './stack-adapter-contract.mjs';

function capability(id, operations, execute) {
  return { schemaVersion: STACK_CAPABILITY_SCHEMA_VERSION, id, version: '1.0.0', operations, execute };
}

function spacetimeLeaseProvider() {
  return capability('spacetime.lease', ['prepare', 'validate-resources', 'listener-pid', 'capture-listener'], (operation, input) => {
    if (operation === 'prepare') return {
      lease: {
        serverUri: input.serverUri,
        database: null,
        module: input.helpers.moduleName(input.track, input.runIndex),
        dataDir: join(input.runtimeDir, 'spacetime-data'),
        container: null,
      },
      lockKeys: [`listener:${input.serverUri}`],
    };
    if (operation === 'validate-resources') {
      const { resources, helpers } = input;
      helpers.loopbackHttpUri(resources?.serverUri);
      helpers.requireString(resources?.module, 'module');
      helpers.requireString(resources?.dataDir, 'dataDir');
      if (!Array.isArray(resources?.listenerPids)) throw new Error('listenerPids must be an array');
      return;
    }
    const { path, lease, helpers } = input;
    const port = Number(new URL(lease.resources.serverUri).port);
    const actual = helpers.pidsOnPort(port);
    if (actual.length !== 1) throw new Error(`expected one listener on :${port}, found ${actual.length}`);
    if (operation === 'listener-pid') {
      if (!lease.resources.listenerPids.includes(actual[0])) {
        throw new Error(`listener PID ${actual[0]} is not owned by lease ${lease.runId}`);
      }
      return actual[0];
    }
    helpers.updateBackendLease(path,
      { token: lease.ownershipToken, backend: lease.backend, runId: lease.runId }, next => {
        next.resources.listenerPids = actual;
        next.state = 'active';
        return next;
      });
    return actual[0];
  });
}

function hostedLeaseProvider(adapterId) {
  const envKey = adapterId === 'postgres' ? 'POSTGRES_CONTAINER' : 'MONGO_CONTAINER';
  const defaultContainer = `stack-bench-${adapterId}`;
  return capability(`${adapterId}.lease`, ['prepare', 'validate-resources'], (operation, input) => {
    if (operation === 'prepare') return {
      lease: {
        serverUri: null,
        database: input.helpers.dbName(input.track, input.runIndex),
        module: null,
        dataDir: null,
        container: input.helpers.containerIdentity(input.env[envKey] ?? defaultContainer),
      },
      lockKeys: [],
    };
    const { resources, helpers } = input;
    helpers.requireString(resources?.database, 'database');
    helpers.requireString(resources?.container?.name, 'container.name');
    helpers.requireString(resources?.container?.id, 'container.id');
  });
}

function resourceFreeLeaseProvider(adapterId) {
  return capability(`${adapterId}.lease`, ['prepare', 'validate-resources'], operation => operation === 'prepare'
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

export function stackLeaseCapability(backend) {
  const lease = STACK_LEASE_CAPABILITIES.get(backend);
  if (!lease) throw new Error(`unknown stack adapter ${JSON.stringify(backend)}`);
  return lease;
}

export function executeStackLeaseCapability(backend, operation, input = {}) {
  return executeStackCapability({ id: backend,
    capabilities: { lease: stackLeaseCapability(backend) } }, 'lease', operation, input);
}
