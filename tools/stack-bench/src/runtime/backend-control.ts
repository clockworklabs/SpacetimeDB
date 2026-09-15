import { leaseFromEnv } from './backend-lease.js';
import { leasedDatabaseEnvironment } from '../stacks/stack-adapter-common.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import { controlHostedAppServer, startAttemptDatabaseProcess }
  from '../stacks/hosted-lifecycle.js';
import type { RuntimeControlMode } from '../stacks/stack-adapter-contract.js';
import type { TextCommandExecutor } from './command-executor.js';
import { prepareProcessCrash } from '../stacks/process-crash.js';
import type { CrashTarget, ProcessCrashReceipt } from '../stacks/process-crash.js';
import { attemptDocker, requireAttemptNetwork } from './docker-network.js';
import { Worker } from 'node:worker_threads';
import { answers, waitFor } from '../stacks/lifecycle-readiness.js';
import { attemptDatabaseIdentity } from '../stacks/hosted-database-identity.js';

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

export interface PreparedRuntimeCrash {
  crash(): Promise<ProcessCrashReceipt>;
  close(): Promise<void>;
  recover(signal: AbortSignal): Promise<void>;
  spacetime: { uri: string; mod: string } | null;
}

export async function recoverRuntimeCrash(spec: RuntimeControlSpec, target: CrashTarget): Promise<void> {
  const { lease } = leaseFromEnv(process.env, { backend: spec.backend, active: true });
  requireAttemptNetwork(lease);
  const signal = AbortSignal.timeout(45_000);
  if (target === 'application') return controlAppServer(spec, 'start', { signal });
  if (target !== 'database') throw new Error('unsupported recovery target');
  startAttemptDatabaseProcess(lease);
  await waitFor(async () => {
    if (lease.backend === 'spacetime') return answers(`${lease.resources.serverUri}/v1/ping`, { requireSuccess: true });
    try {
      const id = lease.resources.container!.id;
      if (lease.backend === 'postgres') attemptDocker(['exec', id, 'pg_isready', '-h', '127.0.0.1', '-U', 'postgres']);
      else attemptDocker(['exec', id, 'mongosh', '--quiet', '--username', 'admin', '--password',
        attemptDatabaseIdentity(lease.ownershipToken).adminPassword, '--authenticationDatabase', 'admin',
        '--eval', 'if (!db.hello().isWritablePrimary) quit(1)']);
      return true;
    } catch { return false; }
  }, 30_000, 'crashed database to become ready', signal);
  try {
    await waitFor(() => answers(`http://127.0.0.1:${spec.port}${spec.probe}`, { requireSuccess: true }),
      10_000, 'application to answer after database recovery', signal);
  } catch (cause) {
    throw Object.assign(new Error('application did not answer after database recovery', { cause }),
      { code: 'generated_app_not_restartable' });
  }
}

// Grading only, after the coding process has exited. An app crash kills all
// application-user processes in its owned container, including dev watchers.
export async function prepareRuntimeCrash(spec: RuntimeControlSpec, target: CrashTarget): Promise<PreparedRuntimeCrash> {
  const { lease } = leaseFromEnv(process.env, { backend: spec.backend, active: true });
  const prepared = await prepareProcessCrash(lease, target);
  return {
    ...prepared,
    spacetime: lease.backend === 'spacetime'
      ? { uri: lease.resources.serverUri!, mod: lease.resources.module! } : null,
    async recover(signal) {
      signal.throwIfAborted();
      // Existing lifecycle helpers use synchronous Docker calls. Run them on a
      // separate event loop so they cannot delay this observer's response times.
      const worker = new Worker(`const { parentPort, workerData } = require('node:worker_threads');
        import(workerData.module).then(m => m.recoverRuntimeCrash(workerData.spec, workerData.target))
          .then(() => parentPort.postMessage(null), error => parentPort.postMessage({
            message: error.message, code: error.code, startLog: error.startLog }));`,
      { eval: true, workerData: { module: import.meta.url, spec, target } });
      try {
        await new Promise<void>((resolve, reject) => {
          const abort = () => { void worker.terminate(); reject(signal.reason); };
          signal.addEventListener('abort', abort, { once: true });
          if (signal.aborted) abort();
          worker.once('message', error => {
            signal.removeEventListener('abort', abort);
            if (error) reject(Object.assign(new Error(error.message), error)); else resolve();
          });
          worker.once('error', error => { signal.removeEventListener('abort', abort); reject(error); });
          worker.once('exit', code => {
            signal.removeEventListener('abort', abort);
            reject(new Error(`crash recovery worker exited without a result (${code})`));
          });
        });
      } finally { await worker.terminate(); }
    },
  };
}

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
  { signal = null, exec }: RuntimeControlOptions = {},
): Promise<void> {
  const { lease } = leaseFromEnv(process.env, { backend: spec.backend, active: true });
  const adapter = STACK_ADAPTER_REGISTRY.get(spec.backend);
  const environment = {
    ...leasedDatabaseEnvironment(adapter, {
      database: lease.resources.database,
      networkMode: lease.resources.buildContainer?.networkMode,
      lease,
    }),
    ...adapter.lifecycle.applicationEnvironment?.(lease),
    APP_WARM_START: '1',
    VITE_PORT: String(spec.port),
  };
  await controlHostedAppServer({
    ...spec,
    adapterId: adapter.id,
    lease,
    mode,
    signal,
    handoffWorkspace: true,
    environment,
    ...(exec ? { exec } : {}),
  });
}
