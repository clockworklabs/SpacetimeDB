import { readFileSync, renameSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { withLoginLock } from './login-lock.js';

// A SuperGrok login written by `grok login` (~/.grok/auth.json). The access token
// lasts hours; the refresh token rotates on every exchange, so exactly one process
// may refresh a login at a time.
const ISSUER = 'https://auth.x.ai';
const TOKEN_ENDPOINT = `${ISSUER}/oauth2/token`;

interface GrokScope extends Record<string, unknown> {
  key: string; refresh_token: string; expires_at: string; oidc_issuer: string; oidc_client_id: string;
  principal_id?: string;
}

export interface GrokLogin { scope: string; token: string; expiresAtMs: number; identity: string }

function scopeOf(file: Record<string, unknown>): [string, GrokScope] {
  const scopes = Object.entries(file).filter(([name]) => name.startsWith(`${ISSUER}::`));
  if (scopes.length !== 1) throw new Error('Grok login must hold exactly one auth.x.ai sign-in');
  const [name, value] = scopes[0]!;
  const scope = value as GrokScope;
  if (!scope || typeof scope.key !== 'string' || !scope.key || typeof scope.refresh_token !== 'string'
    || !scope.refresh_token || scope.oidc_issuer !== ISSUER || typeof scope.oidc_client_id !== 'string'
    || !Number.isFinite(Date.parse(scope.expires_at))) {
    throw new Error('Grok login is not an auth.x.ai OIDC sign-in; run `grok login --device-auth` again');
  }
  return [name, scope];
}

export function readGrokLogin(text: string): GrokLogin {
  let file: unknown;
  try { file = JSON.parse(text); } catch { throw new Error('Grok login must be the JSON written by `grok login`'); }
  if (!file || typeof file !== 'object' || Array.isArray(file)) throw new Error('Grok login must be a JSON object');
  const [name, scope] = scopeOf(file as Record<string, unknown>);
  // The account, not the current token bytes, is what a campaign pins: tokens rotate.
  const identity = createHash('sha256')
    .update(JSON.stringify([scope.oidc_issuer, scope.oidc_client_id, scope.principal_id ?? null])).digest('hex');
  return { scope: name, token: scope.key, expiresAtMs: Date.parse(scope.expires_at), identity };
}

type Fetch = (url: string, init: { method: string; headers: Record<string, string>; body: string;
  signal: AbortSignal }) => Promise<{ ok: boolean; status: number; json(): Promise<unknown> }>;

// Refresh the login when fewer than `minRemainingMs` remain, under an exclusive
// lock next to the file. The replacement is written atomically and keeps every
// other field `grok login` stored.
export async function ensureFreshGrokLogin(path: string, minRemainingMs: number,
  { fetch: request = fetch as unknown as Fetch, now = Date.now, waitMs = 250 }:
  { fetch?: Fetch; now?: () => number; waitMs?: number } = {}): Promise<GrokLogin> {
  const current = readGrokLogin(readFileSync(path, 'utf8'));
  if (current.expiresAtMs - now() >= minRemainingMs) return current;
  return withLoginLock(path, async () => {
    // Another holder may have refreshed while this one waited.
    const text = readFileSync(path, 'utf8');
    const latest = readGrokLogin(text);
    if (latest.expiresAtMs - now() >= minRemainingMs) return latest;
    const file = JSON.parse(text) as Record<string, unknown>;
    const [name, scope] = scopeOf(file);
    const response = await request(TOKEN_ENDPOINT, {
      method: 'POST', headers: { 'content-type': 'application/x-www-form-urlencoded', accept: 'application/json' },
      body: new URLSearchParams({ grant_type: 'refresh_token', refresh_token: scope.refresh_token,
        client_id: scope.oidc_client_id }).toString(),
      signal: AbortSignal.timeout(30_000),
    });
    if (!response.ok) throw new Error(`Grok login refresh was refused (HTTP ${response.status}); run \`grok login --device-auth\` again`);
    const issued = await response.json() as { access_token?: unknown; refresh_token?: unknown; expires_in?: unknown };
    if (typeof issued.access_token !== 'string' || !issued.access_token || typeof issued.expires_in !== 'number'
      || !(issued.expires_in > 0)) throw new Error('Grok login refresh returned no usable access token');
    file[name] = { ...scope, key: issued.access_token,
      refresh_token: typeof issued.refresh_token === 'string' && issued.refresh_token ? issued.refresh_token : scope.refresh_token,
      expires_at: new Date(now() + issued.expires_in * 1000).toISOString() };
    const temporary = `${path}.stack-bench-${process.pid}`;
    writeFileSync(temporary, `${JSON.stringify(file)}\n`, { mode: 0o600 });
    renameSync(temporary, path);
    const refreshed = readGrokLogin(JSON.stringify(file));
    if (refreshed.expiresAtMs - now() < minRemainingMs) {
      throw new Error('Grok login refresh issued a token shorter than an attempt');
    }
    return refreshed;
  }, waitMs);
}
