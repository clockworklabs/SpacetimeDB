import type { StackLifecycle, StackLifecycleInput,
  StackPortBases } from './stack-adapter-contract.js';
import { controlHostedAppServer } from './hosted-lifecycle.js';

type AdapterId = keyof typeof PORT_BASES;

const PORT_BASES = Object.freeze({
  spacetime: Object.freeze({ vite: 6173 }),
  postgres: Object.freeze({ vite: 6273, express: 6001, db: 6532 }),
  mongodb: Object.freeze({ vite: 6373, express: 6101, db: 6537 }),
  stub: Object.freeze({ vite: 7000 }),
});

function portsFor(adapterId: AdapterId) {
  const allocations = PORT_BASES[adapterId];
  return {
    allocations: (): StackPortBases => ({ ...allocations }),
    forRun: ({ trackOffset, runIndex }: { trackOffset: number; runIndex: number }) => {
      if (!Number.isInteger(trackOffset) || trackOffset < 0
        || !Number.isInteger(runIndex) || runIndex < 0) {
        throw new Error(`${adapterId} ports require non-negative integer trackOffset and runIndex`);
      }
      const offset = trackOffset + runIndex;
      return {
        vite: allocations.vite + offset,
        express: 'express' in allocations ? allocations.express + offset : null,
        dbPort: 'db' in allocations ? allocations.db : null,
      };
    },
  };
}

export function defineStackAdapter<const I extends AdapterId, const T extends object>(id: I, lifecycle: StackLifecycle,
  operations: T, { version }: { version: string }) {
  return {
    id,
    version,
    lifecycle,
    ports: portsFor(id),
    ...operations,
  };
}

interface DatabaseEnvironmentAdapter {
  readonly id: string;
  readonly ports: { allocations(): StackPortBases };
  readonly agent: {
    connectionUrl(input: { dbPort: number; database: string; hostUrl(url: string): string }): string | null;
  };
}

export function leasedDatabaseEnvironment(adapter: DatabaseEnvironmentAdapter, { database, networkMode }: {
  database: string | null; networkMode: string | null | undefined;
}): Record<string, string> {
  const dbPort = adapter.ports.allocations().db;
  if (!dbPort || !database) return {};
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
    ports: portsFor(adapterId),
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
        networkMode: resources.buildContainer?.networkMode }),
      VITE_PORT: String(request.port),
    },
  });
}
