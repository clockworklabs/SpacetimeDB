import { readFileSync, renameSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { withLoginLock } from './login-lock.js';

// A Google account sign-in written by the Antigravity CLI
// (~/.gemini/antigravity-cli/antigravity-oauth-token). The access token lasts an hour.
// Only the CLI can renew it: its OAuth client secret is not published.
export interface AgyLogin { token: string; expiresAtMs: number; identity: string }

const SIGN_IN_AGAIN = 'sign in with agy again';

export function readAgyLogin(text: string): AgyLogin {
  let file: unknown;
  try { file = JSON.parse(text); } catch { throw new Error(`Antigravity login must be the JSON file agy writes; ${SIGN_IN_AGAIN}`); }
  const { token, auth_method: method } = (file ?? {}) as Record<string, unknown>;
  const { access_token: access, refresh_token: refresh, expiry } = (token ?? {}) as Record<string, unknown>;
  if (method !== 'consumer' || typeof access !== 'string' || !access || typeof refresh !== 'string' || !refresh
    || typeof expiry !== 'string' || !Number.isFinite(Date.parse(expiry))) {
    throw new Error(`Antigravity login is not a Google account sign-in; ${SIGN_IN_AGAIN}`);
  }
  // A campaign pins the sign-in, not the current access token. Renewal keeps the refresh
  // token, while the file's account fields differ between CLI versions.
  const identity = createHash('sha256').update(refresh).digest('hex');
  return { token: access, expiresAtMs: Date.parse(expiry), identity };
}

// Renew the login when fewer than `minRemainingMs` remain, under an exclusive lock next
// to the file. `renew` runs the CLI on the file's text and returns the text it leaves.
export async function ensureFreshAgyLogin(path: string, minRemainingMs: number,
  renew: (text: string) => Promise<string>, { now = Date.now, waitMs = 250 }: { now?: () => number; waitMs?: number } = {},
): Promise<AgyLogin> {
  const current = readAgyLogin(readFileSync(path, 'utf8'));
  if (current.expiresAtMs - now() >= minRemainingMs) return current;
  return withLoginLock(path, async () => {
    // Another holder may have renewed while this one waited.
    const text = readFileSync(path, 'utf8');
    const latest = readAgyLogin(text);
    if (latest.expiresAtMs - now() >= minRemainingMs) return latest;
    const renewed = (await renew(text)).trim();
    const login = readAgyLogin(renewed);
    if (login.identity !== latest.identity) throw new Error(`Antigravity renewal returned another account; ${SIGN_IN_AGAIN}`);
    if (login.expiresAtMs - now() < minRemainingMs) throw new Error(`Antigravity renewal did not extend the sign-in; ${SIGN_IN_AGAIN}`);
    const temporary = `${path}.stack-bench-${process.pid}`;
    writeFileSync(temporary, `${renewed}\n`, { mode: 0o600 });
    renameSync(temporary, path);
    return login;
  }, waitMs);
}
