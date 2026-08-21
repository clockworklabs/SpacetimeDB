// Lease-authenticated database reset used by the grading orchestrator.

import { leaseFromEnv } from '../runtime/backend-lease.mjs';
import { executeStackCapability } from './stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.mjs';

export const GENERATED_APP_LAYOUT_EXIT_CODE = 10;

export function resetBackend({ backend, app, exec }) {
  const { lease } = leaseFromEnv(process.env, { backend, active: true });
  return executeStackCapability(STACK_ADAPTER_REGISTRY.get(backend), 'reset', 'run',
    { lease, app, ...(exec ? { exec } : {}) });
}
