import { execFileSync } from 'node:child_process';
import { createHash, createHmac, randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { loopbackHttpUri } from '../../runtime/backend-lease.js';
import type { BackendLease, BackendLeaseContainer } from '../../runtime/backend-lease.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';
import { CODING_CONTAINER_CONTROL_DIR } from '../../runtime/coding-container-policy.js';
import { requireAttemptNetwork } from '../../runtime/docker-network.js';
import { STACK_BENCH_ROOT } from '../../package-root.js';
import { SUPABASE_IMAGES } from './supabase-identity.js';

const MIB = 1024 ** 2;
// Self-hosted Supabase at supabase/supabase docker/ e8547352c529ed99545fafbc8619dec42945d74e,
// reduced to the services an application uses. Studio, postgres-meta, Supavisor
// and imgproxy are admin or optional surfaces and are not started.
// Caps, not reservations; qualification measures the real peaks.
export const SUPABASE_CONTAINER_LIMITS = Object.freeze({
  db: { cpuCount: 1, memoryBytes: 1024 * MIB, pids: 512 },
  auth: { cpuCount: 1, memoryBytes: 128 * MIB, pids: 256 },
  rest: { cpuCount: 1, memoryBytes: 256 * MIB, pids: 256 },
  realtime: { cpuCount: 1, memoryBytes: 512 * MIB, pids: 256 },
  storage: { cpuCount: 1, memoryBytes: 384 * MIB, pids: 256 },
  functions: { cpuCount: 1, memoryBytes: 512 * MIB, pids: 512 },
  gateway: { cpuCount: 1, memoryBytes: 128 * MIB, pids: 256 },
});
// The platform services that join the anchor, which runs the database.
export type SupabaseServiceRole = Exclude<keyof typeof SUPABASE_CONTAINER_LIMITS, 'db'>;
export const SUPABASE_SERVICE_ROLES = Object.freeze(Object.keys(SUPABASE_CONTAINER_LIMITS)
  .filter(role => role !== 'db')) as readonly SupabaseServiceRole[];

// The service names the upstream configuration uses, all on the shared loopback.
export const SUPABASE_HOSTS = Object.freeze(['db', 'auth', 'rest', 'realtime-dev.supabase-realtime',
  'storage', 'functions', 'envoy', 'kong']);
export const SUPABASE_JWT_EXPIRY_SECONDS = 3600;
export const SUPABASE_DB_PROCESS_RECORD = `${CODING_CONTAINER_CONTROL_DIR}/restart-supabase-db.pid`;
export const SUPABASE_DB_SOCKET = '/var/run/postgresql';
// Root-only, in the anchor. Never in the lease, arguments, logs or evidence.
export const SUPABASE_SECRET_FILE = '/run/stack-bench/supabase.env';
export const SUPABASE_STORAGE_ROOT = '/var/lib/storage';

const ASSETS = join(STACK_BENCH_ROOT, 'src', 'stacks', 'backends', 'supabase');
export function supabaseAsset(path: string): string {
  return readFileSync(join(ASSETS, path), 'utf8');
}

export interface SupabaseSecrets { postgresPassword: string; jwtSecret: string; anonKey: string; serviceRoleKey: string }
// Values only platform services use. `adminPassword` belongs to supabase_admin,
// the superuser, so application database credentials cannot reach it.
export interface SupabasePlatformSecrets extends SupabaseSecrets {
  adminPassword: string; realtimeEncryptionKey: string; realtimeSecretKeyBase: string;
  s3AccessKeyId: string; s3AccessKeySecret: string;
}

const SECRET_NAMES = Object.freeze({
  POSTGRES_PASSWORD: 'postgresPassword', JWT_SECRET: 'jwtSecret', ANON_KEY: 'anonKey',
  SERVICE_ROLE_KEY: 'serviceRoleKey', ADMIN_PASSWORD: 'adminPassword',
  REALTIME_DB_ENC_KEY: 'realtimeEncryptionKey', SECRET_KEY_BASE: 'realtimeSecretKeyBase',
  S3_PROTOCOL_ACCESS_KEY_ID: 's3AccessKeyId', S3_PROTOCOL_ACCESS_KEY_SECRET: 's3AccessKeySecret',
} as const satisfies Record<string, keyof SupabasePlatformSecrets>);

function signJwt(secret: string, role: string, issuedAt: number): string {
  const encode = (value: unknown) => Buffer.from(JSON.stringify(value)).toString('base64url');
  const body = `${encode({ alg: 'HS256', typ: 'JWT' })}.${encode({ role, iss: 'supabase', iat: issuedAt,
    exp: issuedAt + 365 * 86_400 })}`;
  return `${body}.${createHmac('sha256', secret).update(body).digest('base64url')}`;
}

// Legacy HS256 keys, as the self-hosted setup generates them.
export function generateSupabaseSecrets(random: (size: number) => Buffer = randomBytes,
  issuedAt = Math.floor(Date.now() / 1000)): SupabasePlatformSecrets {
  const hex = (size: number) => random(size).toString('hex');
  const jwtSecret = hex(24);
  return {
    postgresPassword: hex(24), jwtSecret, anonKey: signJwt(jwtSecret, 'anon', issuedAt),
    serviceRoleKey: signJwt(jwtSecret, 'service_role', issuedAt), adminPassword: hex(24),
    // Realtime encrypts tenant settings with AES-128: exactly 16 characters.
    realtimeEncryptionKey: hex(8), realtimeSecretKeyBase: hex(32),
    s3AccessKeyId: hex(16), s3AccessKeySecret: hex(32),
  };
}

const SECRET_VALUE = /^[A-Za-z0-9._-]{16,512}$/;

export function formatSupabaseSecrets(secrets: SupabasePlatformSecrets): string {
  return Object.entries(SECRET_NAMES).map(([name, field]) => {
    if (!SECRET_VALUE.test(secrets[field])) throw new Error(`Supabase secret ${name} is invalid`);
    return `${name}=${secrets[field]}\n`;
  }).join('');
}

export function parseSupabaseSecrets(text: string): SupabasePlatformSecrets {
  const values = new Map<string, string>();
  for (const line of text.split('\n').filter(Boolean)) {
    const match = /^([A-Z0-9_]+)=(.*)$/.exec(line);
    if (!match || values.has(match[1]!) || !Object.hasOwn(SECRET_NAMES, match[1]!)
      || !SECRET_VALUE.test(match[2]!)) throw new Error('Supabase secret file is malformed');
    values.set(match[1]!, match[2]!);
  }
  return Object.fromEntries(Object.entries(SECRET_NAMES).map(([name, field]) => {
    const value = values.get(name);
    if (value === undefined) throw new Error(`Supabase secret file lacks ${name}`);
    return [field, value];
  })) as unknown as SupabasePlatformSecrets;
}

// The owned anchor of an isolated Supabase attempt: exact identity, running
// since the namespace was recorded.
export function supabaseAnchor(lease: BackendLease, exec: TextCommandExecutor = execFileSync): BackendLeaseContainer {
  if (lease.backend !== 'supabase') throw new Error(`lease ${lease.runId} is not a Supabase lease`);
  const container = lease.resources.container;
  if (!container?.owned) throw new Error('Supabase lease has no owned platform container');
  requireAttemptNetwork(lease, exec, container);
  return container;
}

export function readSupabasePlatformSecrets(lease: BackendLease,
  exec: TextCommandExecutor = execFileSync): SupabasePlatformSecrets {
  const anchor = supabaseAnchor(lease, exec);
  return parseSupabaseSecrets(exec('docker', ['exec', anchor.id, 'cat', SUPABASE_SECRET_FILE],
    { encoding: 'utf8', stdio: 'pipe', timeout: 30_000 }));
}

// supabase_admin over the anchor's own socket, which only the harness can reach.
export const SUPABASE_PSQL = Object.freeze(['psql', '-h', SUPABASE_DB_SOCKET, '-U', 'supabase_admin', '-d', 'postgres',
  '-v', 'ON_ERROR_STOP=1', '-X', '-q', '-At']);

export function supabasePsqlArguments(anchorId: string): string[] {
  return ['exec', '-i', '--user', 'postgres', anchorId, ...SUPABASE_PSQL];
}

// Privileged: observers, the external stock writer, reset and setup only.
export function supabaseSql(lease: BackendLease, sql: string,
  { exec = execFileSync, timeoutMs = 30_000 }: { exec?: TextCommandExecutor; timeoutMs?: number } = {}): string {
  const anchor = supabaseAnchor(lease, exec);
  return exec('docker', supabasePsqlArguments(anchor.id),
    { encoding: 'utf8', stdio: 'pipe', timeout: timeoutMs, input: sql });
}

export function supabaseGatewayUrl(lease: BackendLease): string {
  if (lease.backend !== 'supabase') throw new Error(`lease ${lease.runId} is not a Supabase lease`);
  return loopbackHttpUri(lease.resources.serverUri).origin;
}

// Pinned upstream gateway configuration (see gateway/*.yaml): keys, public URL
// and listener port are filled here. No remaining route needs dashboard basic
// auth, but its filter requires a valid entry, so it holds a random unused one.
export function renderSupabaseGateway({ anonKey, serviceRoleKey, gatewayPort, random = randomBytes }: {
  anonKey: string; serviceRoleKey: string; gatewayPort: number; random?: (size: number) => Buffer;
}): Record<'envoy.yaml' | 'lds.yaml' | 'cds.yaml', string> {
  if (!Number.isInteger(gatewayPort) || gatewayPort < 1 || gatewayPort > 65535) throw new Error('invalid gateway port');
  const values: Record<string, string> = {
    ANON_KEY: anonKey, SERVICE_ROLE_KEY: serviceRoleKey, GATEWAY_PORT: String(gatewayPort),
    SUPABASE_PUBLIC_URL: `http://127.0.0.1:${gatewayPort}`,
    DASHBOARD_BASIC_AUTH: `unused:{SHA}${createHash('sha1').update(random(32)).digest('base64')}`,
  };
  const lds = supabaseAsset('gateway/lds.yaml').replace(/\$\{([A-Z_]+)\}/g, (_, name: string) => {
    const value = values[name];
    if (value === undefined) throw new Error(`gateway template has unknown placeholder ${name}`);
    return value;
  });
  return { 'envoy.yaml': supabaseAsset('gateway/envoy.yaml'), 'lds.yaml': lds, 'cds.yaml': supabaseAsset('gateway/cds.yaml') };
}

export interface SupabaseServiceSpec {
  image: string;
  args: string[];
  command: string[];
  // Values for `-e NAME` arguments; they reach Docker through its environment.
  environment: Record<string, string>;
  // Files copied into the created container before it starts: an existing
  // directory in the image, then paths relative to it.
  files: Record<string, Record<string, string>>;
  limits: { cpuCount: number; memoryBytes: number; pids: number };
}

export function supabaseServiceSpecs({ gatewayPort, sitePort, app, secrets, random }: {
  gatewayPort: number; sitePort: number; app: string; secrets: SupabasePlatformSecrets;
  random?: (size: number) => Buffer;
}): Record<SupabaseServiceRole, SupabaseServiceSpec> {
  const gateway = `http://127.0.0.1:${gatewayPort}`;
  const database = (user: string, password = secrets.postgresPassword) =>
    `postgres://${user}:${password}@db:5432/postgres`;
  const spec = (role: SupabaseServiceRole, settings: Record<string, string>, environment: Record<string, string>,
    extra: Partial<Pick<SupabaseServiceSpec, 'args' | 'command' | 'files'>> = {}): SupabaseServiceSpec => ({
    image: SUPABASE_IMAGES[role],
    args: [...(extra.args ?? []), ...Object.entries(settings).flatMap(([key, value]) => ['-e', `${key}=${value}`]),
      ...Object.keys(environment).flatMap(key => ['-e', key])],
    command: extra.command ?? [], environment, files: extra.files ?? {}, limits: SUPABASE_CONTAINER_LIMITS[role],
  });
  const jwtExpiry = String(SUPABASE_JWT_EXPIRY_SECONDS);
  return {
    auth: spec('auth', {
      GOTRUE_API_HOST: '127.0.0.1', GOTRUE_API_PORT: '9999', API_EXTERNAL_URL: `${gateway}/auth/v1`,
      GOTRUE_DB_DRIVER: 'postgres', GOTRUE_SITE_URL: `http://127.0.0.1:${sitePort}`,
      GOTRUE_DISABLE_SIGNUP: 'false', GOTRUE_JWT_ADMIN_ROLES: 'service_role', GOTRUE_JWT_AUD: 'authenticated',
      GOTRUE_JWT_DEFAULT_GROUP_NAME: 'authenticated', GOTRUE_JWT_EXP: jwtExpiry,
      GOTRUE_JWT_ISSUER: `${gateway}/auth/v1`, GOTRUE_EXTERNAL_EMAIL_ENABLED: 'true',
      GOTRUE_EXTERNAL_ANONYMOUS_USERS_ENABLED: 'false', GOTRUE_MAILER_AUTOCONFIRM: 'true',
      GOTRUE_EXTERNAL_PHONE_ENABLED: 'false', GOTRUE_SMS_AUTOCONFIRM: 'false',
    }, {
      GOTRUE_DB_DATABASE_URL: database('supabase_auth_admin'), GOTRUE_JWT_SECRET: secrets.jwtSecret,
    }),
    rest: spec('rest', {
      PGRST_DB_SCHEMAS: 'public,graphql_public', PGRST_DB_MAX_ROWS: '1000', PGRST_DB_EXTRA_SEARCH_PATH: 'public',
      PGRST_DB_ANON_ROLE: 'anon', PGRST_SERVER_HOST: '127.0.0.1', PGRST_SERVER_PORT: '3000',
      PGRST_ADMIN_SERVER_HOST: '127.0.0.1', PGRST_ADMIN_SERVER_PORT: '3001', PGRST_DB_USE_LEGACY_GUCS: 'false',
      PGRST_APP_SETTINGS_JWT_EXP: jwtExpiry,
    }, {
      PGRST_DB_URI: database('authenticator'), PGRST_JWT_SECRET: secrets.jwtSecret,
    }, { command: ['postgrest'] }),
    // The stock start script drops to nobody with sudo, which no-new-privileges
    // forbids; run its three steps as nobody directly.
    realtime: spec('realtime', {
      PORT: '4000', DB_HOST: 'db', DB_PORT: '5432', DB_USER: 'supabase_admin', DB_NAME: 'postgres',
      DB_AFTER_CONNECT_QUERY: 'SET search_path TO _realtime', ERL_AFLAGS: '-proto_dist inet_tcp',
      DNS_NODES: "''", RLIMIT_NOFILE: '10000', APP_NAME: 'realtime', SEED_SELF_HOST: 'true', RUN_JANITOR: 'true',
      DISABLE_HEALTHCHECK_LOGGING: 'true', ERL_CRASH_DUMP: '/tmp/erl_crash.dump',
    }, {
      DB_PASSWORD: secrets.adminPassword, DB_ENC_KEY: secrets.realtimeEncryptionKey,
      API_JWT_SECRET: secrets.jwtSecret, METRICS_JWT_SECRET: secrets.jwtSecret,
      SECRET_KEY_BASE: secrets.realtimeSecretKeyBase,
    }, {
      args: ['--user', '65534:65534', '--entrypoint', '/bin/bash'],
      command: ['-ec', 'ulimit -Sn 10000; /app/bin/migrate; '
        + '/app/bin/realtime eval "Realtime.Release.seeds(Realtime.Repo)"; exec /app/bin/server'],
    }),
    // File objects live on the container's own anonymous volume.
    storage: spec('storage', {
      POSTGREST_URL: 'http://rest:3000', STORAGE_PUBLIC_URL: gateway, REQUEST_ALLOW_X_FORWARDED_PATH: 'true',
      FILE_SIZE_LIMIT: '52428800', STORAGE_BACKEND: 'file', GLOBAL_S3_BUCKET: 'stub',
      FILE_STORAGE_BACKEND_PATH: SUPABASE_STORAGE_ROOT, TENANT_ID: 'stub', REGION: 'stub',
      ENABLE_IMAGE_TRANSFORMATION: 'false', SERVER_PORT: '5000',
    }, {
      ANON_KEY: secrets.anonKey, SERVICE_KEY: secrets.serviceRoleKey, AUTH_JWT_SECRET: secrets.jwtSecret,
      DATABASE_URL: database('supabase_storage_admin'), S3_PROTOCOL_ACCESS_KEY_ID: secrets.s3AccessKeyId,
      S3_PROTOCOL_ACCESS_KEY_SECRET: secrets.s3AccessKeySecret,
    }, { args: ['-v', SUPABASE_STORAGE_ROOT] }),
    // The whole application directory is mounted read-only; the harness router
    // serves <app>/supabase/functions/<name>.
    functions: spec('functions', {
      SUPABASE_URL: gateway, SUPABASE_PUBLIC_URL: gateway, VERIFY_JWT: 'false',
    }, supabaseFunctionEnvironment(secrets), {
      args: ['-v', `${app}:/home/deno/app:ro`],
      command: ['start', '--main-service', '/home/deno/functions/main', '--port', '9000'],
      files: { '/home': { 'deno/functions/main/index.ts': supabaseAsset('edge-router.mts') } },
    }),
    // The stock entrypoint chowns stdout, which needs CAP_CHOWN; run Envoy as its own user.
    gateway: spec('gateway', {}, {}, {
      args: ['--user', '101:101', '--entrypoint', 'envoy'],
      command: ['-c', '/etc/envoy/envoy.yaml', '--concurrency', '2'],
      files: { '/etc/envoy': renderSupabaseGateway({ anonKey: secrets.anonKey,
        serviceRoleKey: secrets.serviceRoleKey, gatewayPort, ...(random ? { random } : {}) }) },
    }),
  };
}

export function supabaseDatabaseUrl(secrets: SupabaseSecrets): string {
  return `postgresql://postgres:${secrets.postgresPassword}@127.0.0.1:5432/postgres`;
}

// Edge Functions receive the stock server-side environment.
function supabaseFunctionEnvironment(secrets: SupabaseSecrets): Record<string, string> {
  return { SUPABASE_ANON_KEY: secrets.anonKey, SUPABASE_SERVICE_ROLE_KEY: secrets.serviceRoleKey,
    SUPABASE_DB_URL: supabaseDatabaseUrl(secrets), JWT_SECRET: secrets.jwtSecret };
}
