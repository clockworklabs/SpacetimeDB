import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { createBackendLease, readBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import type { BackendLease } from '../src/runtime/backend-lease.js';
import type { TextCommandExecutor } from '../src/runtime/command-executor.js';
import { SUPABASE_IDENTITY, SUPABASE_IMAGES } from '../src/stacks/backends/supabase-identity.js';
import { SUPABASE_RUNTIME, resetSupabase, startSupabasePlatform, supabaseOrchestratorConfig }
  from '../src/stacks/backends/supabase-lifecycle.js';
import { SUPABASE_CONTAINER_LIMITS, SUPABASE_SERVICE_ROLES, formatSupabaseSecrets, generateSupabaseSecrets,
  parseSupabaseSecrets, readSupabasePlatformSecrets, renderSupabaseGateway, supabaseAsset, supabaseGatewayUrl,
  supabaseServiceSpecs, supabaseSql } from '../src/stacks/backends/supabase-platform.js';

const ANCHOR = 'a'.repeat(64);
const STARTED = '2026-09-25T00:00:00Z';
const secrets = generateSupabaseSecrets();
const secretValues = Object.values(secrets);

function claims(token: string, secret: string): Record<string, unknown> {
  const [header, body, signature] = token.split('.');
  assert.equal(createHmac('sha256', secret).update(`${header}.${body}`).digest('base64url'), signature);
  return JSON.parse(Buffer.from(body!, 'base64url').toString('utf8'));
}

function activeLease(root: string): { path: string; lease: BackendLease } {
  const lease = createBackendLease({ runId: 'supabase-unit', backend: 'supabase', track: 'ecommerce', runIndex: 0,
    serverUri: 'http://127.0.0.1:13410', database: 'postgres' });
  lease.resources.container = { name: 'sb-unit-backend', id: ANCHOR, image: `sha256:${'d'.repeat(64)}`, owned: true,
    networkMode: 'b'.repeat(64) };
  lease.resources.network = { name: 'sb-unit-network', id: 'b'.repeat(64), namespaceContainerId: ANCHOR,
    namespaceStartedAt: STARTED, hostAddresses: ['172.20.0.1'], services: [], firewallSha256: 'f'.repeat(64),
    firewallInstalledAt: STARTED };
  const path = join(root, 'lease.json');
  writeBackendLease(path, lease);
  return { path, lease };
}

// Answers the anchor's namespace identity check; everything else is recorded.
function fakeExec(calls: { args: string[]; input?: string }[], answer: (args: readonly string[]) => string = () => ''):
  TextCommandExecutor {
  return (file, args, options) => {
    assert.equal(file, 'docker');
    calls.push({ args: [...args], ...(options.input === undefined ? {} : { input: options.input }) });
    if (args[0] === 'inspect' && args[2] === '{{json .}}') {
      return JSON.stringify({ Id: ANCHOR, State: { Running: true, StartedAt: STARTED } });
    }
    return answer(args);
  };
}

test('platform secrets are fresh HS256 keys that round-trip only in their own format', () => {
  assert.deepEqual(claims(secrets.anonKey, secrets.jwtSecret).role, 'anon');
  assert.deepEqual(claims(secrets.serviceRoleKey, secrets.jwtSecret).role, 'service_role');
  assert.equal(secrets.realtimeEncryptionKey.length, 16);
  assert.notEqual(secrets.adminPassword, secrets.postgresPassword);
  assert.notDeepEqual(generateSupabaseSecrets(), secrets);
  const text = formatSupabaseSecrets(secrets);
  assert.deepEqual(parseSupabaseSecrets(text), secrets);
  assert.throws(() => parseSupabaseSecrets(`${text}${text.split('\n')[0]}\n`), /malformed/);
  assert.throws(() => parseSupabaseSecrets(`${text}EXTRA=${'x'.repeat(20)}\n`), /malformed/);
  assert.throws(() => parseSupabaseSecrets(text.split('\n').slice(1).join('\n')), /lacks POSTGRES_PASSWORD/);
  assert.throws(() => formatSupabaseSecrets({ ...secrets, jwtSecret: "x'; drop" }), /JWT_SECRET is invalid/);
});

test('the gateway publishes only application services on the leased port', () => {
  const rendered = renderSupabaseGateway({ anonKey: secrets.anonKey, serviceRoleKey: secrets.serviceRoleKey,
    gatewayPort: 13417 });
  for (const text of Object.values(rendered)) assert.doesNotMatch(text, /\$\{[A-Z_]+\}|STRICT_DNS|cluster: (studio|meta)\b/);
  assert.match(rendered['lds.yaml'], /port_value: 13417/);
  assert(rendered['lds.yaml'].includes(secrets.anonKey) && rendered['lds.yaml'].includes(secrets.serviceRoleKey));
  assert.doesNotMatch(rendered['cds.yaml'], /name: (studio|meta)\n/);
  assert.deepEqual([...rendered['cds.yaml'].matchAll(/address: (\S+)/g)].map(match => match[1])
    .filter(address => address !== '127.0.0.1'), []);
  assert.match(rendered['envoy.yaml'], /path: \/etc\/envoy\/lds\.yaml/);
  assert.throws(() => renderSupabaseGateway({ anonKey: 'a', serviceRoleKey: 'b', gatewayPort: 70_000 }), /port/);
});

test('service specifications keep secrets out of arguments and apply the platform restrictions', () => {
  const specs = supabaseServiceSpecs({ gatewayPort: 13410, sitePort: 7823, app: '/work/app', secrets });
  assert.deepEqual(Object.keys(specs), [...SUPABASE_SERVICE_ROLES]);
  for (const [role, spec] of Object.entries(specs)) {
    const args = [...spec.args, ...spec.command].join(' ');
    for (const value of secretValues) assert(!args.includes(value), `${role} arguments hold a secret`);
    for (const name of Object.keys(spec.environment)) assert(spec.args.includes(name), `${role} passes ${name} by name`);
    assert.equal(spec.image, SUPABASE_IMAGES[role as keyof typeof SUPABASE_IMAGES]);
    assert.deepEqual(spec.limits, SUPABASE_CONTAINER_LIMITS[role as keyof typeof SUPABASE_CONTAINER_LIMITS]);
  }
  assert.equal(specs.realtime.environment.DB_PASSWORD, secrets.adminPassword);
  assert.equal(specs.auth.environment.GOTRUE_DB_DATABASE_URL, `postgres://supabase_auth_admin:${secrets.postgresPassword}@db:5432/postgres`);
  assert.deepEqual(specs.realtime.args.slice(0, 4), ['--user', '65534:65534', '--entrypoint', '/bin/bash']);
  assert.deepEqual(specs.gateway.args.slice(0, 4), ['--user', '101:101', '--entrypoint', 'envoy']);
  assert.deepEqual(specs.functions.args.slice(0, 2), ['-v', '/work/app:/home/deno/app:ro']);
  assert.match(specs.functions.files['/home']!['deno/functions/main/index.ts']!, /\/home\/deno\/app\/supabase\/functions\//);
  assert.equal(specs.functions.environment.SUPABASE_DB_URL, `postgresql://postgres:${secrets.postgresPassword}@127.0.0.1:5432/postgres`);
  assert.deepEqual(specs.storage.args.slice(0, 2), ['-v', '/var/lib/storage']);
  assert.match(specs.gateway.files['/etc/envoy']!['lds.yaml']!, /port_value: 13410/);
  assert.equal(SUPABASE_CONTAINER_LIMITS.db.memoryBytes, 1024 ** 3);
});

test('privileged SQL runs as supabase_admin over the exact anchor socket with the statement on stdin', () => {
  const root = mkdtempSync(join(tmpdir(), 'supabase-sql-'));
  try {
    const { lease } = activeLease(root);
    const calls: { args: string[]; input?: string }[] = [];
    assert.equal(supabaseSql(lease, 'select 1;', { exec: fakeExec(calls, () => '1\n') }), '1\n');
    assert.deepEqual(calls[1], { args: ['exec', '-i', '--user', 'postgres', ANCHOR, 'psql', '-h', '/var/run/postgresql',
      '-U', 'supabase_admin', '-d', 'postgres', '-v', 'ON_ERROR_STOP=1', '-X', '-q', '-At'], input: 'select 1;' });
    const replaced: TextCommandExecutor = () => JSON.stringify({ Id: 'c'.repeat(64), State: { Running: true, StartedAt: STARTED } });
    assert.throws(() => supabaseSql(lease, 'select 1;', { exec: replaced }), /changed after lease creation/);
    const restarted: TextCommandExecutor = () => JSON.stringify({ Id: ANCHOR, State: { Running: true, StartedAt: 'later' } });
    assert.throws(() => readSupabasePlatformSecrets(lease, restarted), /stopped or restarted/);
    assert.deepEqual(readSupabasePlatformSecrets(lease, fakeExec([], () => formatSupabaseSecrets(secrets))), secrets);
    assert.equal(supabaseGatewayUrl(lease), 'http://127.0.0.1:13410');
    assert.throws(() => supabaseSql({ ...lease, backend: 'postgres' }, 'select 1;'), /not a Supabase lease/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('the platform starts in order, reaches readiness through the gateway, then records its reset baseline', () => {
  const root = mkdtempSync(join(tmpdir(), 'supabase-start-'));
  try {
    const { path, lease } = activeLease(root);
    const calls: { args: string[]; input?: string; environment?: Record<string, string> }[] = [];
    const ids = new Map(SUPABASE_SERVICE_ROLES.map((role, index) => [role, String(index + 1).repeat(64)]));
    const probes: string[] = [];
    const docker = (args: string[], input?: string, environment?: Record<string, string>) => {
      calls.push({ args, ...(input === undefined ? {} : { input }), ...(environment ? { environment } : {}) });
      if (args[0] === 'inspect' && args[2] === '{{json .}}') {
        return JSON.stringify({ Id: ANCHOR, State: { Running: true, StartedAt: STARTED } });
      }
      if (args[0] === 'image') return `sha256:${'e'.repeat(64)}`;
      if (args[0] === 'create') return ids.get(/service-([a-z]+)$/.exec(args[args.indexOf('--name') + 1]!)![1] as never)!;
      if (args.includes('psql') && input?.includes('supabase_realtime_messages_publication')) return '1\n';
      return '';
    };
    startSupabasePlatform({ leasePath: path, leaseToken: lease.ownershipToken, app: '/work/app', secrets,
      ports: { vite: 7823, express: 7901, dbPort: null }, docker, sleep: () => undefined, probes: {
        http: (url, headers) => { probes.push(`${url} ${headers.apikey === secrets.serviceRoleKey}`); return 200; },
        realtimeJoin: (url, key) => { probes.push(`join ${url} ${key === secrets.anonKey}`); return true; },
      } });
    for (const call of calls) {
      for (const value of secretValues) assert(!call.args.join(' ').includes(value), `secret in ${call.args.join(' ')}`);
    }
    const kind = (call: typeof calls[number]) => call.args[0] === 'exec' && call.args[1] === '-d' ? 'start-db'
      : call.args.includes('pg_isready') ? 'ready-db'
        : call.args.includes('psql') ? `sql:${call.input?.includes('stackbench_reset') ? 'baseline'
          : call.input?.includes('alter role supabase_admin') ? 'admin' : 'query'}`
          : call.args[0] === 'exec' && call.args.at(-1)?.startsWith('/') ? `write:${call.args.at(-1)}`
            : call.args[0] === 'create' ? `create:${/service-([a-z]+)$/.exec(call.args[call.args.indexOf('--name') + 1]!)![1]}`
              : call.args[0] === 'start' ? 'start' : call.args[0] === 'cp' ? 'cp' : null;
    const order = calls.map(kind).filter(Boolean);
    assert.deepEqual(order.slice(0, 11), ['write:/run/stack-bench/supabase.env', 'write:/etc/postgresql/pg_hba.conf',
      'write:/docker-entrypoint-initdb.d/init-scripts/98-webhooks.sql', 'write:/docker-entrypoint-initdb.d/init-scripts/99-roles.sql',
      'write:/docker-entrypoint-initdb.d/init-scripts/99-jwt.sql', 'write:/docker-entrypoint-initdb.d/migrations/97-_supabase.sql',
      'write:/docker-entrypoint-initdb.d/migrations/99-realtime.sql', 'write:/docker-entrypoint-initdb.d/migrations/99-logs.sql',
      'start-db', 'ready-db', 'sql:admin']);
    assert.deepEqual(order.filter(step => step?.startsWith('create:')), SUPABASE_SERVICE_ROLES.map(role => `create:${role}`));
    assert.equal(order.at(-1), 'sql:baseline');
    const secretFile = calls.find(call => kind(call) === 'write:/run/stack-bench/supabase.env')!;
    assert(secretFile.input === formatSupabaseSecrets(secrets), 'secrets reach the anchor on stdin');
    assert.match(secretFile.args.join(' '), /umask 077/);
    const hba = calls.find(call => kind(call) === 'write:/etc/postgresql/pg_hba.conf')!.input!;
    assert.match(hba, /host {2}all {2}all {2}127\.0\.0\.1\/32 {2}scram-sha-256/);
    assert.doesNotMatch(hba, /127\.0\.0\.1\/32\s+trust/);
    const auth = calls.find(call => kind(call) === 'create:auth')!;
    assert.equal(auth.environment?.GOTRUE_JWT_SECRET, secrets.jwtSecret);
    assert.equal(auth.args[auth.args.indexOf('--network') + 1], `container:${ANCHOR}`);
    assert.deepEqual(probes, ['/auth/v1/health', '/rest/v1/', '/storage/v1/version', '/functions/v1/_health']
      .map(route => `http://127.0.0.1:13410${route} true`).concat('join http://127.0.0.1:13410 true'));
    const services = readBackendLease(path).resources.serviceContainers!;
    assert.deepEqual(Object.keys(services), [...SUPABASE_SERVICE_ROLES]);
    assert.equal(services.gateway!.id, ids.get('gateway'));
    const baseline = calls.find(call => kind(call) === 'sql:baseline')!.input!;
    assert(baseline.startsWith('begin;\n') && baseline.trimEnd().endsWith('commit;'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reset empties data through the baseline function and storage files, only on the exact storage service', () => {
  const root = mkdtempSync(join(tmpdir(), 'supabase-reset-'));
  try {
    const { path } = activeLease(root);
    const lease = readBackendLease(path);
    lease.state = 'active';
    assert.throws(() => resetSupabase({ lease, exec: fakeExec([]) }), /owned storage service/);
    const storage = { name: 'sb-unit-service-storage', id: '5'.repeat(64),
      image: `sha256:${'e'.repeat(64)}`, owned: true, networkMode: `container:${ANCHOR}` };
    lease.resources.serviceContainers = { storage };
    const calls: { args: string[]; input?: string }[] = [];
    resetSupabase({ lease, exec: fakeExec(calls, args => args[0] === 'inspect' ? storage.id : '') });
    assert.deepEqual(calls.map(call => call.input ?? call.args.slice(0, 2).join(' ')),
      ['inspect --format', 'inspect --format', 'select stackbench_reset.reset();\n', `exec ${storage.id}`]);
    assert.match(calls.at(-1)!.args.at(-1)!, /find \/var\/lib\/storage -mindepth 1 -maxdepth 1 -exec rm -rf/);
    const replaced: { args: string[]; input?: string }[] = [];
    assert.throws(() => resetSupabase({ lease, exec: fakeExec(replaced, () => 'other') }), /changed after lease creation/);
    assert(!replaced.some(call => call.input?.includes('reset()')), 'no data is reset for a replaced storage service');
    assert.throws(() => resetSupabase({ lease: { ...lease, state: 'released' } }), /active lease/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('the reset baseline leaves extension objects to their extension and restores Realtime publications', () => {
  const baseline = supabaseAsset('reset/baseline.sql');
  const namespaceLoop = /select nspname from pg_namespace n where[\s\S]*?loop/.exec(baseline)?.[0];
  assert.match(namespaceLoop ?? '', /not_extension_member, 'pg_%', 'namespace', 'n\.oid', 'pg_namespace', 'n\.oid'/);
  for (const catalog of ['pg_namespace', 'pg_class', 'pg_policy', 'pg_trigger', 'pg_publication_rel', 'pg_publication',
    'pg_event_trigger']) assert(baseline.includes(`'${catalog}'`), catalog);
  assert.match(baseline, /x\.deptype = ''e''/);
  assert.match(baseline, /create publication supabase_realtime;\n\s+alter publication supabase_realtime owner to postgres;/);
  assert.match(baseline, /alter publication supabase_realtime add table stackbench_reset\.realtime_canary;\n\s+insert into stackbench_reset\.baseline/);
  assert(!/^\s*(begin|commit)\s*;/im.test(baseline), 'activation wraps the baseline in its own transaction');
});

test('Supabase identity, orchestration and crash boundary are its own', () => {
  assert.deepEqual(SUPABASE_IDENTITY.ports, { vite: 7823, express: 7901 });
  assert.equal(supabaseOrchestratorConfig({ env: {} }).lease.serverUri, 'http://127.0.0.1:13410');
  assert.deepEqual(supabaseOrchestratorConfig({ env: { STACK_BENCH_SUPABASE_URI: 'http://127.0.0.1:14000' } }).environment,
    { STACK_BENCH_SUPABASE_URI: 'http://127.0.0.1:14000' });
  assert.throws(() => supabaseOrchestratorConfig({ env: { STACK_BENCH_SUPABASE_URI: 'http://example.com:80' } }), /loopback/);
  const helpers = { requireString: (value: unknown) => value, loopbackHttpUri: (value: unknown) => new URL(String(value)) };
  assert.throws(() => SUPABASE_IDENTITY.lease.validateResources({ helpers,
    resources: { serverUri: 'http://127.0.0.1:13410', database: 'app', container: null } }), /postgres database/);
  assert.equal(SUPABASE_RUNTIME.combinedBoundary, false);
  assert.equal(SUPABASE_RUNTIME.databaseUser, 'postgres');
  assert.equal(SUPABASE_RUNTIME.processRecord, '/run/application/restart-supabase-db.pid');
  const drain = SUPABASE_RUNTIME.drainCommand!({} as BackendLease, 'postgres');
  assert.deepEqual(drain.slice(0, 5), ['psql', '-h', '/var/run/postgresql', '-U', 'supabase_admin']);
  assert.match(drain.at(-1)!, /usename IN \('postgres','authenticator'\) AND state<>'idle'/);
});
