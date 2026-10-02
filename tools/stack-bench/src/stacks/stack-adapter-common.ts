import type { SavedCheckoutRead, StackAdapterOptions, StackLifecycle, StackLifecycleInput,
  StackPortBases } from './stack-adapter-contract.js';
import { controlHostedAppServer } from './hosted-lifecycle.js';
import { attemptDatabaseUrl } from './hosted-database-identity.js';
import type { BackendLease } from '../runtime/backend-lease.js';
import { stackPorts } from './stack-ports.js';

// Every adapter carries the contract's optional members, declared or not.
type StackAdapter<I extends string, T> = { id: I; version: string; lifecycle: StackLifecycle;
  ports: ReturnType<typeof stackPorts> } & T & StackAdapterOptions
  & (T extends { databaseRead: object } ? { databaseRead: SavedCheckoutRead } : unknown);

export function defineStackAdapter<const I extends string, const T extends object>(id: I, lifecycle: StackLifecycle,
  operations: T & StackAdapterOptions & { databaseRead?: SavedCheckoutRead }, { version }: { version: string }) {
  return { id, version, lifecycle, ports: stackPorts(id), ...operations } as StackAdapter<I, T>;
}

interface DatabaseEnvironmentAdapter {
  readonly id: string;
  readonly lifecycle?: Pick<StackLifecycle, 'applicationEnvironment'>;
  readonly ports: { allocations(): StackPortBases };
  readonly agent: {
    connectionUrl(input: { dbPort: number; database: string; hostUrl(url: string): string }): string | null;
  };
}

export function leasedDatabaseEnvironment(adapter: DatabaseEnvironmentAdapter, { database, networkMode, lease }: {
  database: string | null; networkMode: string | null | undefined;
  lease?: BackendLease;
}): Record<string, string> {
  const dbPort = adapter.ports.allocations().db;
  if (!dbPort || !database) return lease ? adapter.lifecycle?.applicationEnvironment?.(lease) ?? {} : {};
  if (lease?.resources.network) return { DATABASE_URL: attemptDatabaseUrl({ backend: adapter.id,
    database, ownershipToken: lease.ownershipToken }) };
  const databaseUrl = adapter.agent.connectionUrl({
    dbPort,
    database,
    hostUrl: url => url.replace(/127\.0\.0\.1|localhost/g,
      networkMode === 'host' ? '127.0.0.1' : 'host.docker.internal'),
  });
  if (!databaseUrl) {
    throw new Error(`stack adapter ${adapter.id} did not provide its leased database URL`);
  }
  return { DATABASE_URL: databaseUrl };
}

export function controlHostedFor(adapterId: 'postgres' | 'mongodb',
  connectionUrl: DatabaseEnvironmentAdapter['agent']['connectionUrl'],
  request: StackLifecycleInput): Promise<void> {
  const { resources } = request.lease;
  const adapter = {
    id: adapterId,
    ports: stackPorts(adapterId),
    agent: { connectionUrl },
  };
  return controlHostedAppServer({
    adapterId: request.adapterId,
    app: request.app,
    port: request.port,
    probe: request.probe,
    mode: request.mode,
    signal: request.signal,
    exec: request.exec,
    lease: request.lease,
    environment: {
      ...leasedDatabaseEnvironment(adapter, { database: resources.database,
        networkMode: resources.buildContainer?.networkMode, lease: request.lease }),
      APP_WARM_START: '1',
      VITE_PORT: String(request.port),
    },
  });
}
