import { leaseFromEnv, type BackendLease } from '../../runtime/backend-lease.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';
import type { NamedAction } from '../../composition/tracks.js';
import type { AuthRequestPatch, PlatformAuthPatch } from '../../actions/auth-request-patch.js';
import type { LeasedDatabase } from '../backend-reset-guard.js';
import type { OrderDataStorage } from '../order-data.js';
import { provePostgresMarker, readPostgresOrderData, readPostgresStock, writePostgresStock,
  type PsqlRunner } from '../postgres-sql.js';
import { readSupabasePlatformSecrets, supabaseGatewayUrl, supabaseSql } from './supabase-platform.js';

// Observers and the external stock writer use the fixed `order-data` tables in
// `public` through privileged SQL, which bypasses row-level security.
const READ_TIMEOUT_MS = 60_000;
const PROVENANCE_TIMEOUT_MS = 120_000;
const SUPABASE = { backend: 'supabase', label: 'Supabase', quiet: true };

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object';

// grading.databaseLease hands observers the authenticated platform lease itself.
type PlatformDatabase = BackendLease & LeasedDatabase;

function psql(lease: LeasedDatabase | BackendLease, exec: TextCommandExecutor | undefined, timeoutMs: number): PsqlRunner {
  return sql => supabaseSql(lease as PlatformDatabase, sql, { timeoutMs, ...(exec ? { exec } : {}) });
}

export function getSupabaseCheckoutState({ account, item, storage, lease, exec }: {
  account: string; item: string; app?: string; storage?: OrderDataStorage; lease: LeasedDatabase; exec?: TextCommandExecutor;
}) {
  // Only zero-point diagnostic reads omit storage; that is the caller's error, not the app's.
  if (!storage) throw new Error('Supabase requires the declared order data interface');
  return readPostgresOrderData(psql(lease, exec, READ_TIMEOUT_MS), account, item, storage);
}

export function getSupabaseStock({ item, warehouse, lease, exec }: {
  item: string; warehouse?: string; lease: LeasedDatabase; exec?: TextCommandExecutor;
}): { backend: string; item: string; warehouse?: string; quantity: number } {
  return readPostgresStock(psql(lease, exec, READ_TIMEOUT_MS), SUPABASE, item, warehouse);
}

export function setSupabaseStock({ item, warehouse, quantity, lease, exec }: {
  item: string; warehouse: string; quantity: number; lease: LeasedDatabase; exec?: TextCommandExecutor;
}): { backend: string; item: string; warehouse: string; quantity: number } {
  return writePostgresStock(psql(lease, exec, READ_TIMEOUT_MS), SUPABASE, { item, warehouse, quantity });
}

// Auth's own tables are not the application's records.
export function proveSupabaseUse({ lease, marker, exec }: { lease: BackendLease; marker: unknown; exec?: TextCommandExecutor }):
  { ok: boolean; verified: boolean; matches: number; reason: string } {
  return provePostgresMarker(psql(lease, exec, PROVENANCE_TIMEOUT_MS), SUPABASE, marker);
}

// Data API and Edge Function requests to the leased gateway carry the
// application's writes. Auth requests carry credentials, not application writes.
export function supabaseWriteEndpoints(lease: BackendLease): readonly string[] {
  const gateway = supabaseGatewayUrl(lease);
  return [`${gateway}/rest/v1/`, `${gateway}/functions/v1/`];
}

// Password sign-up and sign-in at the leased gateway. Applications may send
// Auth any encoding of the typed username and password, so the endpoint alone
// identifies the credential request: a changed password replaces whatever
// the application sent, and claimed fields go at the top level and into
// `data`, the user metadata a sign-up stores.
export function supabaseAuthRequestPatch(lease: BackendLease): PlatformAuthPatch {
  const gateway = supabaseGatewayUrl(lease);
  return (url: string, body: unknown, patch: AuthRequestPatch) => {
    const target = new URL(url);
    if (target.origin !== gateway || !(target.pathname === '/auth/v1/signup'
      || target.pathname === '/auth/v1/token' && target.searchParams.get('grant_type') === 'password')) return undefined;
    if (!record(body) || Array.isArray(body) || typeof body.password !== 'string') return null;
    const copy = structuredClone(body);
    const define = (object: Record<string, unknown>, key: string, value: unknown) =>
      Object.defineProperty(object, key, { value, enumerable: true, writable: true, configurable: true });
    if (Object.hasOwn(patch, 'password')) define(copy, 'password', patch.password);
    const fields = Object.entries(patch.fields ?? {});
    if (fields.length) {
      const metadata = copy.data ?? {};
      if (!record(metadata) || Array.isArray(metadata)) throw new Error('Credential metadata is not an object');
      const data = { ...metadata };
      for (const [key, value] of fields) {
        define(copy, key, value);
        define(data, key, value);
      }
      define(copy, 'data', data);
    }
    return { body: JSON.stringify(copy), shape: 'supabase-auth' };
  };
}

// A named operation is a `public` function called through the data API. The
// project key identifies the project; the caller's bearer token replaces the
// anonymous default and identifies the actor.
export function supabaseNamedActionRequest({ action, input, lease, exec }: {
  action: NamedAction; input?: unknown; spacetime?: unknown; url?: string | null;
  lease?: BackendLease; exec?: TextCommandExecutor;
}) {
  if (!action.reducer) return null;
  const supplied = (input && typeof input === 'object' ? input : {}) as {
    values?: Record<string, unknown>; args?: readonly unknown[]; body?: Record<string, unknown>;
  };
  const params = action.params ?? [];
  const args = supplied.args ?? action.args ?? [];
  if (!supplied.values && args.length && params.length !== args.length) {
    throw Object.assign(new Error('Supabase named action requires declared argument names'), { code: 'invalid_named_action_input' });
  }
  const values = supplied.values ?? supplied.body ?? Object.fromEntries(params.map((param, index) => [param.name, args[index]]));
  const leased = lease ?? leaseFromEnv(process.env, { backend: 'supabase', active: true }).lease;
  const { anonKey } = readSupabasePlatformSecrets(leased, exec);
  return {
    url: `${supabaseGatewayUrl(leased)}/rest/v1/rpc/${encodeURIComponent(action.reducer)}`,
    method: 'POST',
    body: JSON.stringify(values),
    headers: { apikey: anonKey, Authorization: `Bearer ${anonKey}` },
    missingNote: `no database function public.${action.reducer}`,
  };
}
