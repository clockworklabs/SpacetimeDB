// Lease-authenticated database reset used by the grading orchestrator.

import { leaseFromEnv } from '../runtime/backend-lease.js';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.js';
import { requireLeasedDatabase, requireLeasedSpacetime } from './backend-reset-guard.js';
import type { TextCommandExecutor } from '../runtime/command-executor.js';

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

// Candidate rollback must remove schema changes too. Ordinary probe resets
// retain the PostgreSQL schema; durability probes do not call either reset.
export function resetRepairBackend(input: BackendResetRequest): unknown {
  if (input.backend !== 'postgres') return resetBackend(input);
  const { lease } = leaseFromEnv(process.env, { backend: input.backend, active: true });
  const database = requireLeasedDatabase(lease);
  return STACK_ADAPTER_REGISTRY.get('postgres').database.prepare({
    lease: database, name: database.resources.database,
    expectedName: database.resources.database, wipe: true,
    ...(input.exec ? { exec: input.exec } : {}),
  });
}
