// Lease-authenticated database reset used by the grading orchestrator.

import { leaseFromEnv } from '../runtime/backend-lease.js';
import { executeStackCapability } from './stack-adapter-contract.js';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.js';

export const GENERATED_APP_LAYOUT_EXIT_CODE = 10;

export type BackendResetExecutor = (
  file: string,
  args: string[],
  options?: Record<string, unknown>,
) => unknown;

export interface BackendResetRequest {
  backend: string;
  app: string;
  exec?: BackendResetExecutor;
}

export function resetBackend({ backend, app, exec }: BackendResetRequest): unknown {
  const { lease } = leaseFromEnv(process.env, { backend, active: true });
  return executeStackCapability(STACK_ADAPTER_REGISTRY.get(backend), 'reset', 'run',
    { lease, app, ...(exec ? { exec } : {}) });
}
