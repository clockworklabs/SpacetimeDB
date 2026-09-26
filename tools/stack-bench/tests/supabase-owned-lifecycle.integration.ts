import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { backendResourceLockKeys, claimBackendResources, createBackendLease, publicBackendLease,
  readBackendLease, resourceLockScope } from '../src/runtime/backend-lease.js';
import { attemptDocker, requireAttemptNetwork } from '../src/runtime/docker-network.js';
import { releaseBackendLease } from '../src/runtime/backend-teardown.js';
import { prepareProcessCrash } from '../src/stacks/process-crash.js';
import { activateSupabase, recoverSupabase, resetSupabase, supabaseApplicationEnvironment }
  from '../src/stacks/backends/supabase-lifecycle.js';
import { SUPABASE_SERVICE_ROLES, readSupabasePlatformSecrets, supabaseSql } from '../src/stacks/backends/supabase-platform.js';

// Run inside the Linux controller with its normal lock directory and Docker socket,
// with the pinned Supabase images present. No paid calls or registry entry.
test('Supabase owned platform: activation, gateway readiness, secrets, reset, crash recovery and exact cleanup', {
  skip: process.env.STACK_BENCH_SUPABASE_OWNED_TEST !== '1', timeout: 600_000,
}, async () => {
  assert.equal(process.platform, 'linux');
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-supabase-owned-'));
  const path = join(root, 'lease.json');
  const app = join(root, 'app');
  mkdirSync(join(app, 'supabase', 'functions', 'hello'), { recursive: true });
  writeFileSync(join(app, 'supabase', 'functions', 'hello', 'index.ts'),
    "Deno.serve(() => Response.json({ url: Deno.env.get('SUPABASE_URL') }));\n");
  const ports = { vite: 14409, express: 14411, dbPort: null };
  const lease = createBackendLease({ runId: root.split('/').at(-1)!, backend: 'supabase', track: 'ecommerce',
    runIndex: 0, serverUri: 'http://127.0.0.1:14410', database: 'postgres' });
  const evidenceDirectory = process.env.STACK_BENCH_SUPABASE_EVIDENCE_DIR;
  const evidence: Record<string, unknown> = { result: 'running', ports };
  const save = () => { if (evidenceDirectory) writeFileSync(join(evidenceDirectory, 'supabase-owned-lifecycle.json'),
    JSON.stringify(evidence, null, 2)); };
  const owned: string[] = [];
  let volumes: string[] = [];
  let network: string | undefined;
  let failure: unknown;
  try {
    claimBackendResources(path, lease, { ...resourceLockScope(), keys: backendResourceLockKeys(lease, ports) });
    const started = Date.now();
    activateSupabase({ leasePath: path, leaseToken: lease.ownershipToken, ports, app });
    evidence.activationMs = Date.now() - started;
    const active = readBackendLease(path, { token: lease.ownershipToken, active: true });
    evidence.activeLease = publicBackendLease(active);
    requireAttemptNetwork(active);
    const anchor = active.resources.container!;
    network = active.resources.network!.id;
    const services = active.resources.serviceContainers!;
    assert.deepEqual(Object.keys(services).sort(), [...SUPABASE_SERVICE_ROLES].sort());
    for (const container of [anchor, ...Object.values(services), active.resources.browserContainer!]) owned.push(container.id);
    for (const service of Object.values(services)) assert.equal(service.networkMode, `container:${anchor.id}`);
    volumes = [anchor.id, services.storage!.id].flatMap(id => JSON.parse(attemptDocker(['inspect', '--format',
      '{{json .Mounts}}', id])).filter((mount: { Type: string }) => mount.Type === 'volume')
      .map((mount: { Name: string }) => mount.Name));
    assert.equal(volumes.length, 2, 'database and storage data are container-owned anonymous volumes');

    // Every service answers through the published gateway as the application reaches it.
    const secrets = readSupabasePlatformSecrets(active);
    const environment = supabaseApplicationEnvironment(active);
    assert.equal(environment.SUPABASE_URL, 'http://127.0.0.1:14410');
    const service = { apikey: secrets.serviceRoleKey, Authorization: `Bearer ${secrets.serviceRoleKey}` };
    const anon = { apikey: secrets.anonKey, 'content-type': 'application/json' };
    for (const route of ['/auth/v1/health', '/rest/v1/', '/storage/v1/version', '/functions/v1/_health']) {
      assert.equal((await fetch(`${environment.SUPABASE_URL}${route}`, { headers: service })).status, 200, route);
    }
    const hello = await fetch(`${environment.SUPABASE_URL}/functions/v1/hello`, { headers: service });
    assert.deepEqual(await hello.json(), { url: environment.SUPABASE_URL });
    for (const route of ['/', '/pg/', '/api/mcp']) {
      assert.notEqual((await fetch(`${environment.SUPABASE_URL}${route}`, { headers: service })).status, 200, route);
    }

    // Secrets stay out of the lease, container definitions and process arguments.
    const secretValues = Object.values(secrets);
    const leaseText = readFileSync(path, 'utf8');
    const definitions = owned.map(id => attemptDocker(['inspect', '--format',
      '{{json .Path}}{{json .Args}}{{json .Config.Cmd}}{{json .Config.Entrypoint}}', id]));
    const processes = attemptDocker(['top', anchor.id, '-eo', 'pid,args']);
    for (const value of secretValues) {
      assert(!leaseText.includes(value) && !definitions.some(text => text.includes(value)) && !processes.includes(value));
    }
    // The application's database password cannot reach the platform superuser.
    const login = (user: string, password: string) => {
      try {
        attemptDocker(['exec', '-i', anchor.id, 'sh', '-ec', `PGPASSWORD=$(cat) psql -h 127.0.0.1 -U ${user} -d postgres -Atc 'select 1'`],
          password);
        return true;
      } catch { return false; }
    };
    assert.equal(login('postgres', secrets.postgresPassword), true);
    assert.equal(login('supabase_admin', secrets.postgresPassword), false);
    assert.equal(login('supabase_admin', ''), false);
    evidence.secrets = 'absent from the lease, container definitions and process arguments; superuser separated';

    // Application state of every kind, then a data-level reset.
    supabaseSql(active, 'create schema app; create table app.items (id int primary key); insert into app.items values (1); '
      + 'create extension pg_cron; alter publication supabase_realtime add table app.items;\n');
    const signUp = await fetch(`${environment.SUPABASE_URL}/auth/v1/signup`, { method: 'POST', headers: anon,
      body: JSON.stringify({ email: 'owned@example.com', password: 'password123' }) });
    assert.equal(signUp.status, 200);
    assert.equal((await fetch(`${environment.SUPABASE_URL}/storage/v1/bucket`, { method: 'POST',
      headers: { ...service, 'content-type': 'application/json' }, body: JSON.stringify({ name: 'owned' }) })).status, 200);
    assert.equal((await fetch(`${environment.SUPABASE_URL}/storage/v1/object/owned/file.txt`, { method: 'POST',
      headers: { ...service, 'content-type': 'text/plain' }, body: 'stored' })).status, 200);
    const resetStarted = Date.now();
    resetSupabase({ lease: active });
    evidence.resetMs = Date.now() - resetStarted;
    assert.equal(supabaseSql(active, "select count(*) from pg_namespace where nspname in ('app', 'cron');\n").trim(), '0');
    assert.equal(supabaseSql(active, 'select (select count(*) from auth.users) + (select count(*) from storage.buckets);\n').trim(), '0');
    assert.equal(attemptDocker(['exec', services.storage!.id, 'find', '/var/lib/storage', '-mindepth', '1']), '');
    assert.equal(supabaseSql(active, "select string_agg(pubname, ',' order by pubname) from pg_publication;\n").trim(),
      'supabase_realtime,supabase_realtime_messages_publication');
    assert.equal(supabaseSql(active, "select string_agg(prrelid::regclass::text, ',') from pg_publication_rel pr "
      + "join pg_publication p on p.oid = pr.prpubid where p.pubname = 'supabase_realtime';\n").trim(),
    'stackbench_reset.realtime_canary');
    supabaseSql(active, 'create extension pg_cron;\n');
    const signIn = await fetch(`${environment.SUPABASE_URL}/auth/v1/token?grant_type=password`, { method: 'POST',
      headers: anon, body: JSON.stringify({ email: 'owned@example.com', password: 'password123' }) });
    assert.equal(signIn.status, 400, 'a previous account cannot sign in after reset');
    resetSupabase({ lease: active });
    assert.equal(supabaseSql(active, "select count(*) from pg_extension where extname = 'pg_cron';\n").trim(), '0');
    evidence.reset = 'schemas, extensions with their schemas, accounts and stored files removed; Realtime publications kept';

    // A database process crash leaves the namespace and services; recovery restarts only the database.
    await assert.rejects(async () => recoverSupabase({ leasePath: path, leaseToken: lease.ownershipToken, ports }),
      /Command failed/, 'recovery refuses a live database');
    const crash = await prepareProcessCrash(active, 'database');
    try {
      evidence.crash = await crash.crash();
      assert.equal(attemptDocker(['inspect', '--format', '{{.State.Running}}', anchor.id]), 'true');
      recoverSupabase({ leasePath: path, leaseToken: lease.ownershipToken, ports });
    } finally { await crash.close(); }
    const deadline = Date.now() + 30_000;
    while ((await fetch(`${environment.SUPABASE_URL}/rest/v1/`, { headers: service })).status !== 200) {
      assert(Date.now() < deadline, 'REST serves again after database recovery');
      await new Promise(resolve => setTimeout(resolve, 500));
    }
    requireAttemptNetwork(readBackendLease(path));
    evidence.result = 'passed';
  } catch (error) {
    evidence.result = 'failed';
    evidence.error = error instanceof Error ? error.message : String(error);
    failure = error;
  } finally {
    try {
      if (existsSync(path)) {
        const partial = readBackendLease(path);
        for (const container of [partial.resources.container, ...Object.values(partial.resources.serviceContainers ?? {}),
          partial.resources.browserContainer]) if (container && !owned.includes(container.id)) owned.push(container.id);
        network ??= partial.resources.network?.id;
        assert.equal(releaseBackendLease(path, lease.ownershipToken), true, 'Exact owned cleanup must succeed');
        const final = readBackendLease(path);
        evidence.finalLease = publicBackendLease(final);
        assert.equal(final.state, 'released');
        for (const id of owned) assert.throws(() => attemptDocker(['inspect', id]), /no such object/i);
        for (const volume of volumes) assert.throws(() => attemptDocker(['volume', 'inspect', volume]), /no such volume/i);
        if (network) assert.throws(() => attemptDocker(['network', 'inspect', network!]), /not found|no such network/i);
        evidence.cleanup = { containers: owned.length, volumes: volumes.length, network: Boolean(network) };
      }
    } catch (error) {
      evidence.result = 'failed';
      evidence.cleanupError = error instanceof Error ? error.message : String(error);
      failure ??= error;
    } finally {
      save();
      rmSync(root, { recursive: true, force: true });
    }
  }
  if (failure) throw failure;
});
