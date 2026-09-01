import { join, resolve } from 'node:path';
import { CODING_CONTAINER_AGENT } from '../runtime/coding-container-policy.js';
import { databaseContainerName } from './database-containers.js';
import { POSTGRES_APPLICATION_IDENTITY } from './hosted-database-identity.js';

type HostUrl = (url: string) => string;

interface StackSetupMetadata {
  spacetime: string | null;
  spacetimeBindings: string | null;
  database: string | null;
}

export interface BuildContainerPlan {
  networkNamespace: string | null;
  requiredPaths: string[];
  ensureDirectories: string[];
  mounts: Array<{ kind: string; source: string; target: string; readOnly: boolean }>;
  init: string;
  readyFile: string | null;
  readyDescription: string | null;
}

export function postgresConnectionUrl({ dbPort, database, hostUrl }:
  { dbPort: number; database: string; hostUrl: HostUrl }): string {
  const { user, password } = POSTGRES_APPLICATION_IDENTITY;
  return hostUrl(`postgresql://${user}:${password}@localhost:${dbPort}/${database}`);
}

export function mongoDbConnectionUrl({ dbPort, database, hostUrl }:
  { dbPort: number; database: string; hostUrl: HostUrl }): string {
  return hostUrl(`mongodb://localhost:${dbPort}/${database}`);
}

export function noConnectionUrl(): null {
  return null;
}

export function spacetimeSetupMetadata({ imageId, localPackage, helpers }: {
  imageId: string;
  localPackage: string;
  helpers: {
    linuxSpacetimeVersion: (imageId: string) => string;
    bindingsIdentity: (localPackage: string) => string;
  };
}): StackSetupMetadata {
  return {
    spacetime: helpers.linuxSpacetimeVersion(imageId),
    spacetimeBindings: helpers.bindingsIdentity(localPackage),
    database: null,
  };
}

export function postgresSetupMetadata({ helpers, env }: {
  helpers: { containerImage: (container: string) => string };
  env?: NodeJS.ProcessEnv;
}): StackSetupMetadata {
  return {
    spacetime: null,
    spacetimeBindings: null,
    database: helpers.containerImage(databaseContainerName('postgres', env)),
  };
}

export function mongoDbSetupMetadata({ helpers, env }: {
  helpers: { containerImage: (container: string) => string };
  env?: NodeJS.ProcessEnv;
}): StackSetupMetadata {
  return {
    spacetime: null,
    spacetimeBindings: null,
    database: helpers.containerImage(databaseContainerName('mongodb', env)),
  };
}

export function emptySetupMetadata(): StackSetupMetadata {
  return { spacetime: null, spacetimeBindings: null, database: null };
}

export function spacetimeBuildContainerPlan({ repo, appDir, env = {} }: {
  repo: string; appDir: string; env?: NodeJS.ProcessEnv;
}): BuildContainerPlan {
  const bindings = join(repo, 'crates', 'bindings-typescript');
  const cli = join(repo, 'tools', 'stack-bench', 'container', 'bin', 'spacetimedb-cli');
  const standalone = join(repo, 'tools', 'stack-bench', 'container', 'bin', 'spacetimedb-standalone');
  const config = resolve(appDir, '..', '.spacetime-cli-config');
  const releaseVolume = env.STACK_BENCH_RELEASE_DEPS_VOLUME?.trim() || null;
  if (releaseVolume) {
    return {
      networkNamespace: env.STACK_BENCH_APPLIANCE === '1' ? 'host' : null,
      requiredPaths: [],
      ensureDirectories: [config],
      mounts: [
        { kind: 'bind', source: config,
          target: `${CODING_CONTAINER_AGENT.home}/.config/spacetime`, readOnly: false },
        { kind: 'volume', source: releaseVolume, target: '/release-deps', readOnly: true },
      ],
      init: 'set -eu; '
        + 'mkdir -p /deps; '
        + 'test -d /release-deps/bindings-typescript; '
        + 'test -x /release-deps/spacetimedb-cli; '
        + 'test -x /release-deps/spacetimedb-standalone; '
        + 'test -f /release-deps/spacetimedb.tgz; '
        + 'ln -s /release-deps/spacetimedb-cli /deps/spacetimedb-cli; '
        + 'ln -s /release-deps/spacetimedb-standalone /deps/spacetimedb-standalone; '
        + 'ln -s /release-deps/spacetimedb.tgz /deps/spacetimedb.tgz; '
        + 'touch /deps/.ready; exec sleep infinity',
      readyFile: '/deps/.ready',
      readyDescription: 'SpacetimeDB SDK staging',
    };
  }
  return {
    networkNamespace: null,
    requiredPaths: [bindings, cli, standalone],
    ensureDirectories: [config],
    mounts: [
      { kind: 'bind', source: config,
        target: `${CODING_CONTAINER_AGENT.home}/.config/spacetime`, readOnly: false },
      { kind: 'bind', source: bindings, target: '/deps-src/bindings-typescript', readOnly: true },
      { kind: 'bind', source: cli, target: '/deps/spacetimedb-cli', readOnly: true },
      { kind: 'bind', source: standalone, target: '/deps/spacetimedb-standalone', readOnly: true },
    ],
    init: 'set -eu; '
      + 'test -f /deps-src/bindings-typescript/dist/server/index.d.ts; '
      + 'test -f /deps-src/bindings-typescript/dist/server/index.mjs; '
      + 'mkdir -p /deps/bindings-typescript; '
      + 'cp -a /deps-src/bindings-typescript/. /deps/bindings-typescript/; '
      + 'cd /deps/bindings-typescript; '
      + 'npm install --omit=dev --ignore-scripts --no-audit --no-fund; '
      + 'pack_name=$(npm pack --pack-destination /deps --silent); '
      + 'mv "/deps/$pack_name" /deps/spacetimedb.tgz; '
      + 'touch /deps/.ready; exec sleep infinity',
    readyFile: '/deps/.ready',
    readyDescription: 'SpacetimeDB SDK staging',
  };
}

export function standardBuildContainerPlan({ env = {} }:
  { env?: NodeJS.ProcessEnv } = {}): BuildContainerPlan {
  return {
    networkNamespace: env.STACK_BENCH_APPLIANCE === '1' ? 'host' : null,
    requiredPaths: [], ensureDirectories: [], mounts: [],
    init: 'exec sleep infinity', readyFile: null, readyDescription: null,
  };
}
