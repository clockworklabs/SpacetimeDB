export const DATABASE_IMAGES = {
  postgres: 'postgres:19@sha256:d2c8a7d3d224a9c0335bf9f69ae6ad0b9aa7d88c4b64bab89f0f261332f6d4de',
  mongodb: 'mongo:9@sha256:e5964383224061a512e9a886a4beaf6ed299f0237b7239dbf9876cbc7882910d',
};

type ContainerEnvironment = Readonly<Record<string, string | undefined>>;

const DATABASE_CONTAINERS = Object.freeze({
  mongodb: Object.freeze({ appliance: 'stack-bench-mongodb', development: 'stack-bench-dev-mongodb',
    environmentKey: 'MONGO_CONTAINER', internalPort: 27017 }),
  postgres: Object.freeze({ appliance: 'stack-bench-postgres', development: 'stack-bench-dev-postgres',
    environmentKey: 'POSTGRES_CONTAINER', internalPort: 5432 }),
});

export type DatabaseContainerBackend = keyof typeof DATABASE_CONTAINERS;

export function isDatabaseContainerBackend(backend: string): backend is DatabaseContainerBackend {
  return Object.hasOwn(DATABASE_CONTAINERS, backend);
}

export function databaseContainer(
  backend: string,
  env: ContainerEnvironment = process.env,
): { name: string; internalPort: number } {
  if (!isDatabaseContainerBackend(backend)) {
    throw new Error(`unknown database container backend ${backend}`);
  }
  const definition = DATABASE_CONTAINERS[backend];
  const explicit = String(env[definition.environmentKey] ?? '').trim();
  const name = explicit || (env.STACK_BENCH_APPLIANCE === '1'
    ? definition.appliance : definition.development);
  return { name, internalPort: definition.internalPort };
}

export function databaseContainerName(
  backend: string,
  env: ContainerEnvironment = process.env,
): string {
  return databaseContainer(backend, env).name;
}
