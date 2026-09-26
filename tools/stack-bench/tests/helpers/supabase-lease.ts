import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createBackendLease, writeBackendLease, type BackendLease } from '../../src/runtime/backend-lease.js';
import type { TextCommandExecutor } from '../../src/runtime/command-executor.js';
import { formatSupabaseSecrets, generateSupabaseSecrets } from '../../src/stacks/backends/supabase-platform.js';

export const SUPABASE_GATEWAY = 'http://127.0.0.1:13410';
export const SUPABASE_SECRETS = generateSupabaseSecrets();
const STARTED_AT = '2026-09-25T00:00:00.000Z';

// An active, isolated Supabase lease with an owned anchor.
export function supabaseLease(): BackendLease {
  const lease = createBackendLease({ runId: 'supabase-grading', backend: 'supabase', track: 'ecommerce',
    runIndex: 0, serverUri: SUPABASE_GATEWAY, database: 'postgres' });
  const anchor = 'a'.repeat(64);
  lease.state = 'active';
  lease.resources.container = { name: 'sb-anchor', id: anchor, owned: true };
  lease.resources.network = { name: 'sb-network', id: 'b'.repeat(64), namespaceContainerId: anchor,
    namespaceStartedAt: STARTED_AT, hostAddresses: [], services: [],
    firewallSha256: 'c'.repeat(64), firewallInstalledAt: STARTED_AT };
  return lease;
}

// Docker as the anchor presents it: identity, the root-only secret file, and
// privileged psql, whose SQL arrives on stdin.
export function supabaseExec(lease: BackendLease, psql: (sql: string, args: readonly string[]) => string,
  calls: { args: readonly string[] }[] = []): TextCommandExecutor {
  return (command, args, options) => {
    if (command !== 'docker') throw new Error(`unexpected command ${command}`);
    calls.push({ args });
    if (args[0] === 'inspect') return JSON.stringify({ Id: lease.resources.container!.id,
      State: { Running: true, StartedAt: lease.resources.network!.namespaceStartedAt } });
    if (args.includes('cat')) return formatSupabaseSecrets(SUPABASE_SECRETS);
    if (args.includes('psql')) return psql(String(options.input ?? ''), args);
    throw new Error(`unexpected docker ${args.join(' ')}`);
  };
}

// The grader's authenticated lease environment for the duration of one test.
export function withSupabaseLeaseEnvironment(t: { after(fn: () => void): void }, lease: BackendLease): void {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-supabase-lease-'));
  const path = join(root, 'lease.json');
  writeBackendLease(path, lease);
  const previous = { path: process.env.STACK_BENCH_LEASE, token: process.env.STACK_BENCH_LEASE_TOKEN };
  process.env.STACK_BENCH_LEASE = path;
  process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
  t.after(() => {
    for (const [key, value] of [['STACK_BENCH_LEASE', previous.path], ['STACK_BENCH_LEASE_TOKEN', previous.token]] as const) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    rmSync(root, { recursive: true, force: true });
  });
}
