import { leaseFromEnv } from './backend-lease.js';
import { executeStackCapability, StackCapabilityUnsupportedError } from '../stacks/stack-adapter-contract.js';
import { leasedDatabaseEnvironment } from '../stacks/stack-adapter-common.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import { controlApplication as controlLeasedApplication }
  from '../stacks/stack-lifecycle-operations.js';
import type { TextCommandExecutor } from './command-executor.js';

export { hostedStopScript } from '../stacks/stack-lifecycle-operations.js';

interface BackendControlSpec extends Record<string, unknown> {
  backend: string;
  app: string;
}

interface DiagnosticsOptions {
  exec?: TextCommandExecutor;
}

interface BackendControlOptions {
  signal?: AbortSignal | null;
}

interface ApplicationControlSpec extends BackendControlSpec {
  port: number;
  probe: string;
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

// Adapter lifecycle controls the stack service used during grading. This path
// always controls the generated application, including the SpacetimeDB client.
export async function controlApplication(
  spec: unknown,
  mode = 'restart',
  { signal = null }: BackendControlOptions = {},
): Promise<void> {
  if (!spec || typeof spec !== 'object' || Array.isArray(spec)
    || !('backend' in spec) || typeof spec.backend !== 'string' || !spec.backend
    || !('app' in spec) || typeof spec.app !== 'string' || !spec.app
    || !('port' in spec) || !Number.isInteger(Number(spec.port))
    || Number(spec.port) <= 0 || Number(spec.port) > 65535
    || !('probe' in spec) || typeof spec.probe !== 'string') {
    throw new Error('application control spec is incomplete');
  }
  if (!['restart', 'stop', 'start'].includes(mode)) {
    throw new Error(`unknown application control mode ${mode}`);
  }
  const controlSpec: ApplicationControlSpec = {
    ...spec,
    backend: spec.backend,
    app: spec.app,
    port: Number(spec.port),
    probe: spec.probe,
  };
  const { lease } = leaseFromEnv(process.env, { backend: controlSpec.backend, active: true });
  const adapter = STACK_ADAPTER_REGISTRY.get(controlSpec.backend);
  const environment = {
    ...leasedDatabaseEnvironment(adapter, {
      database: lease.resources.database,
      networkMode: lease.resources.buildContainer?.networkMode,
    }),
    VITE_PORT: String(controlSpec.port),
  };
  await controlLeasedApplication({
    ...controlSpec,
    adapterId: adapter.id,
    lease,
    mode,
    signal,
    environment,
  });
}
