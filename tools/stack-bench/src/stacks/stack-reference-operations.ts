import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { dockerHostServiceAddress } from '../runtime/docker-network.js';
import { containerReachableSpacetimeUri } from '../runtime/spacetime-target.js';
import { referenceInstallSteps } from '../references/reference-install.js';
import { POSTGRES_APPLICATION_IDENTITY } from './hosted-database-identity.js';

import type { BackendLease } from '../runtime/backend-lease.js';
import type { StackRunPorts } from './stack-adapter-contract.js';
import type { Track, TrackDefinition } from '../composition/tracks.js';
import type { ReferenceInstallMetadata } from '../references/reference-install.js';

// What deploying a reference application needs from its caller. The reference
// runner owns the container and the waiting; these operations own the shape of
// each backend's deployment.
export interface ReferenceHelpers {
  phase: (message: string) => void;
  docker: (container: string, cwd: string, command: string,
    args: readonly string[], env?: Record<string, string>) => unknown;
  startDetached: (container: string, cwd: string, name: string,
    env: Record<string, string>,
    options?: { script?: string; networkVisible?: boolean; port?: number }) => unknown;
  waitFor: (url: string, timeoutMs: number, description: string,
    logs: () => unknown) => Promise<unknown>;
  containerLogs: (container: string, name: string) => unknown;
  runSync: (purpose: string, file: string, args: readonly string[],
    options?: Record<string, unknown>) => string;
  dbName: (track: TrackDefinition, runIndex: number) => string;
  moduleName: (track: TrackDefinition, runIndex: number) => string;
  loadTrack: (name: string) => Track;
}

export interface ReferenceMetadata extends ReferenceInstallMetadata {
  server: { directory: string };
  client: { directory: string };
  moduleDirectory: string;
  bindingsDirectory: string;
}

export interface ReferenceDeployment {
  args: { app: string; backend: string; track: string; runIndex: number };
  metadata: ReferenceMetadata;
  lease: BackendLease;
  track: Track;
  container: string;
  ports: StackRunPorts;
  buildNetworkMode: string | undefined;
  helpers: ReferenceHelpers;
}

type HostedDatabase = { expected: string; service: { id: string; name: string } };

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object';


function validateHostedDatabase({ args, lease, track, helpers }:
  ReferenceDeployment): HostedDatabase {
  const expected = helpers.dbName(track, args.runIndex);
  if (lease.resources.database !== expected) throw new Error(`lease database is not ${expected}`);
  const service = lease.resources.container;
  if (!service) throw new Error('lease has no database container');
  const actual = helpers.runSync('inspecting leased database container', 'docker',
    ['inspect', '--format', '{{.Id}}', service.name],
    { encoding: 'utf8', stdio: 'pipe' }).trim();
  if (actual !== service.id) throw new Error(`${service.name} no longer matches its lease`);
  return { expected, service };
}

async function deployHostedReference(input: ReferenceDeployment, { databaseUrl, extraEnv = {},
  prepare, pushSchema }: {
  databaseUrl: (target: Pick<ReferenceDeployment, 'ports' | 'lease' | 'buildNetworkMode'>) => string;
  extraEnv?: Record<string, string>;
  prepare: (database: HostedDatabase, helpers: ReferenceHelpers) => void;
  pushSchema?: (stage: { container: string; metadata: ReferenceMetadata;
    serverEnv: Record<string, string>; helpers: ReferenceHelpers }) => void;
}): Promise<void> {
  const { args, metadata, lease, track, container, ports, buildNetworkMode, helpers } = input;
  helpers.phase('preparing database');
  const database = validateHostedDatabase(input);
  prepare(database, helpers);
  const serverEnv = {
    DATABASE_URL: databaseUrl({ ports, lease, buildNetworkMode }),
    PORT: String(ports.express),
    ...extraEnv,
  };
  for (const directory of metadata.installDirectories) {
    helpers.phase(`installing ${directory}`);
    helpers.docker(container, `/app/${directory}`, 'npm', ['ci', '--no-audit', '--no-fund']);
  }
  if (pushSchema) pushSchema({ container, metadata, serverEnv, helpers });
  helpers.phase('starting reference server and client');
  const serverPackage: unknown = JSON.parse(readFileSync(join(args.app,
    metadata.server.directory, 'package.json'), 'utf8'));
  const scripts = record(serverPackage) ? serverPackage.scripts : null;
  const serverScript = record(scripts) && scripts.start ? 'start' : 'dev';
  helpers.startDetached(container, `/app/${metadata.server.directory}`, 'reference-server', serverEnv,
    { script: serverScript });
  helpers.startDetached(container, `/app/${metadata.client.directory}`, 'reference-client', {
    API_PORT: String(ports.express), VITE_PORT: String(ports.vite),
  }, { networkVisible: true, port: ports.vite });
  await helpers.waitFor(`http://127.0.0.1:${ports.express}${track.restartProbe}`, 180_000,
    `${args.backend} API`, () => helpers.containerLogs(container, 'reference-server'));
  helpers.phase('reference server ready');
  await helpers.waitFor(`http://127.0.0.1:${ports.vite}`, 180_000,
    `${args.backend} client`, () => helpers.containerLogs(container, 'reference-client'));
  helpers.phase('reference client ready');
}

export function deployPostgresReference(input: ReferenceDeployment): Promise<void> {
  const { user, password } = POSTGRES_APPLICATION_IDENTITY;
  return deployHostedReference(input, {
    databaseUrl: ({ ports, lease, buildNetworkMode }) =>
      `postgresql://${user}:${password}@${dockerHostServiceAddress(buildNetworkMode)}:${ports.dbPort}/${lease.resources.database}`,
    prepare: ({ expected, service }, helpers) => {
      try {
        helpers.runSync('creating PostgreSQL reference database', 'docker',
          ['exec', service.name, 'psql', '-U', user, '-d', 'postgres',
            '-c', `CREATE DATABASE ${expected} OWNER ${user};`], { stdio: 'pipe' });
      } catch { /* an existing run-index database is expected */ }
      helpers.runSync('resetting PostgreSQL reference schema', 'docker',
        ['exec', service.name, 'psql', '-U', user, '-d', expected,
          '-c', `DROP SCHEMA public CASCADE; CREATE SCHEMA public; GRANT ALL ON SCHEMA public TO ${user};`],
        { stdio: 'pipe' });
    },
    pushSchema: ({ container, metadata, serverEnv, helpers }) => {
      helpers.phase('pushing PostgreSQL schema');
      helpers.docker(container, `/app/${metadata.server.directory}`,
        './node_modules/.bin/drizzle-kit', ['push', '--force'], serverEnv);
    },
  });
}

export function deployMongoDbReference(input: ReferenceDeployment): Promise<void> {
  return deployHostedReference(input, {
    databaseUrl: ({ ports, lease, buildNetworkMode }) =>
      `mongodb://${dockerHostServiceAddress(buildNetworkMode)}:${ports.dbPort}/${lease.resources.database}`,
    extraEnv: { JWT_SECRET: 'stack-bench-reference-only-secret-2026' },
    prepare: ({ expected, service }, helpers) => {
      helpers.runSync('resetting MongoDB reference database', 'docker',
        ['exec', service.name, 'mongosh', expected, '--quiet', '--eval', 'db.dropDatabase()'],
        { stdio: 'pipe' });
    },
  });
}

export async function deploySpacetimeReference({ args, metadata, lease, container, ports,
  buildNetworkMode, helpers }: ReferenceDeployment): Promise<void> {
  for (const step of referenceInstallSteps(metadata)) {
    helpers.docker(container, `/app/${step.directory}`, step.command, step.args);
  }
  const module = helpers.moduleName(helpers.loadTrack(args.track), args.runIndex);
  if (lease.resources.module !== module) throw new Error(`lease module is not ${module}`);
  const serverUri = lease.resources.serverUri;
  if (!serverUri) throw new Error('SpacetimeDB lease records no server URI');
  const hostUri = containerReachableSpacetimeUri({ ...lease,
    resources: { ...lease.resources, serverUri } }, buildNetworkMode ?? null);
  helpers.docker(container, `/app/${metadata.moduleDirectory}`, '/deps/spacetimedb-cli',
    ['publish', module, '--module-path', `/app/${metadata.moduleDirectory}`, '-s', hostUri, '-y']);
  helpers.docker(container, `/app/${metadata.moduleDirectory}`, '/deps/spacetimedb-cli',
    ['generate', '--lang', 'typescript', '--module-path', `/app/${metadata.moduleDirectory}`,
      '--out-dir', `/app/${metadata.bindingsDirectory}`, '--yes', '--no-config']);
  helpers.startDetached(container, `/app/${metadata.client.directory}`, 'reference-client', {
    VITE_MODULE_NAME: module, VITE_SPACETIMEDB_URI: serverUri,
    VITE_PORT: String(ports.vite),
  }, { networkVisible: true, port: ports.vite });
  await helpers.waitFor(`http://127.0.0.1:${ports.vite}`, 180_000, 'Spacetime client',
    () => helpers.containerLogs(container, 'reference-client'));
}
