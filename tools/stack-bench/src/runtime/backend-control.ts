import { leaseFromEnv } from './backend-lease.js';
import { leasedDatabaseEnvironment } from '../stacks/stack-adapter-common.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import { controlHostedAppServer }
  from '../stacks/hosted-lifecycle.js';
import type { RuntimeControlMode } from '../stacks/stack-adapter-contract.js';
import type { TextCommandExecutor } from './command-executor.js';

export { hostedStopScript } from '../stacks/hosted-lifecycle.js';
export type { RuntimeControlMode } from '../stacks/stack-adapter-contract.js';

export interface RuntimeControlSpec {
  backend: string;
  app: string;
  port: number;
  probe: string;
}

interface DiagnosticsOptions { exec?: TextCommandExecutor }
interface RuntimeControlOptions { signal?: AbortSignal | null; exec?: TextCommandExecutor }

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
  if (!('diagnostics' in adapter)) {
    return { captured: false, reason: 'backend has no hosted app restart log' };
  }
  return adapter.diagnostics.capture(
    { lease, output, ...(exec ? { exec } : {}) });
}

export async function controlBackendRuntime(
  spec: RuntimeControlSpec,
  mode: RuntimeControlMode = 'restart',
  { signal = null, exec }: RuntimeControlOptions = {},
): Promise<void> {
  const { lease } = leaseFromEnv(process.env, { backend: spec.backend, active: true });
  const adapter = STACK_ADAPTER_REGISTRY.get(spec.backend);
  if (!adapter.lifecycle.control) {
    throw new Error(`stack adapter ${adapter.id} does not support runtime control`);
  }
  await adapter.lifecycle.control({ ...spec, adapterId: adapter.id, lease, mode, signal,
    ...(exec ? { exec } : {}) });
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
    ...adapter.lifecycle.applicationEnvironment?.(lease),
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
