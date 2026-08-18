#!/usr/bin/env node
import { pidsOnPort } from '../src/runtime/platform.mjs';
import { leaseFromEnv, updateBackendLease } from '../src/runtime/backend-lease.mjs';
import { executeStackCapability } from '../src/stacks/stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.mjs';

const [command, requestedBackend] = process.argv.slice(2);

function current(expected = {}) {
  return leaseFromEnv(process.env, { backend: requestedBackend, ...expected });
}

try {
  if (command === 'validate') {
    current({ active: true });
  } else if (command === 'field') {
    const field = process.argv[4];
    const { lease } = current({ active: true });
    const fields = {
      serverUri: lease.resources.serverUri,
      serverPort: lease.resources.serverUri ? new URL(lease.resources.serverUri).port : null,
      dataDir: lease.resources.dataDir,
      module: lease.resources.module,
      database: lease.resources.database,
      containerName: lease.resources.container?.name,
      containerId: lease.resources.container?.id,
      buildContainerName: lease.resources.buildContainer?.name,
      buildContainerId: lease.resources.buildContainer?.id,
    };
    if (!(field in fields) || fields[field] == null) throw new Error(`field is unavailable: ${field}`);
    process.stdout.write(String(fields[field]));
  } else if (command === 'listener-pid') {
    const { path, lease } = current({ active: true });
    const adapter = STACK_ADAPTER_REGISTRY.get(lease.backend);
    process.stdout.write(String(executeStackCapability(adapter, 'lease', 'listener-pid',
      { path, lease, helpers: { pidsOnPort, updateBackendLease } })));
  } else if (command === 'mark-restarting') {
    const { path, lease } = current({ active: true });
    updateBackendLease(path, { token: lease.ownershipToken, backend: lease.backend, runId: lease.runId }, next => {
      next.state = 'restarting';
      return next;
    });
  } else if (command === 'capture-listener') {
    const { path, lease } = current();
    const adapter = STACK_ADAPTER_REGISTRY.get(lease.backend);
    process.stdout.write(String(executeStackCapability(adapter, 'lease', 'capture-listener',
      { path, lease, helpers: { pidsOnPort, updateBackendLease } })));
  } else {
    throw new Error('usage: lease-cli.mjs validate|field|listener-pid|mark-restarting|capture-listener <backend> [field]');
  }
} catch (error) {
  console.error(error.message);
  process.exit(3);
}
