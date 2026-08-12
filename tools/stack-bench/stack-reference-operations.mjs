import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

function validateHostedDatabase({ args, lease, track, helpers }) {
  const expected = helpers.dbName(track, args.runIndex);
  if (lease.resources.database !== expected) throw new Error(`lease database is not ${expected}`);
  const service = lease.resources.container;
  const actual = helpers.runSync('inspecting leased database container', 'docker',
    ['inspect', '--format', '{{.Id}}', service.name],
    { encoding: 'utf8', stdio: 'pipe' }).trim();
  if (actual !== service.id) throw new Error(`${service.name} no longer matches its lease`);
  return { expected, service };
}

async function deployHostedReference(input, { databaseUrl, extraEnv = {}, prepare, pushSchema }) {
  const { args, metadata, lease, track, container, ports, helpers } = input;
  helpers.phase('preparing database');
  const database = validateHostedDatabase(input);
  prepare(database, helpers);
  const serverEnv = {
    DATABASE_URL: databaseUrl({ ports, lease }),
    PORT: String(ports.express),
    ...extraEnv,
  };
  writeFileSync(join(args.app, metadata.server.directory, '.env'),
    `${Object.entries(serverEnv).map(([key, value]) => `${key}=${value}`).join('\n')}\n`);
  for (const directory of metadata.installDirectories) {
    helpers.phase(`installing ${directory}`);
    helpers.docker(container, `/app/${directory}`, 'npm', ['ci', '--no-audit', '--no-fund']);
  }
  if (pushSchema) pushSchema({ container, metadata, serverEnv, helpers });
  helpers.phase('starting reference server and client');
  helpers.startDetached(container, `/app/${metadata.server.directory}`, 'reference-server', serverEnv);
  helpers.startDetached(container, `/app/${metadata.client.directory}`, 'reference-client', {
    API_PORT: String(ports.express), VITE_PORT: String(ports.vite),
  });
  await helpers.waitFor(`http://127.0.0.1:${ports.express}${track.restartProbe}`, 180_000,
    `${args.backend} API`, () => helpers.containerLogs(container, 'reference-server'));
  helpers.phase('reference server ready');
  await helpers.waitFor(`http://127.0.0.1:${ports.vite}`, 180_000,
    `${args.backend} client`, () => helpers.containerLogs(container, 'reference-client'));
  helpers.phase('reference client ready');
}

export function deployPostgresReference(input) {
  return deployHostedReference(input, {
    databaseUrl: ({ ports, lease }) =>
      `postgresql://stackbench:stackbench@host.docker.internal:${ports.dbPort}/${lease.resources.database}`,
    prepare: ({ expected, service }, helpers) => {
      try {
        helpers.runSync('creating PostgreSQL reference database', 'docker',
          ['exec', service.name, 'psql', '-U', 'stackbench', '-d', 'postgres',
            '-c', `CREATE DATABASE ${expected} OWNER stackbench;`], { stdio: 'pipe' });
      } catch { /* an existing run-index database is expected */ }
      helpers.runSync('resetting PostgreSQL reference schema', 'docker',
        ['exec', service.name, 'psql', '-U', 'stackbench', '-d', expected,
          '-c', 'DROP SCHEMA public CASCADE; CREATE SCHEMA public; GRANT ALL ON SCHEMA public TO stackbench;'],
        { stdio: 'pipe' });
    },
    pushSchema: ({ container, metadata, serverEnv, helpers }) => {
      helpers.phase('pushing PostgreSQL schema');
      helpers.docker(container, `/app/${metadata.server.directory}`, 'npx',
        ['drizzle-kit', 'push', '--force'], serverEnv);
    },
  });
}

export function deployMongoDbReference(input) {
  return deployHostedReference(input, {
    databaseUrl: ({ ports, lease }) =>
      `mongodb://host.docker.internal:${ports.dbPort}/${lease.resources.database}`,
    extraEnv: { JWT_SECRET: 'stack-bench-reference-only-secret-2026' },
    prepare: ({ expected, service }, helpers) => {
      helpers.runSync('resetting MongoDB reference database', 'docker',
        ['exec', service.name, 'mongosh', expected, '--quiet', '--eval', 'db.dropDatabase()'],
        { stdio: 'pipe' });
    },
  });
}

export async function deploySpacetimeReference({ args, metadata, lease, container, ports, helpers }) {
  for (const directory of metadata.installDirectories) {
    helpers.docker(container, `/app/${directory}`, 'npm', ['ci', '--no-audit', '--no-fund']);
  }
  const module = helpers.moduleName(helpers.loadTrack(args.track), args.runIndex);
  if (lease.resources.module !== module) throw new Error(`lease module is not ${module}`);
  const hostUri = lease.resources.serverUri.replace('127.0.0.1', 'host.docker.internal')
    .replace('localhost', 'host.docker.internal');
  helpers.docker(container, `/app/${metadata.moduleDirectory}`, '/deps/spacetimedb-cli',
    ['publish', module, '--module-path', `/app/${metadata.moduleDirectory}`, '-s', hostUri, '-y']);
  helpers.docker(container, `/app/${metadata.moduleDirectory}`, '/deps/spacetimedb-cli',
    ['generate', '--lang', 'typescript', '--module-path', `/app/${metadata.moduleDirectory}`,
      '--out-dir', `/app/${metadata.bindingsDirectory}`, '--yes', '--no-config']);
  helpers.startDetached(container, `/app/${metadata.client.directory}`, 'reference-client', {
    VITE_MODULE_NAME: module, VITE_SPACETIMEDB_URI: lease.resources.serverUri,
    VITE_PORT: String(ports.vite),
  });
  await helpers.waitFor(`http://127.0.0.1:${ports.vite}`, 180_000, 'Spacetime client',
    () => helpers.containerLogs(container, 'reference-client'));
}
