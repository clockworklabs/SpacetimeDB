import type { execFileSync } from 'node:child_process';

import { leaseFromEnv } from './backend-lease.js';
import { executeStackCapability, StackCapabilityUnsupportedError } from '../stacks/stack-adapter-contract.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';

export { hostedStopScript } from '../stacks/stack-lifecycle-operations.mjs';

interface BackendControlSpec extends Record<string, unknown> {
  backend: string;
  app: string;
}

interface DiagnosticsOptions {
  exec?: typeof execFileSync;
}

interface BackendControlOptions {
  signal?: AbortSignal | null;
}

export function captureBackendDiagnostics(
  output: string,
  { exec }: DiagnosticsOptions = {},
): unknown {
  const { lease } = leaseFromEnv(process.env, { active: true });
  const adapter = STACK_ADAPTER_REGISTRY.get(lease.backend);
  try {
    return executeStackCapability(adapter, 'diagnostics', 'capture',
      { adapterId: adapter.id, lease, output, ...(exec ? { exec } : {}) });
  } catch (error) {
    if (error instanceof StackCapabilityUnsupportedError) {
      return { captured: false, reason: 'backend has no hosted app restart log' };
    }
    throw error;
  }
}

export async function controlBackend(
  spec: unknown,
  mode = 'restart',
  { signal = null }: BackendControlOptions = {},
): Promise<unknown> {
  if (!spec || typeof spec !== 'object' || Array.isArray(spec)
    || !('backend' in spec) || typeof spec.backend !== 'string' || !spec.backend
    || !('app' in spec) || typeof spec.app !== 'string' || !spec.app) {
    throw new Error('backend control spec is incomplete');
  }
  const controlSpec: BackendControlSpec = { ...spec, backend: spec.backend, app: spec.app };
  if (!['restart', 'stop', 'start'].includes(mode)) {
    throw new Error(`unknown backend control mode ${mode}`);
  }
  const { lease } = leaseFromEnv(process.env, { backend: controlSpec.backend, active: true });
  const adapter = STACK_ADAPTER_REGISTRY.get(controlSpec.backend);
  return executeStackCapability(adapter, 'lifecycle', 'control',
    { ...controlSpec, adapterId: adapter.id, lease, mode, signal });
}
