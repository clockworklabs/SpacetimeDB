import { leaseFromEnv } from './backend-lease.js';
import { executeStackCapability, StackCapabilityUnsupportedError } from '../stacks/stack-adapter-contract.js';
import { leasedDatabaseEnvironment } from '../stacks/stack-adapter-common.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import { controlHostedAppServer }
  from '../stacks/stack-lifecycle-operations.js';
import type { TextCommandExecutor } from './command-executor.js';

export { hostedStopScript } from '../stacks/stack-lifecycle-operations.js';

export interface RuntimeControlSpec {
  backend: string;
  app: string;
  port: number;
  probe: string;
}

export type RuntimeControlMode = 'restart' | 'stop' | 'start';

interface DiagnosticsOptions { exec?: TextCommandExecutor }
interface RuntimeControlOptions { signal?: AbortSignal | null }

export function parseRuntimeControlSpec(value: unknown): RuntimeControlSpec {
  if (!value || typeof value !== 'object' || Array.isArray(value)
    || !('backend' in value) || typeof value.backend !== 'string' || !value.backend
    || !('app' in value) || typeof value.app !== 'string' || !value.app
    || !('port' in value) || typeof value.port !== 'number' || !Number.isInteger(value.port)
    || value.port <= 0 || value.port > 65535
    || !('probe' in value) || typeof value.probe !== 'string') {
    throw new Error('runtime control spec is incomplete');
  }
  return { backend: value.backend, app: value.app, port: value.port, probe: value.probe };
}

export function captureApplicationDiagnostics(
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

export async function controlBackendRuntime(
  spec: RuntimeControlSpec,
  mode: RuntimeControlMode = 'restart',
  { signal = null }: RuntimeControlOptions = {},
): Promise<void> {
  const { lease } = leaseFromEnv(process.env, { backend: spec.backend, active: true });
  const adapter = STACK_ADAPTER_REGISTRY.get(spec.backend);
  await executeStackCapability(adapter, 'lifecycle', 'control',
    { ...spec, adapterId: adapter.id, lease, mode, signal });
}

// Always control the generated app server, not the stack's backend runtime.
export async function controlAppServer(
  spec: RuntimeControlSpec,
  mode: RuntimeControlMode = 'restart',
  { signal = null }: RuntimeControlOptions = {},
): Promise<void> {
  const { lease } = leaseFromEnv(process.env, { backend: spec.backend, active: true });
  const adapter = STACK_ADAPTER_REGISTRY.get(spec.backend);
  const environment = {
    ...leasedDatabaseEnvironment(adapter, {
      database: lease.resources.database,
      networkMode: lease.resources.buildContainer?.networkMode,
    }),
    VITE_PORT: String(spec.port),
  };
  await controlHostedAppServer({
    ...spec,
    adapterId: adapter.id,
    lease,
    mode,
    signal,
    environment,
  });
}
