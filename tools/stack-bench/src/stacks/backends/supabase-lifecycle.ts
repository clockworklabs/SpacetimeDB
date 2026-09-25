import { execFileSync } from 'node:child_process';
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { backendResourceLockKeys, loopbackHttpUri, readBackendLease, updateBackendLease,
  verifyBackendResourceClaims } from '../../runtime/backend-lease.js';
import type { BackendLease } from '../../runtime/backend-lease.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';
import { CODING_CONTAINER_CONTROL_DIR } from '../../runtime/coding-container-policy.js';
import { attemptDocker, createAttemptBrowser, createAttemptContainer, createAttemptNetwork, createAttemptService,
  installAttemptFirewall, requireAttemptNetwork, startAttemptContainer } from '../../runtime/docker-network.js';
import { sleepSync } from '../../runtime/platform.js';
import { loadTrack, portsFor } from '../../composition/tracks.js';
import type { StackDatabaseRuntime } from '../process-crash.js';
import type { StackLifecycleInput, StackRunPorts } from '../stack-adapter-contract.js';
import { controlHostedAppServer } from '../hosted-lifecycle.js';
import { DEFAULT_SUPABASE_GATEWAY_URI, SUPABASE_IMAGES } from './supabase-identity.js';
import { SUPABASE_CONTAINER_LIMITS, SUPABASE_DB_PROCESS_RECORD, SUPABASE_DB_SOCKET, SUPABASE_HOSTS,
  SUPABASE_JWT_EXPIRY_SECONDS, SUPABASE_PSQL, SUPABASE_SECRET_FILE, SUPABASE_SERVICE_ROLES,
  SUPABASE_STORAGE_ROOT, formatSupabaseSecrets, generateSupabaseSecrets, supabaseAnchor, supabaseAsset,
  readSupabasePlatformSecrets, supabaseDatabaseUrl, supabaseGatewayUrl, supabasePsqlArguments, supabaseServiceSpecs,
  supabaseSql } from './supabase-platform.js';
import type { SupabasePlatformSecrets } from './supabase-platform.js';

type Docker = typeof attemptDocker;

// Checks the controller makes through the published gateway. Keys travel on
// stdin, never in a probe's arguments.
export interface SupabaseProbes {
  // The HTTP status of a loopback URL; null when nothing answers.
  http(url: string, headers: Record<string, string>): number | null;
  // Joins a Realtime channel, which also starts the tenant's replication.
  realtimeJoin(gatewayUrl: string, anonKey: string): boolean;
}

interface SupabaseLifecycleInput { leasePath: string; leaseToken: string; ports: StackRunPorts }

// Database init scripts at the paths the pinned Compose file mounts them.
const INIT_SCRIPTS = Object.freeze({
  'init-scripts/98-webhooks.sql': 'webhooks.sql',
  'init-scripts/99-roles.sql': 'roles.sql',
  'init-scripts/99-jwt.sql': 'jwt.sql',
  'migrations/97-_supabase.sql': '_supabase.sql',
  'migrations/99-realtime.sql': 'realtime.sql',
  'migrations/99-logs.sql': 'logs.sql',
});

// Every client shares the attempt namespace, so loopback is the only network
// path. The image trusts loopback TCP; here only the socket inside the anchor
// is trusted, for the harness's own supabase_admin commands.
const PG_HBA = [
  'local all  supabase_admin     trust',
  'local all  all                peer map=supabase_map',
  'host  all  all  127.0.0.1/32  scram-sha-256',
  'host  all  all  ::1/128       scram-sha-256',
  '',
].join('\n');

const HTTP_PROBE = 'let s="";process.stdin.on("data",c=>s+=c).on("end",async()=>{const{url,headers}=JSON.parse(s);'
  + 'try{const r=await fetch(url,{headers,signal:AbortSignal.timeout(5000)});process.stdout.write(String(r.status))}'
  + 'catch{process.stdout.write("0")}})';
const REALTIME_JOIN = 'let s="";process.stdin.on("data",c=>s+=c).on("end",()=>{const{url,key}=JSON.parse(s);'
  + 'const ws=new WebSocket(url.replace(/^http/,"ws")+"/realtime/v1/websocket?vsn=1.0.0&apikey="+key);'
  + 'setTimeout(()=>process.exit(2),10000);ws.onerror=()=>process.exit(3);'
  + 'ws.onopen=()=>ws.send(JSON.stringify({topic:"realtime:stack-bench-ready",event:"phx_join",ref:"1",join_ref:"1",'
  + 'payload:{config:{broadcast:{self:false},presence:{key:""},postgres_changes:[]},access_token:key}}));'
  + 'ws.onmessage=e=>{const m=JSON.parse(e.data);if(m.event==="phx_reply")process.exit(m.payload.status==="ok"?0:1)}})';

const node = (script: string, input: unknown): string => execFileSync(process.execPath, ['-e', script],
  { encoding: 'utf8', stdio: 'pipe', input: JSON.stringify(input), timeout: 15_000, windowsHide: true });

const CONTROLLER_PROBES: SupabaseProbes = {
  http(url, headers) {
    try {
      const status = Number(node(HTTP_PROBE, { url, headers }).trim());
      return Number.isInteger(status) && status > 0 ? status : null;
    } catch { return null; }
  },
  realtimeJoin: (url, key) => succeeds(() => node(REALTIME_JOIN, { url, key })),
};

function claimedPorts(lease: BackendLease, ports: StackRunPorts): number[] {
  if (ports.express === null || ports.dbPort !== null) {
    throw new Error('Supabase requires frontend and application server ports and no database port');
  }
  const endpoints = [ports.vite, ports.express, Number(new URL(supabaseGatewayUrl(lease)).port)];
  if (new Set(endpoints).size !== endpoints.length) throw new Error('Supabase endpoint ports must differ');
  verifyBackendResourceClaims(lease, backendResourceLockKeys(lease, ports));
  return endpoints;
}

function waitUntil(check: () => boolean, timeoutMs: number, description: string, sleep = sleepSync): void {
  const deadline = Date.now() + timeoutMs;
  while (!check()) {
    if (Date.now() >= deadline) throw new Error(`timed out waiting for ${description}`);
    sleep(250);
  }
}

function succeeds(run: () => unknown): boolean {
  try { run(); return true; } catch { return false; }
}

function writeInto(docker: Docker, containerId: string, path: string, content: string, umask = '022'): void {
  if (!/^\/[A-Za-z0-9_./-]+$/.test(path) || path.split('/').includes('..')) throw new Error(`unsafe container path ${path}`);
  docker(['exec', '-i', containerId, 'sh', '-ec', `umask ${umask}; mkdir -p "$(dirname "$1")"; cat > "$1"`,
    'sh', path], content);
}

// The image's own entrypoint initializes an empty data directory on first start
// and drops to the postgres user. The recorded group is what a crash kills.
export function startSupabaseDatabase(lease: BackendLease, { docker = attemptDocker, timeoutMs = 120_000,
  sleep = sleepSync }: { docker?: Docker; timeoutMs?: number; sleep?: (ms: number) => void } = {}): void {
  const anchor = lease.resources.container;
  if (!anchor?.owned) throw new Error('Supabase lease has no owned platform container');
  docker(['exec', '-d', anchor.id, 'bash', '-ec',
    `mkdir -p -m 755 ${CODING_CONTAINER_CONTROL_DIR}; `
    + `POSTGRES_PASSWORD=$(sed -n 's/^POSTGRES_PASSWORD=//p' ${SUPABASE_SECRET_FILE}); export POSTGRES_PASSWORD; `
    + `exec setsid bash -ec 'stat=$(cat /proc/$$/stat); rest=\${stat##*) }; set -- $rest; `
    + `printf "%s %s\\n" "$$" "\${20}" > ${SUPABASE_DB_PROCESS_RECORD}; `
    + 'exec docker-entrypoint.sh postgres -c config_file=/etc/postgresql/postgresql.conf -c log_min_messages=fatal'
    + `' > ${CODING_CONTAINER_CONTROL_DIR}/stack-bench-backend.log 2>&1`]);
  // TCP answers only after first-start initialization; its temporary server uses the socket.
  waitUntil(() => succeeds(() => docker(['exec', anchor.id, 'pg_isready', '-h', '127.0.0.1', '-p', '5432',
    '-U', 'postgres'])), timeoutMs, 'the Supabase database', sleep);
}

function installDatabase(lease: BackendLease, secrets: SupabasePlatformSecrets, docker: Docker): void {
  const anchor = lease.resources.container!.id;
  writeInto(docker, anchor, SUPABASE_SECRET_FILE, formatSupabaseSecrets(secrets), '077');
  writeInto(docker, anchor, '/etc/postgresql/pg_hba.conf', PG_HBA);
  for (const [target, asset] of Object.entries(INIT_SCRIPTS)) {
    writeInto(docker, anchor, `/docker-entrypoint-initdb.d/${target}`, supabaseAsset(`db/${asset}`));
  }
}

// supabase_admin is the platform superuser. Only Realtime and the harness use
// it, so it gets its own password rather than the one the application receives.
function separateAdminPassword(lease: BackendLease, secrets: SupabasePlatformSecrets, docker: Docker): void {
  docker(supabasePsqlArguments(lease.resources.container!.id),
    `alter role supabase_admin with password '${secrets.adminPassword}';\n`);
}

function copyFiles(docker: Docker, containerId: string, files: Record<string, Record<string, string>>): void {
  for (const [directory, entries] of Object.entries(files)) {
    const staging = mkdtempSync(join(tmpdir(), 'stack-bench-supabase-'));
    try {
      for (const [path, content] of Object.entries(entries)) {
        const target = resolve(staging, path);
        const inside = relative(staging, target);
        if (!inside || inside.startsWith('..') || isAbsolute(inside)) throw new Error(`unsafe service file ${path}`);
        mkdirSync(dirname(target), { recursive: true, mode: 0o755 });
        writeFileSync(target, content, { mode: 0o644 });
        chmodSync(target, 0o644);
      }
      docker(['cp', `${staging}/.`, `${containerId}:${directory}`]);
    } finally { rmSync(staging, { recursive: true, force: true }); }
  }
}

// Readiness through the published gateway, as the application reaches it. A
// first Realtime join creates Realtime's own publication and replication slot,
// which the reset baseline must then record as platform objects.
export function waitForSupabaseGateway(lease: BackendLease, secrets: SupabasePlatformSecrets, exec: TextCommandExecutor,
  { probes = CONTROLLER_PROBES, timeoutMs = 180_000, sleep = sleepSync }: {
    probes?: SupabaseProbes; timeoutMs?: number; sleep?: (ms: number) => void;
  } = {}): void {
  const gateway = supabaseGatewayUrl(lease);
  // The service key also passes the gateway's rule that reserves /rest/v1/ itself.
  const headers = { apikey: secrets.serviceRoleKey, Authorization: `Bearer ${secrets.serviceRoleKey}` };
  for (const path of ['/auth/v1/health', '/rest/v1/', '/storage/v1/version', '/functions/v1/_health']) {
    waitUntil(() => probes.http(`${gateway}${path}`, headers) === 200, timeoutMs, `Supabase ${path} through the gateway`, sleep);
  }
  waitUntil(() => probes.realtimeJoin(gateway, secrets.anonKey), timeoutMs, 'a Supabase Realtime channel join', sleep);
  waitUntil(() => supabaseSql(lease,
    "select count(*) from pg_publication where pubname = 'supabase_realtime_messages_publication';\n",
    { exec }).trim() === '1', timeoutMs, 'Supabase Realtime replication', sleep);
}

// Starts the platform inside an isolated attempt namespace whose anchor is
// running: database, services, readiness, then the reset baseline.
export function startSupabasePlatform({ leasePath, leaseToken, ports, app, docker = attemptDocker,
  probes = CONTROLLER_PROBES, sleep = sleepSync, secrets = generateSupabaseSecrets() }: SupabaseLifecycleInput & {
  app: string; docker?: Docker; probes?: SupabaseProbes; sleep?: (ms: number) => void; secrets?: SupabasePlatformSecrets;
}): BackendLease {
  const exec: TextCommandExecutor = (file, args, options) => {
    if (file !== 'docker') throw new Error(`unexpected command ${file}`);
    return docker([...args], options.input);
  };
  let lease = readBackendLease(leasePath, { token: leaseToken, backend: 'supabase' });
  supabaseAnchor(lease, exec);
  installDatabase(lease, secrets, docker);
  startSupabaseDatabase(lease, { docker, sleep });
  separateAdminPassword(lease, secrets, docker);
  const specs = supabaseServiceSpecs({ gatewayPort: Number(new URL(supabaseGatewayUrl(lease)).port),
    sitePort: ports.vite, app, secrets });
  for (const role of SUPABASE_SERVICE_ROLES) {
    const spec = specs[role];
    const image = docker(['image', 'inspect', '--format', '{{.Id}}', spec.image]);
    const container = createAttemptService(leasePath, readBackendLease(leasePath, { token: leaseToken }), role, image,
      { args: spec.args, command: spec.command, limits: spec.limits, environment: spec.environment, docker });
    copyFiles(docker, container.id, spec.files);
    startAttemptContainer(container.id, `${role} service`, docker);
  }
  lease = readBackendLease(leasePath, { token: leaseToken });
  waitForSupabaseGateway(lease, secrets, exec, { probes, sleep });
  // Recorded after every service has migrated, so reset() keeps their objects.
  supabaseSql(lease, `begin;\n${supabaseAsset('reset/baseline.sql')}\ncommit;\n`, { exec, timeoutMs: 120_000 });
  return lease;
}

export function activateSupabase({ leasePath, leaseToken, ports, app }: SupabaseLifecycleInput & { app?: string }): void {
  if (process.platform !== 'linux') throw new Error('Supabase activation requires the Linux Docker controller');
  if (!app) throw new Error('Supabase activation requires the application directory');
  const lease = readBackendLease(leasePath, { token: leaseToken, backend: 'supabase' });
  if (lease.state !== 'created' || lease.resources.container || lease.resources.creationIntents) {
    throw new Error('Supabase activation requires a fresh lease');
  }
  const endpoints = claimedPorts(lease, ports);
  // Edge Functions mount this directory; Docker would otherwise create it as root.
  mkdirSync(resolve(app), { recursive: true });
  updateBackendLease(leasePath, { token: leaseToken }, next => { next.state = 'starting'; return next; });
  const image = attemptDocker(['image', 'inspect', '--format', '{{.Id}}', SUPABASE_IMAGES.db]);
  const current = createAttemptNetwork(leasePath, lease);
  createAttemptContainer(leasePath, current, 'backend', image, current.resources.network!.id, [
    ...endpoints.flatMap(port => ['--publish', `127.0.0.1:${port}:${port}`]),
    ...SUPABASE_HOSTS.flatMap(host => ['--add-host', `${host}:127.0.0.1`]),
    // The image's entrypoint prepares its directories and drops to postgres.
    ...['CHOWN', 'DAC_OVERRIDE', 'FOWNER', 'SETGID', 'SETUID'].flatMap(capability => ['--cap-add', capability]),
    '-v', '/var/lib/postgresql/data',
    '-e', `POSTGRES_HOST=${SUPABASE_DB_SOCKET}`, '-e', 'PGPORT=5432', '-e', 'POSTGRES_PORT=5432',
    '-e', 'POSTGRES_DB=postgres', '-e', 'PGDATABASE=postgres', '-e', `JWT_EXP=${SUPABASE_JWT_EXPIRY_SECONDS}`,
  ], attemptDocker, SUPABASE_CONTAINER_LIMITS.db);
  installAttemptFirewall(leasePath, readBackendLease(leasePath, { token: leaseToken }));
  startSupabasePlatform({ leasePath, leaseToken, ports, app: resolve(app) });
  createAttemptBrowser(leasePath, readBackendLease(leasePath, { token: leaseToken }));
  updateBackendLease(leasePath, { token: leaseToken }, next => { next.state = 'active'; return next; });
}

function requireAnchorIdentity(lease: BackendLease, docker: Docker): void {
  const anchor = lease.resources.container!;
  const inspected = JSON.parse(docker(['inspect', anchor.name]))[0];
  if (inspected.Id !== anchor.id || inspected.Image !== anchor.image
    || !['', 'private'].includes(inspected.HostConfig.PidMode)) throw new Error('Supabase container identity changed');
}

// A crash must leave a valid record and no live process at that PID. Never
// adopt a new process or erase data to make recovery succeed.
export function recoverSupabase({ leasePath, leaseToken, ports, signal, docker = attemptDocker }: SupabaseLifecycleInput & {
  signal?: AbortSignal; docker?: Docker;
}): void {
  signal?.throwIfAborted();
  const lease = readBackendLease(leasePath, { token: leaseToken, backend: 'supabase', active: true });
  claimedPorts(lease, ports);
  requireAttemptNetwork(lease);
  requireAnchorIdentity(lease, docker);
  docker(['exec', lease.resources.container!.id, 'sh', '-ec',
    `read pid started < ${SUPABASE_DB_PROCESS_RECORD}; `
    + 'case "$pid:$started" in *[!0-9:]*) exit 4;; esac; '
    + '[ "$pid" -gt 1 ] && [ "$started" -gt 0 ] && [ ! -e /proc/$pid ]']);
  updateBackendLease(leasePath, { token: leaseToken }, next => { next.state = 'restarting'; return next; });
  startSupabaseDatabase(lease, { docker, timeoutMs: 30_000 });
  updateBackendLease(leasePath, { token: leaseToken }, next => { next.state = 'active'; return next; });
  signal?.throwIfAborted();
}

// Data-level reset with every service running. The application is stopped
// first; its next start applies its migrations again.
export function resetSupabase({ lease, exec = execFileSync }: { lease: BackendLease; exec?: TextCommandExecutor }): void {
  if (!['active', 'restarting'].includes(lease.state)) throw new Error('Supabase reset requires an active lease');
  const storage = lease.resources.serviceContainers?.storage;
  const anchor = lease.resources.container;
  if (!storage?.owned || !anchor || storage.networkMode !== `container:${anchor.id}`) {
    throw new Error('Supabase reset requires the owned storage service');
  }
  const actual = exec('docker', ['inspect', '--format', '{{.Id}}', storage.name],
    { encoding: 'utf8', stdio: 'pipe', timeout: 30_000 }).trim();
  if (actual !== storage.id) throw new Error(`${storage.name} changed after lease creation; refusing reset`);
  supabaseSql(lease, 'select stackbench_reset.reset();\n', { exec, timeoutMs: 120_000 });
  exec('docker', ['exec', storage.id, 'sh', '-ec', `test "$(readlink -f ${SUPABASE_STORAGE_ROOT})" = ${SUPABASE_STORAGE_ROOT}; `
    + `find ${SUPABASE_STORAGE_ROOT} -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +`],
  { encoding: 'utf8', stdio: 'pipe', timeout: 60_000 });
}

export function supabaseApplicationEnvironment(lease: BackendLease): Record<string, string> {
  const secrets = readSupabasePlatformSecrets(lease);
  const url = supabaseGatewayUrl(lease);
  return {
    SUPABASE_URL: url,
    SUPABASE_ANON_KEY: secrets.anonKey,
    SUPABASE_SERVICE_ROLE_KEY: secrets.serviceRoleKey,
    SUPABASE_DB_URL: supabaseDatabaseUrl(secrets),
    VITE_SUPABASE_URL: url,
    VITE_SUPABASE_ANON_KEY: secrets.anonKey,
  };
}

// Runtime control restarts the application server, as for PostgreSQL.
export function controlSupabaseApplication(input: StackLifecycleInput): Promise<void> {
  return controlHostedAppServer({
    adapterId: input.adapterId, app: input.app, port: input.port, probe: input.probe, mode: input.mode,
    signal: input.signal, exec: input.exec, lease: input.lease,
    environment: { ...supabaseApplicationEnvironment(input.lease), APP_WARM_START: '1', VITE_PORT: String(input.port) },
  });
}

export function supabaseOrchestratorConfig({ env }: { env: NodeJS.ProcessEnv }) {
  const serverUri = env.STACK_BENCH_SUPABASE_URI ?? DEFAULT_SUPABASE_GATEWAY_URI;
  loopbackHttpUri(serverUri);
  return { environment: { STACK_BENCH_SUPABASE_URI: serverUri }, lease: { serverUri },
    lifecycle: {}, windowsEnvironmentBridge: ['STACK_BENCH_SUPABASE_URI'] };
}

// Realtime drops its sockets when the database restarts, but the application
// server is a separate process: not a combined boundary.
export const SUPABASE_RUNTIME: StackDatabaseRuntime = {
  combinedBoundary: false,
  databaseUser: 'postgres',
  processRecord: SUPABASE_DB_PROCESS_RECORD,
  recoverDatabase: ({ leasePath, lease, signal }) => recoverSupabase({ leasePath, leaseToken: lease.ownershipToken,
    ports: portsFor(loadTrack(lease.track), 'supabase', lease.runIndex), signal }),
  async databaseReady(lease) {
    return succeeds(() => attemptDocker(['exec', lease.resources.container!.id, 'pg_isready', '-h', '127.0.0.1',
      '-p', '5432', '-U', 'postgres']));
  },
  // Work the application's own database connections and REST requests still hold.
  // Platform services keep idle pooled connections; they are not pending work.
  drainCommand: () => [...SUPABASE_PSQL, '-c',
    "SELECT count(*) FROM pg_stat_activity WHERE backend_type='client backend' AND pid<>pg_backend_pid() "
    + "AND usename IN ('postgres','authenticator') AND state<>'idle'"],
};
