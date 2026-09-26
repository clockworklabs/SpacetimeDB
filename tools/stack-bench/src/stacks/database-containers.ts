export const DATABASE_IMAGES = {
  postgres: 'postgres:16@sha256:219341e4cedb06c8634f80af40851da3425b41b76603fd890272f58e37e139f7',
  mongodb: 'mongo:7@sha256:554a9bb1ec6e00c40ba078a41974a834d1a9a8ab1772645b69142afecc87f082',
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
