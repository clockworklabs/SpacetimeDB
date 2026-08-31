// Lease-authenticated database reset used by the grading orchestrator.

import { leaseFromEnv } from '../runtime/backend-lease.js';
import { executeStackCapability } from './stack-adapter-contract.js';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.js';
import type { TextCommandExecutor } from '../runtime/command-executor.js';

export const GENERATED_APP_LAYOUT_EXIT_CODE = 10;

export interface BackendResetRequest {
  backend: string;
  app: string;
  exec?: TextCommandExecutor;
}

export function resetBackend({ backend, app, exec }: BackendResetRequest): unknown {
  const { lease } = leaseFromEnv(process.env, { backend, active: true });
  return executeStackCapability(STACK_ADAPTER_REGISTRY.get(backend), 'reset', 'run',
    { lease, app, ...(exec ? { exec } : {}) });
}
