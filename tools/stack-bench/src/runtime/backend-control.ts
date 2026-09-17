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
import type { BackendLease } from './backend-lease.js';
import { assertLeasedContainer, requireLeasedDatabase } from '../stacks/backend-reset-guard.js';
import { execFileSync } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';

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
  recover(signal: AbortSignal): Promise<DatabaseDrainReceipt | null>;
  spacetime: { uri: string; mod: string } | null;
}

export interface DatabaseDrainReceipt {
  backend: string;
  database: string;
  startedAtMs: number;
  completedAtMs: number;
  settled: boolean;
  samples: Array<{ atMs: number; pending: number }>;
}

// The app stays stopped while this read-only probe waits for its old database
// connections and transactions. A client SIGKILL cannot retract a sent COMMIT.
export async function drainApplicationDatabase(lease: BackendLease, deadlineMs: number,
  signal: AbortSignal, exec: TextCommandExecutor = execFileSync): Promise<DatabaseDrainReceipt> {
  const database = requireLeasedDatabase(lease);
  const container = assertLeasedContainer(database.resources.container, exec, 5000, 'crash database drain');
  const receipt: DatabaseDrainReceipt = { backend: lease.backend, database: database.resources.database,
    startedAtMs: Date.now(), completedAtMs: Date.now(), settled: false, samples: [] };
  const identity = attemptDatabaseIdentity(lease.ownershipToken);
  let command: string[];
  if (lease.backend === 'postgres') {
    command = ['psql', '-U', identity.user, '-d', database.resources.database, '-v', 'ON_ERROR_STOP=1', '-At', '-c',
      "SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND backend_type='client backend' AND pid<>pg_backend_pid()"];
  } else if (lease.backend === 'mongodb') {
    // A single pooled connection lets us exclude precisely this observer, not
    // app transactions that no longer have a live connection.
    command = ['mongosh', `mongodb://127.0.0.1/${encodeURIComponent(database.resources.database)}?maxPoolSize=1&directConnection=true`,
      '--username', identity.user, '--password', identity.password, '--authenticationDatabase', database.resources.database,
      '--quiet', '--eval', `
        const self = db.hello().connectionId;
        if (!Number.isSafeInteger(Number(self)) || Number(self) <= 0) throw new Error('missing observer connection');
        const work = db.getSiblingDB('admin').aggregate([
          {$currentOp:{allUsers:false,idleConnections:true,idleSessions:true}},
          {$match:{connectionId:{$ne:self}}}, {$count:'pending'}
        ]).toArray();
        print(work.length ? work[0].pending : 0);`];
  } else throw new Error('database drain requires a separate hosted application');
  try {
    while (Date.now() < deadlineMs) {
      signal.throwIfAborted();
      let output: string;
      try {
        output = exec('docker', ['exec', container, ...command],
          { encoding: 'utf8', stdio: 'pipe', timeout: Math.max(1, Math.min(5000, deadlineMs - Date.now())) }).trim();
      } catch (error) {
        if (error && typeof error === 'object' && 'code' in error && error.code === 'ETIMEDOUT'
          && Date.now() >= deadlineMs) break;
        throw new Error('could not observe pending database work');
      }
      if (!/^\d+$/.test(output) || !Number.isSafeInteger(Number(output))) throw new Error('invalid database work count');
      receipt.samples.push({ atMs: Date.now(), pending: Number(output) });
      if (output === '0' && Date.now() <= deadlineMs) { receipt.settled = true; break; }
      await delay(Math.min(500, Math.max(0, deadlineMs - Date.now())), undefined, { signal });
    }
    signal.throwIfAborted();
  } catch (error) {
    receipt.completedAtMs = Date.now();
    throw Object.assign(error instanceof Error ? error : new Error('database drain interrupted'), { databaseDrain: receipt });
  }
  receipt.completedAtMs = Date.now();
  return receipt;
}

export async function recoverRuntimeCrash(spec: RuntimeControlSpec, target: CrashTarget): Promise<DatabaseDrainReceipt | null> {
  const { lease } = leaseFromEnv(process.env, { backend: spec.backend, active: true });
  requireAttemptNetwork(lease);
  const signal = AbortSignal.timeout(target === 'application' ? 110_000 : 45_000);
  if (target === 'application') {
    let drain: DatabaseDrainReceipt | undefined, failure: Error | undefined;
    // MongoDB's default 60-second transaction lifetime is enforced by a
    // 30-second sweep. Allow both intervals before declaring work unsettled.
    try { drain = await drainApplicationDatabase(lease, Date.now() + 100_000, signal); }
    catch (error) { failure = error instanceof Error ? error : new Error('database drain failed'); }
    if (signal.aborted) throw failure ?? Object.assign(new Error('crash recovery interrupted'), { databaseDrain: drain ?? null });
    // Restore service even after an observer error. Otherwise later checks
    // would test a service that the harness itself left stopped.
    try { await controlAppServer(spec, 'start', { signal }); }
    catch (error) {
      if (failure) {
        failure.message += `; application restart also failed: ${error instanceof Error ? error.message : 'unknown restart error'}`;
        if (error && typeof error === 'object' && 'startLog' in error) Object.assign(failure, { startLog: error.startLog });
      }
      else failure = Object.assign(error instanceof Error ? error : new Error('application restart failed'), { databaseDrain: drain });
    }
    if (failure) throw failure;
    return drain!;
  }
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
  return null;
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
          .then(drain => parentPort.postMessage({ drain }), error => parentPort.postMessage({ error: {
            message: error.message, code: error.code, startLog: error.startLog, databaseDrain: error.databaseDrain ?? null } }));`,
      { eval: true, workerData: { module: import.meta.url, spec, target } });
      try {
        return await new Promise<DatabaseDrainReceipt | null>((resolve, reject) => {
          const abort = () => { void worker.terminate(); reject(signal.reason); };
          signal.addEventListener('abort', abort, { once: true });
          if (signal.aborted) abort();
          worker.once('message', result => {
            signal.removeEventListener('abort', abort);
            if (result.error) reject(Object.assign(new Error(result.error.message), result.error)); else resolve(result.drain);
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
