import { Buffer } from 'node:buffer';
import { join } from 'node:path';
import { CODING_CONTAINER_DEPENDENCY_READY_FILE,
  CODING_CONTAINER_RELEASE_DEPS_ROOT, CODING_CONTAINER_SPACETIME_CLI,
  CODING_CONTAINER_SPACETIME_PACKAGE, CODING_CONTAINER_SPACETIME_STANDALONE }
  from '../runtime/coding-container-policy.js';
import { DEFAULT_SPACETIME_SERVER_URI, loopbackHttpUri } from '../runtime/backend-lease.js';
import { databaseContainerName } from './database-containers.js';
import { POSTGRES_APPLICATION_IDENTITY } from './hosted-database-identity.js';

type HostUrl = (url: string) => string;

interface StackSetupMetadata {
  spacetime: { commit: string | null; binarySha256: string | null; raw: string } | null;
  spacetimeBindings: { package: string; sourceSha256: string | null; sourceFiles: number } | null;
  database: { reference: string; imageId: string | null | undefined } | null;
}

interface StackSetupMetadataInput {
  imageId: string;
  localPackage: string;
  env?: NodeJS.ProcessEnv;
  helpers: {
    linuxSpacetimeVersion: (imageId: string) => NonNullable<StackSetupMetadata['spacetime']>;
    bindingsIdentity: (localPackage: string) => NonNullable<StackSetupMetadata['spacetimeBindings']>;
    containerImage: (container: string) => NonNullable<StackSetupMetadata['database']>;
  };
}

export interface BuildContainerPlan {
  networkNamespace: string | null;
  requiredPaths: string[];
  ensureDirectories: string[];
  mounts: Array<{ kind: 'bind' | 'volume'; source: string; target: string; readOnly: boolean }>;
  init: string;
  readyFile: string | null;
  readyDescription: string | null;
}

const SPACETIME_CLI_BINARY = '/deps/.spacetimedb-cli';
const SPACETIME_CLI_HOME = '/deps/.spacetime-owner';

function spacetimeCliSetup(serverUri: URL): string {
  const uri = serverUri.toString().replace(/\/$/, '');
  const wrapper = `#!/bin/sh
case " $* " in
  *" publish "*" --anonymous "*|*" publish "*" --no-config "*|*" dev "*" --anonymous "*|*" dev "*" --no-config "*)
    echo "SpacetimeDB publish and dev must use the run identity" >&2
    exit 64
    ;;
esac
exec env HOME=${SPACETIME_CLI_HOME} ${SPACETIME_CLI_BINARY} "$@"
`;
  const encodedWrapper = Buffer.from(wrapper).toString('base64');
  return `mkdir -p ${SPACETIME_CLI_HOME}; `
    + `identity=$(curl -fsS -X POST ${uri}/v1/identity); `
    + `token=$(node -e 'process.stdout.write(JSON.parse(process.argv[1]).token)' "$identity"); `
    + `HOME=${SPACETIME_CLI_HOME} ${SPACETIME_CLI_BINARY} login --token "$token" >/dev/null; `
    + `chmod -R a-w ${SPACETIME_CLI_HOME}; `
    + `printf %s ${encodedWrapper} | base64 -d > ${CODING_CONTAINER_SPACETIME_CLI}; `
    + `chmod 755 ${CODING_CONTAINER_SPACETIME_CLI}; `;
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

export function spacetimeSetupMetadata({ imageId, localPackage, helpers }:
  StackSetupMetadataInput): StackSetupMetadata {
  return {
    spacetime: helpers.linuxSpacetimeVersion(imageId),
    spacetimeBindings: helpers.bindingsIdentity(localPackage),
    database: null,
  };
}

export function postgresSetupMetadata({ helpers, env }:
  StackSetupMetadataInput): StackSetupMetadata {
  return {
    spacetime: null,
    spacetimeBindings: null,
    database: helpers.containerImage(databaseContainerName('postgres', env)),
  };
}

export function mongoDbSetupMetadata({ helpers, env }:
  StackSetupMetadataInput): StackSetupMetadata {
  return {
    spacetime: null,
    spacetimeBindings: null,
    database: helpers.containerImage(databaseContainerName('mongodb', env)),
  };
}

export function emptySetupMetadata(_input?: StackSetupMetadataInput): StackSetupMetadata {
  return { spacetime: null, spacetimeBindings: null, database: null };
}

export function spacetimeBuildContainerPlan({ repo, env = {} }: {
  repo: string; appDir: string; env?: NodeJS.ProcessEnv;
}): BuildContainerPlan {
  const bindings = join(repo, 'crates', 'bindings-typescript');
  const cli = join(repo, 'tools', 'stack-bench', 'container', 'bin', 'spacetimedb-cli');
  const standalone = join(repo, 'tools', 'stack-bench', 'container', 'bin', 'spacetimedb-standalone');
  const serverUri = loopbackHttpUri(env.STACK_BENCH_STDB_URI ?? DEFAULT_SPACETIME_SERVER_URI);
  if (env.STACK_BENCH_APPLIANCE !== '1') serverUri.hostname = 'host.docker.internal';
  const cliSetup = spacetimeCliSetup(serverUri);
  const releaseVolume = env.STACK_BENCH_RELEASE_DEPS_VOLUME?.trim() || null;
  if (releaseVolume) {
    return {
      networkNamespace: env.STACK_BENCH_APPLIANCE === '1' ? 'host' : null,
      requiredPaths: [],
      ensureDirectories: [],
      mounts: [
        { kind: 'volume', source: releaseVolume,
          target: CODING_CONTAINER_RELEASE_DEPS_ROOT, readOnly: true },
      ],
      init: 'set -eu; '
        + 'mkdir -p /deps; '
        + `test -d ${CODING_CONTAINER_RELEASE_DEPS_ROOT}/bindings-typescript; `
        + `test -x ${CODING_CONTAINER_RELEASE_DEPS_ROOT}/spacetimedb-cli; `
        + `test -x ${CODING_CONTAINER_RELEASE_DEPS_ROOT}/spacetimedb-standalone; `
        + `test -f ${CODING_CONTAINER_RELEASE_DEPS_ROOT}/spacetimedb.tgz; `
        + `ln -s ${CODING_CONTAINER_RELEASE_DEPS_ROOT}/spacetimedb-cli ${SPACETIME_CLI_BINARY}; `
        + `ln -s ${CODING_CONTAINER_RELEASE_DEPS_ROOT}/spacetimedb-standalone ${CODING_CONTAINER_SPACETIME_STANDALONE}; `
        + `ln -s ${CODING_CONTAINER_RELEASE_DEPS_ROOT}/spacetimedb.tgz ${CODING_CONTAINER_SPACETIME_PACKAGE}; `
        + cliSetup
        + `touch ${CODING_CONTAINER_DEPENDENCY_READY_FILE}; exec sleep infinity`,
      readyFile: CODING_CONTAINER_DEPENDENCY_READY_FILE,
      readyDescription: 'SpacetimeDB SDK staging',
    };
  }
  return {
    networkNamespace: null,
    requiredPaths: [bindings, cli, standalone],
    ensureDirectories: [],
    mounts: [
      { kind: 'bind', source: bindings, target: '/deps-src/bindings-typescript', readOnly: true },
      { kind: 'bind', source: cli, target: SPACETIME_CLI_BINARY, readOnly: true },
      { kind: 'bind', source: standalone,
        target: CODING_CONTAINER_SPACETIME_STANDALONE, readOnly: true },
    ],
    init: 'set -eu; '
      + 'test -f /deps-src/bindings-typescript/dist/server/index.d.ts; '
      + 'test -f /deps-src/bindings-typescript/dist/server/index.mjs; '
      + 'mkdir -p /deps/bindings-typescript; '
      + 'cp -a /deps-src/bindings-typescript/. /deps/bindings-typescript/; '
      + 'cd /deps/bindings-typescript; '
      + 'npm install --omit=dev --ignore-scripts --no-audit --no-fund; '
      + 'pack_name=$(npm pack --pack-destination /deps --silent); '
      + `mv "/deps/$pack_name" ${CODING_CONTAINER_SPACETIME_PACKAGE}; `
      + cliSetup
      + `touch ${CODING_CONTAINER_DEPENDENCY_READY_FILE}; exec sleep infinity`,
    readyFile: CODING_CONTAINER_DEPENDENCY_READY_FILE,
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
