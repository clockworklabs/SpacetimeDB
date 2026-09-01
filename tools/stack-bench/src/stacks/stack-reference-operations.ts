import { dockerHostServiceAddress } from '../runtime/docker-network.js';
import { containerReachableSpacetimeUri } from '../runtime/spacetime-target.js';
import { referenceInstallSteps } from '../references/reference-install.js';
import { POSTGRES_APPLICATION_IDENTITY } from './hosted-database-identity.js';

import type { StackRunPorts } from './stack-adapter-contract.js';
import type { Track, TrackDefinition } from '../composition/tracks.js';
import type { ReferenceInstallMetadata } from '../references/reference-install.js';

// What deploying a reference application needs from its caller. The reference
// runner owns the container and the waiting; these operations own the shape of
// each backend's deployment.
export interface HostedReferenceHelpers {
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
  dbName: (track: Pick<TrackDefinition, 'slug'>, runIndex: number) => string;
}

export interface SpacetimeReferenceHelpers {
  docker: (container: string, cwd: string, command: string,
    args: readonly string[], env?: Record<string, string>) => unknown;
  startDetached: (container: string, cwd: string, name: string,
    env: Record<string, string>,
    options?: { script?: string; networkVisible?: boolean; port?: number }) => unknown;
  waitFor: (url: string, timeoutMs: number, description: string,
    logs: () => unknown) => Promise<unknown>;
  containerLogs: (container: string, name: string) => unknown;
  moduleName: (track: TrackDefinition, runIndex: number) => string;
  loadTrack: (name: string) => Track;
}

interface HostedReferenceMetadata {
  installDirectories: string[];
  server: { directory: string };
  client: { directory: string };
}

interface SpacetimeReferenceMetadata extends ReferenceInstallMetadata {
  moduleDirectory: string;
  bindingsDirectory: string;
  client: { directory: string };
}

interface HostedLease {
  resources: { database: string; container: { name: string; id: string } };
}

interface SpacetimeLease {
  resources: { module: string; serverUri: string;
    buildContainer?: { networkMode?: 'host' | 'bridge' } | null };
}

interface HostedReferenceDeployment {
  args: { backend: string; runIndex: number };
  metadata: HostedReferenceMetadata;
  lease: HostedLease;
  track: Pick<Track, 'restartProbe' | 'slug'>;
  container: string;
  ports: Pick<StackRunPorts, 'dbPort' | 'vite'>;
  buildNetworkMode: string | undefined;
  helpers: HostedReferenceHelpers;
}

interface SpacetimeReferenceDeployment {
  args: { track: string; runIndex: number };
  metadata: SpacetimeReferenceMetadata;
  lease: SpacetimeLease;
  container: string;
  ports: Pick<StackRunPorts, 'vite'>;
  buildNetworkMode: string | undefined;
  helpers: SpacetimeReferenceHelpers;
}

type HostedDatabase = { expected: string; containerId: string };


function validateHostedDatabase({ args, lease, track, helpers }:
  HostedReferenceDeployment): HostedDatabase {
  const expected = helpers.dbName(track, args.runIndex);
  if (lease.resources.database !== expected) throw new Error(`lease database is not ${expected}`);
  const service = lease.resources.container;
  if (!service) throw new Error('lease has no database container');
  const actual = helpers.runSync('inspecting leased database container', 'docker',
    ['inspect', '--format', '{{.Id}}', service.name],
    { encoding: 'utf8', stdio: 'pipe' }).trim();
  if (actual !== service.id) throw new Error(`${service.name} no longer matches its lease`);
  return { expected, containerId: actual };
}

async function deployHostedReference(input: HostedReferenceDeployment, { databaseUrl, extraEnv = {},
  prepare, pushSchema }: {
  databaseUrl: (target: Pick<HostedReferenceDeployment, 'ports' | 'lease' | 'buildNetworkMode'>) => string;
  extraEnv?: Record<string, string>;
  prepare: (database: HostedDatabase, helpers: HostedReferenceHelpers) => void;
  pushSchema?: (stage: { container: string; metadata: HostedReferenceMetadata;
    applicationEnv: Record<string, string>; helpers: HostedReferenceHelpers }) => void;
}): Promise<void> {
  const { args, metadata, lease, track, container, ports, buildNetworkMode, helpers } = input;
  helpers.phase('preparing database');
  const database = validateHostedDatabase(input);
  prepare(database, helpers);
  const applicationEnv = {
    DATABASE_URL: databaseUrl({ ports, lease, buildNetworkMode }),
    PORT: String(ports.vite),
    ...extraEnv,
  };
  for (const directory of metadata.installDirectories) {
    helpers.phase(`installing ${directory}`);
    helpers.docker(container, `/app/${directory}`, 'npm', ['ci', '--no-audit', '--no-fund']);
  }
  if (pushSchema) pushSchema({ container, metadata, applicationEnv, helpers });
  helpers.phase('building reference client');
  helpers.docker(container, `/app/${metadata.client.directory}`, 'npm', ['run', 'build']);
  helpers.phase('starting reference application');
  helpers.startDetached(container, '/app', 'reference-application', applicationEnv, { script: 'start' });
  await helpers.waitFor(`http://127.0.0.1:${ports.vite}${track.restartProbe}`, 180_000,
    `${args.backend} API`, () => helpers.containerLogs(container, 'reference-application'));
  helpers.phase('reference API ready');
  await helpers.waitFor(`http://127.0.0.1:${ports.vite}`, 180_000,
    `${args.backend} application`, () => helpers.containerLogs(container, 'reference-application'));
  helpers.phase('reference application ready');
}

export function deployPostgresReference(input: HostedReferenceDeployment): Promise<void> {
  const { user, password } = POSTGRES_APPLICATION_IDENTITY;
  return deployHostedReference(input, {
    databaseUrl: ({ ports, lease, buildNetworkMode }) =>
      `postgresql://${user}:${password}@${dockerHostServiceAddress(buildNetworkMode)}:${ports.dbPort}/${lease.resources.database}`,
    prepare: ({ expected, containerId }, helpers) => {
      try {
        helpers.runSync('creating PostgreSQL reference database', 'docker',
          ['exec', containerId, 'psql', '-U', user, '-d', 'postgres',
            '-c', `CREATE DATABASE ${expected} OWNER ${user};`], { stdio: 'pipe' });
      } catch { /* an existing run-index database is expected */ }
      helpers.runSync('resetting PostgreSQL reference schema', 'docker',
        ['exec', containerId, 'psql', '-U', user, '-d', expected,
          '-c', `DROP SCHEMA public CASCADE; CREATE SCHEMA public; GRANT ALL ON SCHEMA public TO ${user};`],
        { stdio: 'pipe' });
    },
    pushSchema: ({ container, metadata, applicationEnv, helpers }) => {
      helpers.phase('pushing PostgreSQL schema');
      helpers.docker(container, `/app/${metadata.server.directory}`,
        './node_modules/.bin/drizzle-kit', ['push', '--force'], applicationEnv);
    },
  });
}

export function deployMongoDbReference(input: HostedReferenceDeployment): Promise<void> {
  return deployHostedReference(input, {
    databaseUrl: ({ ports, lease, buildNetworkMode }) =>
      `mongodb://${dockerHostServiceAddress(buildNetworkMode)}:${ports.dbPort}/${lease.resources.database}`,
    extraEnv: { JWT_SECRET: 'stack-bench-reference-only-secret-2026' },
    prepare: ({ expected, containerId }, helpers) => {
      helpers.runSync('resetting MongoDB reference database', 'docker',
        ['exec', containerId, 'mongosh', expected, '--quiet', '--eval', 'db.dropDatabase()'],
        { stdio: 'pipe' });
    },
  });
}

export async function deploySpacetimeReference({ args, metadata, lease, container, ports,
  buildNetworkMode, helpers }: SpacetimeReferenceDeployment): Promise<void> {
  for (const step of referenceInstallSteps(metadata)) {
    helpers.docker(container, `/app/${step.directory}`, step.command, step.args);
  }
  const module = helpers.moduleName(helpers.loadTrack(args.track), args.runIndex);
  if (lease.resources.module !== module) throw new Error(`lease module is not ${module}`);
  const serverUri = lease.resources.serverUri;
  if (!serverUri) throw new Error('SpacetimeDB lease records no server URI');
  const hostUri = containerReachableSpacetimeUri(lease, buildNetworkMode ?? null);
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
