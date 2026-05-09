export const validConnectors = [
  'convex',
  'spacetimedb',
  'bun',
  'postgres_rpc',
  'cockroach_rpc',
  'sqlite_rpc',
  'supabase_rpc',
  'planetscale_pg_rpc',
  'postgres_storedproc_rpc',
] as const;

export type ConnectorKey = (typeof validConnectors)[number];

export const validStdbCompressions = ['none', 'gzip'] as const;

export type StdbCompression = (typeof validStdbCompressions)[number];

export const defaultDemoSystems: readonly ConnectorKey[] = [
  'convex',
  'spacetimedb',
];

export const defaultBenchTestName = 'test-1';

export interface ContentionTests {
  startAlpha: number;
  endAlpha: number;
  step: number;
  concurrency: number;
}

export interface ConcurrencyTests {
  startConc: number;
  endConc: number;
  step: number;
  alpha: number;
}

export interface SharedRuntimeConfig {
  accounts: number;
  initialBalance: number;
  stdbUrl: string;
  stdbModule: string;
  stdbModulePath: string;
  stdbCompression: StdbCompression;
  stdbConfirmedReads: boolean;
  useDocker: boolean;
  poolMax: number;
  bunUrl: string;
  convexUrl: string;
  convexDir: string;
  opTimeoutMs: number;
  minOpTimeoutMs: number;
  tailSlackMs: number;
  precomputedTransferPairs: number;
  benchPipelined?: boolean;
  maxInflightPerWorker?: number;
  logErrors: boolean;
  verifyTransactions: boolean;
}

export interface DemoOptions extends SharedRuntimeConfig {
  seconds: number;
  concurrency: number;
  alpha: number;
  systems: ConnectorKey[];
  skipPrep: boolean;
  noAnimation: boolean;
}

export interface BenchOptions extends SharedRuntimeConfig {
  testName: string;
  seconds: number;
  concurrency: number;
  alpha: number;
  connectors: ConnectorKey[] | null;
  contentionTests: ContentionTests | null;
  concurrencyTests: ConcurrencyTests | null;
}

export type SeedConfig = Pick<SharedRuntimeConfig, 'accounts' | 'initialBalance'>;

export type ConnectorRuntimeConfig = Pick<
  SharedRuntimeConfig,
  | 'bunUrl'
  | 'convexUrl'
  | 'initialBalance'
  | 'stdbCompression'
  | 'stdbConfirmedReads'
  | 'stdbModule'
  | 'stdbUrl'
>;

export interface SpacetimeConnectorConfig {
  initialBalance: number;
  stdbCompression: StdbCompression;
  stdbConfirmedReads: boolean;
  stdbModule: string;
  stdbUrl: string;
}

export type RunnerRuntimeConfig = Pick<
  SharedRuntimeConfig,
  | 'benchPipelined'
  | 'logErrors'
  | 'maxInflightPerWorker'
  | 'minOpTimeoutMs'
  | 'opTimeoutMs'
  | 'precomputedTransferPairs'
  | 'tailSlackMs'
  | 'verifyTransactions'
>;

export type ConvexInitConfig = SeedConfig &
  Pick<SharedRuntimeConfig, 'convexDir' | 'convexUrl'>;

export type SpacetimeInitConfig = SeedConfig &
  Pick<SharedRuntimeConfig, 'stdbModule' | 'stdbModulePath'>;

function parseFiniteNumber(raw: string, label: string): number {
  const value = Number(raw);
  if (!Number.isFinite(value)) {
    throw new Error(`invalid number for ${label}: ${raw}`);
  }
  return value;
}

function parseBooleanLike(raw: string | boolean): boolean {
  if (typeof raw === 'boolean') return raw;
  return !(raw === '0' || raw === '' || raw === 'false');
}

export function normalizeStdbUrl(url: string): string {
  return url.replace(/^(http|ws)s?:\/\//, '');
}

export function parseStdbCompression(
  raw: string,
  label: string,
): StdbCompression {
  if (validStdbCompressions.includes(raw as StdbCompression)) {
    return raw as StdbCompression;
  }

  throw new Error(
    `invalid value for ${label}: ${raw} (expected one of: ${validStdbCompressions.join(', ')})`,
  );
}

export function readNumberEnv(
  name: string,
  defaultValue: number,
  env: NodeJS.ProcessEnv = process.env,
): number {
  const raw = env[name];
  if (raw === undefined) return defaultValue;
  return parseFiniteNumber(raw, name);
}

export function readOptionalNumberEnv(
  name: string,
  env: NodeJS.ProcessEnv = process.env,
): number | undefined {
  const raw = env[name];
  if (raw === undefined) return undefined;
  return parseFiniteNumber(raw, name);
}

export function readStringEnv(
  name: string,
  defaultValue: string,
  env: NodeJS.ProcessEnv = process.env,
): string {
  return env[name] ?? defaultValue;
}

export function readBooleanEnv(
  name: string,
  defaultValue: boolean,
  env: NodeJS.ProcessEnv = process.env,
): boolean {
  const raw = env[name];
  if (raw === undefined) return defaultValue;
  return parseBooleanLike(raw);
}

export function readOptionalBooleanEnv(
  name: string,
  env: NodeJS.ProcessEnv = process.env,
): boolean | undefined {
  const raw = env[name];
  if (raw === undefined) return undefined;
  return parseBooleanLike(raw);
}

export function parseConnectorList(
  raw: string | string[] | undefined,
  label: string,
): ConnectorKey[] | undefined {
  if (raw === undefined) return undefined;

  const values = (Array.isArray(raw) ? raw : raw.split(','))
    .flatMap((value) => value.split(','))
    .map((value) => value.trim())
    .filter(Boolean);

  if (values.length === 0) {
    return undefined;
  }

  for (const value of values) {
    if (!validConnectors.includes(value as ConnectorKey)) {
      throw new Error(`${value} is not a valid value for ${label}`);
    }
  }

  return values as ConnectorKey[];
}

export function getSharedRuntimeDefaults(
  env: NodeJS.ProcessEnv = process.env,
): SharedRuntimeConfig {
  return {
    accounts: readNumberEnv('SEED_ACCOUNTS', 100_000, env),
    initialBalance: readNumberEnv('SEED_INITIAL_BALANCE', 10_000_000, env),
    stdbUrl: normalizeStdbUrl(readStringEnv('STDB_URL', '127.0.0.1:3000', env)),
    stdbModule: readStringEnv('STDB_MODULE', 'test-1', env),
    stdbModulePath: readStringEnv('STDB_MODULE_PATH', './spacetimedb', env),
    stdbCompression: parseStdbCompression(
      readStringEnv('STDB_COMPRESSION', 'none', env),
      'STDB_COMPRESSION',
    ),
    stdbConfirmedReads: readBooleanEnv('STDB_CONFIRMED_READS', true, env),
    useDocker: readBooleanEnv('USE_DOCKER', false, env),
    poolMax: readNumberEnv('MAX_POOL', 1000, env),
    bunUrl: readStringEnv('BUN_URL', 'http://127.0.0.1:4000', env),
    convexUrl: readStringEnv('CONVEX_URL', 'http://127.0.0.1:3210', env),
    convexDir: readStringEnv('CONVEX_DIR', './convex-app', env),
    opTimeoutMs: readNumberEnv('BENCH_OP_TIMEOUT_MS', 15000, env),
    minOpTimeoutMs: readNumberEnv('MIN_OP_TIMEOUT_MS', 250, env),
    tailSlackMs: readNumberEnv('TAIL_SLACK_MS', 1000, env),
    precomputedTransferPairs: readNumberEnv(
      'BENCH_PRECOMPUTED_TRANSFER_PAIRS',
      10_000_000,
      env,
    ),
    benchPipelined: readOptionalBooleanEnv('BENCH_PIPELINED', env),
    maxInflightPerWorker: readOptionalNumberEnv(
      'MAX_INFLIGHT_PER_WORKER',
      env,
    ),
    logErrors: readBooleanEnv('LOG_ERRORS', false, env),
    verifyTransactions: readBooleanEnv('VERIFY', false, env),
  };
}
