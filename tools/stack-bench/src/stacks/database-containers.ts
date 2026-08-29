type ContainerEnvironment = Readonly<Record<string, string | undefined>>;

const DEFAULTS = Object.freeze({
  mongodb: Object.freeze({ appliance: 'stack-bench-mongodb', development: 'stack-bench-dev-mongodb' }),
  postgres: Object.freeze({ appliance: 'stack-bench-postgres', development: 'stack-bench-dev-postgres' }),
});

export function databaseContainerName(
  backend: string,
  env: ContainerEnvironment = process.env,
): string {
  if (backend !== 'mongodb' && backend !== 'postgres') {
    throw new Error(`unknown database container backend ${backend}`);
  }
  const names = DEFAULTS[backend];
  const explicit = backend === 'postgres' ? env.POSTGRES_CONTAINER : env.MONGO_CONTAINER;
  if (String(explicit ?? '').trim()) return String(explicit).trim();
  return env.STACK_BENCH_APPLIANCE === '1' ? names.appliance : names.development;
}
