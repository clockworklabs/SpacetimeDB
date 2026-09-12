// Lease-authenticated database reset used by the grading orchestrator.

import { leaseFromEnv } from '../runtime/backend-lease.js';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.js';
import { requireLeasedDatabase, requireLeasedSpacetime } from './backend-reset-guard.js';
import type { TextCommandExecutor } from '../runtime/command-executor.js';
import { handoffHostedWorkspace } from './hosted-lifecycle.js';

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
    const { lease } = leaseFromEnv(process.env, { backend: input.backend, active: true });
    // Publish runs as the agent before the normal app-start handoff.
    handoffHostedWorkspace(lease, input.exec);
  }
  return resetBackend(input);
}
