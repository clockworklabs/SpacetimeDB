// Lease-authenticated database reset used by the grading orchestrator.

import { execFileSync } from 'node:child_process';
import { leaseFromEnv } from '../runtime/backend-lease.js';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.js';
import { requireLeasedDatabase, requireLeasedSpacetime } from './backend-reset-guard.js';
import type { TextCommandExecutor } from '../runtime/command-executor.js';
import { leasedSpacetimeTarget } from '../runtime/spacetime-target.js';
import { prepareSpacetimeDatabase } from './backends/spacetime-operations.js';
import { CODING_CONTAINER_SPACETIME_CLI, codingContainerAgentCommand, codingContainerAgentExecOptions }
  from '../runtime/coding-container-policy.js';

export const GENERATED_APP_LAYOUT_EXIT_CODE = 10;

interface BackendResetRequest {
  backend: string;
  app: string;
  exec?: TextCommandExecutor;
}

export function resetBackend({ backend, app, exec }: BackendResetRequest): unknown {
  const { lease } = leaseFromEnv(process.env, { backend, active: true });
  const adapter = STACK_ADAPTER_REGISTRY.get(backend);
  const input = { app, ...(exec ? { exec } : {}) };
  if (adapter.id === 'convex') return adapter.reset.run();
  if (adapter.id === 'postgres' || adapter.id === 'mongodb') {
    return adapter.reset.run({ ...input, lease: requireLeasedDatabase(lease) });
  }
  if (adapter.id === 'spacetime') {
    return adapter.reset.run({ ...input, lease: requireLeasedSpacetime(lease) });
  }
  throw new Error(`stack adapter ${backend} does not support reset`);
}

// Candidate rollback and isolated scenarios both start from a fresh database.
// Durability probes do not call either reset.
export function resetRepairBackend(input: BackendResetRequest): unknown {
  if (input.backend === 'spacetime') {
    const exec: TextCommandExecutor = input.exec ?? execFileSync;
    const target = leasedSpacetimeTarget({ requireBuildContainer: true, exec });
    const container = target.buildContainer;
    if (!container) throw new Error('leased build container is not active');
    // Clean restoration has no installed dependencies yet. Remove the rejected
    // module; accepted startup installs, builds and publishes its own schema.
    return prepareSpacetimeDatabase({
      lease: { resources: { module: target.mod, serverUri: target.containerUri } },
      name: target.mod, wipe: true, cli: CODING_CONTAINER_SPACETIME_CLI,
      expectedModule: target.mod, expectedServerUri: target.containerUri,
      exec: (cli, args, options) => exec('docker', ['exec', ...codingContainerAgentExecOptions(),
        container.id, ...codingContainerAgentCommand(cli, args)], options),
    });
  }
  return resetBackend(input);
}
