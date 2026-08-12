import { leaseFromEnv } from './backend-lease.mjs';
import { executeStackCapability, StackCapabilityUnsupportedError } from './stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.mjs';

export { hostedStopScript } from './stack-lifecycle-operations.mjs';

export function captureBackendDiagnostics(output, { exec } = {}) {
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

export async function controlBackend(spec, mode = 'restart', { signal = null } = {}) {
  if (!spec || !spec.backend || !spec.app) throw new Error('backend control spec is incomplete');
  if (!['restart', 'stop', 'start'].includes(mode)) throw new Error(`unknown backend control mode ${mode}`);
  const { lease } = leaseFromEnv(process.env, { backend: spec.backend, active: true });
  const adapter = STACK_ADAPTER_REGISTRY.get(spec.backend);
  return executeStackCapability(adapter, 'lifecycle', 'control',
    { ...spec, adapterId: adapter.id, lease, mode, signal });
}
