import { join, resolve } from 'node:path';

export function postgresConnectionUrl({ dbPort, database, hostUrl }) {
  return hostUrl(`postgresql://stackbench:stackbench@localhost:${dbPort}/${database}`);
}

export function mongoDbConnectionUrl({ dbPort, database, hostUrl }) {
  return hostUrl(`mongodb://localhost:${dbPort}/${database}`);
}

export function noConnectionUrl() {
  return null;
}

export function spacetimeSetupMetadata({ imageId, localPackage, helpers }) {
  return {
    spacetime: helpers.linuxSpacetimeVersion(imageId),
    spacetimeBindings: helpers.bindingsIdentity(localPackage),
    database: null,
  };
}

export function postgresSetupMetadata({ helpers, env }) {
  return {
    spacetime: null,
    spacetimeBindings: null,
    database: helpers.containerImage(env.POSTGRES_CONTAINER ?? 'stack-bench-postgres'),
  };
}

export function mongoDbSetupMetadata({ helpers, env }) {
  return {
    spacetime: null,
    spacetimeBindings: null,
    database: helpers.containerImage(env.MONGO_CONTAINER ?? 'stack-bench-mongodb'),
  };
}

export function emptySetupMetadata() {
  return { spacetime: null, spacetimeBindings: null, database: null };
}

export function spacetimeBuildContainerPlan({ repo, appDir }) {
  const bindings = join(repo, 'crates', 'bindings-typescript');
  const cli = join(repo, 'tools', 'stack-bench', 'container', 'bin', 'spacetimedb-cli');
  const standalone = join(repo, 'tools', 'stack-bench', 'container', 'bin', 'spacetimedb-standalone');
  const config = resolve(appDir, '..', '.spacetime-cli-config');
  return {
    requiredPaths: [bindings, cli, standalone],
    ensureDirectories: [config],
    mounts: [
      { source: config, target: '/root/.config/spacetime', readOnly: false },
      { source: bindings, target: '/deps-src/bindings-typescript', readOnly: true },
      { source: cli, target: '/deps/spacetimedb-cli', readOnly: true },
      { source: standalone, target: '/deps/spacetimedb-standalone', readOnly: true },
    ],
    init: 'set -eu; mkdir -p /deps/bindings-typescript; '
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

export function standardBuildContainerPlan() {
  return {
    requiredPaths: [], ensureDirectories: [], mounts: [],
    init: 'exec sleep infinity', readyFile: null, readyDescription: null,
  };
}
