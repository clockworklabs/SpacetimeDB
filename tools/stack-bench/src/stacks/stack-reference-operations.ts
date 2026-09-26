import { dockerHostServiceAddress } from '../runtime/docker-network.js';
import { containerReachableSpacetimeUri } from '../runtime/spacetime-target.js';
import { referenceInstallSteps } from '../references/reference-install.js';
import { POSTGRES_APPLICATION_IDENTITY, attemptDatabaseUrl } from './hosted-database-identity.js';
import { resetMongoDb } from './backends/mongodb-operations.js';
import { resetPostgres } from './backends/postgres-operations.js';
import { requireLeasedDatabase, requireLeasedSpacetime } from './backend-reset-guard.js';
import type { LeasedDatabase } from './backend-reset-guard.js';
import { CODING_CONTAINER_APP_ROOT, CODING_CONTAINER_SPACETIME_CLI }
  from '../runtime/coding-container-policy.js';

import type { StackRunPorts } from './stack-adapter-contract.js';
import type { Track, TrackDefinition } from '../composition/tracks.js';
import type { ReferenceInstallMetadata } from '../references/reference-install.js';
import type { BackendLease } from '../runtime/backend-lease.js';
import { convexApplicationEnvironment } from './backends/convex-operations.js';

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
    logs: () => string) => Promise<void>;
  containerLogs: (container: string, name: string) => string;
  runSync: (purpose: string, file: string, args: readonly string[],
    options?: Record<string, unknown>) => string;
  dbName: (track: Pick<TrackDefinition, 'slug'>, runIndex: number) => string;
  moduleName: (track: TrackDefinition, runIndex: number) => string;
  loadTrack: (name: string) => Track;
}

// The one input every adapter's reference deployment receives. Each deployment
// narrows the lease and the reference.json metadata it needs.
export interface ReferenceDeployInput {
  args: { backend: string; track: string; runIndex: number };
  metadata: unknown;
  lease: BackendLease;
  track: Pick<Track, 'restartProbe' | 'slug'>;
  container: string;
  ports: StackRunPorts;
  buildNetworkMode: string | undefined;
  helpers: ReferenceHelpers;
}

export interface ReferenceBuildStep {
  directory: string;
  command: string;
  args: readonly string[];
}

// The reference.json kind an adapter deploys, the metadata directories that kind
// names beyond `client.directory` (listed ones must also be install directories),
// and the model-free build checks that follow installation.
export interface ReferenceLayout {
  readonly kind: string;
  readonly directories: readonly { readonly field: string; readonly installed: boolean }[];
  buildSteps(metadata: Record<string, unknown>): ReferenceBuildStep[];
}

// The reference.json value at a dotted field path.
export const metadataField = (metadata: Record<string, unknown>, field: string): unknown => field.split('.')
  .reduce<unknown>((value, key) => value !== null && typeof value === 'object'
    ? (value as Record<string, unknown>)[key] : undefined, metadata);
const metadataPath = (metadata: Record<string, unknown>, field: string): string => String(metadataField(metadata, field));

const clientBuild = (metadata: Record<string, unknown>): ReferenceBuildStep =>
  ({ directory: metadataPath(metadata, 'client.directory'), command: 'npm', args: ['run', 'build'] });

export const HOSTED_REFERENCE_LAYOUT: ReferenceLayout = {
  kind: 'node-api',
  directories: [{ field: 'server.directory', installed: true }],
  buildSteps: metadata => [
    { directory: metadataPath(metadata, 'server.directory'), command: 'npm', args: ['exec', 'tsc', '--', '--noEmit'] },
    clientBuild(metadata),
  ],
};

export const SPACETIME_REFERENCE_LAYOUT: ReferenceLayout = {
  kind: 'spacetime',
  directories: [{ field: 'moduleDirectory', installed: true }, { field: 'bindingsDirectory', installed: false }],
  buildSteps: metadata => {
    const module = metadataPath(metadata, 'moduleDirectory');
    return [
      { directory: module, command: CODING_CONTAINER_SPACETIME_CLI,
        args: ['build', '--module-path', `${CODING_CONTAINER_APP_ROOT}/${module}`] },
      { directory: module, command: CODING_CONTAINER_SPACETIME_CLI,
        args: ['generate', '--lang', 'typescript', '--module-path', `${CODING_CONTAINER_APP_ROOT}/${module}`,
          '--out-dir', `${CODING_CONTAINER_APP_ROOT}/${metadataPath(metadata, 'bindingsDirectory')}`, '--yes', '--no-config'] },
      clientBuild(metadata),
    ];
  },
};

// A reference whose `start` script serves the client against the leased platform.
export const startScriptLayout = (kind: string): ReferenceLayout =>
  ({ kind, directories: [], buildSteps: metadata => [clientBuild(metadata)] });

export const CONVEX_REFERENCE_LAYOUT = startScriptLayout('convex');

export interface HostedReferenceMetadata {
  installDirectories: string[];
  server: { directory: string };
  client: { directory: string };
}

export interface SpacetimeReferenceMetadata extends ReferenceInstallMetadata {
  moduleDirectory: string;
  bindingsDirectory: string;
  client: { directory: string };
}

interface StartScriptReferenceMetadata {
  installDirectories: string[];
  client: { directory: string };
}

// The application's start script migrates, seeds and serves the client; the
// platform services are already running in the leased namespace.
export async function deployStartScriptReference({ metadata: rawMetadata, container, ports, helpers }: ReferenceDeployInput,
  platformEnvironment: Record<string, string>, label: string): Promise<void> {
  const metadata = rawMetadata as StartScriptReferenceMetadata;
  const environment: Record<string, string> = { ...platformEnvironment, VITE_PORT: String(ports.vite) };
  for (const directory of metadata.installDirectories) {
    helpers.phase(`installing ${directory}`);
    helpers.docker(container, `${CODING_CONTAINER_APP_ROOT}/${directory}`,
      'npm', ['ci', '--no-audit', '--no-fund']);
  }
  helpers.phase(`deploying ${label} reference application`);
  helpers.startDetached(container, CODING_CONTAINER_APP_ROOT, 'reference-application', environment, { script: 'start' });
  await helpers.waitFor(`http://127.0.0.1:${ports.vite}`, 180_000, `${label} application`,
    () => helpers.containerLogs(container, 'reference-application'));
}

export const deployConvexReference = async (input: ReferenceDeployInput): Promise<void> =>
  deployStartScriptReference(input, convexApplicationEnvironment(input.lease), 'Convex');

interface SpacetimeLease {
  resources: { module: string; serverUri: string;
    buildContainer?: { networkMode?: string } | null };
}

interface HostedReferenceDeployment {
  args: { backend: string; runIndex: number };
  metadata: HostedReferenceMetadata;
  lease: LeasedDatabase;
  track: Pick<Track, 'restartProbe' | 'slug'>;
  container: string;
  ports: Pick<StackRunPorts, 'dbPort' | 'vite'>;
  buildNetworkMode: string | undefined;
  helpers: ReferenceHelpers;
}

type HostedDatabase = { expected: string; containerId: string };

function hostedDeployment(input: ReferenceDeployInput): HostedReferenceDeployment {
  return { ...input, metadata: input.metadata as HostedReferenceMetadata, lease: requireLeasedDatabase(input.lease) };
}

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
  prepare }: {
  databaseUrl: (target: Pick<HostedReferenceDeployment, 'ports' | 'lease' | 'buildNetworkMode'>) => string;
  extraEnv?: Record<string, string>;
  prepare: (database: HostedDatabase, helpers: ReferenceHelpers) => void;
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
    helpers.docker(container, `${CODING_CONTAINER_APP_ROOT}/${directory}`,
      'npm', ['ci', '--no-audit', '--no-fund']);
  }
  helpers.phase('building reference client');
  helpers.docker(container, `${CODING_CONTAINER_APP_ROOT}/${metadata.client.directory}`,
    'npm', ['run', 'build']);
  helpers.phase('starting reference application');
  helpers.startDetached(container, CODING_CONTAINER_APP_ROOT,
    'reference-application', applicationEnv, { script: 'start' });
  await helpers.waitFor(`http://127.0.0.1:${ports.vite}${track.restartProbe}`, 180_000,
    `${args.backend} API`, () => helpers.containerLogs(container, 'reference-application'));
  helpers.phase('reference API ready');
  await helpers.waitFor(`http://127.0.0.1:${ports.vite}`, 180_000,
    `${args.backend} application`, () => helpers.containerLogs(container, 'reference-application'));
  helpers.phase('reference application ready');
}

export function deployPostgresReference(input: ReferenceDeployInput): Promise<void> {
  const { user, password } = POSTGRES_APPLICATION_IDENTITY;
  const hosted = hostedDeployment(input);
  return deployHostedReference(hosted, {
    databaseUrl: ({ ports, lease, buildNetworkMode }) => lease.resources.network
      ? attemptDatabaseUrl({ backend: 'postgres', database: lease.resources.database, ownershipToken: lease.ownershipToken ?? '' })
      : `postgresql://${user}:${password}@${dockerHostServiceAddress(buildNetworkMode)}:${ports.dbPort}/${lease.resources.database}`,
    prepare: (_database, helpers) => resetPostgres({ lease: hosted.lease,
      exec: (command, args, options) => helpers.runSync('resetting PostgreSQL reference database', command, args, { ...options }) }),
  });
}

export function deployMongoDbReference(input: ReferenceDeployInput): Promise<void> {
  const hosted = hostedDeployment(input);
  return deployHostedReference(hosted, {
    databaseUrl: ({ ports, lease, buildNetworkMode }) => lease.resources.network
      ? attemptDatabaseUrl({ backend: 'mongodb', database: lease.resources.database, ownershipToken: lease.ownershipToken ?? '' })
      : `mongodb://${dockerHostServiceAddress(buildNetworkMode)}:${ports.dbPort}/${lease.resources.database}?replicaSet=rs0&directConnection=true`,
    extraEnv: { JWT_SECRET: 'stack-bench-reference-only-secret-2026' },
    prepare: (_database, helpers) => resetMongoDb({ lease: hosted.lease,
      exec: (command, args, options) => helpers.runSync('resetting MongoDB reference database', command, args, { ...options }) }),
  });
}

export async function deploySpacetimeReference({ args, metadata: rawMetadata, lease: backendLease, container, ports,
  buildNetworkMode, helpers }: ReferenceDeployInput): Promise<void> {
  const metadata = rawMetadata as SpacetimeReferenceMetadata;
  const lease: SpacetimeLease = { resources: {
    ...requireLeasedSpacetime(backendLease).resources,
    buildContainer: backendLease.resources.buildContainer,
  } };
  for (const step of referenceInstallSteps(metadata)) {
    helpers.docker(container, `${CODING_CONTAINER_APP_ROOT}/${step.directory}`,
      step.command, step.args);
  }
  const module = helpers.moduleName(helpers.loadTrack(args.track), args.runIndex);
  if (lease.resources.module !== module) throw new Error(`lease module is not ${module}`);
  const serverUri = lease.resources.serverUri;
  if (!serverUri) throw new Error('SpacetimeDB lease records no server URI');
  const hostUri = containerReachableSpacetimeUri(lease, buildNetworkMode ?? null);
  helpers.docker(container, `${CODING_CONTAINER_APP_ROOT}/${metadata.moduleDirectory}`,
    CODING_CONTAINER_SPACETIME_CLI,
    ['publish', module, '--module-path', `${CODING_CONTAINER_APP_ROOT}/${metadata.moduleDirectory}`,
      '-s', hostUri, '-y']);
  helpers.docker(container, `${CODING_CONTAINER_APP_ROOT}/${metadata.moduleDirectory}`,
    CODING_CONTAINER_SPACETIME_CLI,
    ['generate', '--lang', 'typescript',
      '--module-path', `${CODING_CONTAINER_APP_ROOT}/${metadata.moduleDirectory}`,
      '--out-dir', `${CODING_CONTAINER_APP_ROOT}/${metadata.bindingsDirectory}`,
      '--yes', '--no-config']);
  helpers.startDetached(container, `${CODING_CONTAINER_APP_ROOT}/${metadata.client.directory}`,
    'reference-client', {
    VITE_MODULE_NAME: module, VITE_SPACETIMEDB_URI: serverUri,
    VITE_PORT: String(ports.vite),
  }, { networkVisible: true, port: ports.vite });
  await helpers.waitFor(`http://127.0.0.1:${ports.vite}`, 180_000, 'Spacetime client',
    () => helpers.containerLogs(container, 'reference-client'));
}
