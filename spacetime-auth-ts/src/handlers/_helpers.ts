import { SyncResponse, type Request } from 'spacetimedb/server';
import { verifyJwt, type JwtClaims } from '../jwt.ts';
import { privateKeyFromPem, publicKeyFromPem } from '../keys.ts';
import type { AuthConfig } from '../types.ts';
import type { AuthHandlerCtx, AuthTransactionCtx } from '../context.ts';

export type { AuthHandlerCtx, AuthTransactionCtx };

export interface CookieOpts {
  maxAgeSeconds?: number;
  path?: string;
  domain?: string;
  httpOnly?: boolean;
  secure?: boolean;
  sameSite?: 'Strict' | 'Lax' | 'None';
}

export function makeCookie(
  name: string,
  value: string,
  opts: CookieOpts = {}
): string {
  const parts = [`${name}=${value}`];
  parts.push(`Path=${opts.path ?? '/'}`);
  if (opts.maxAgeSeconds != null) parts.push(`Max-Age=${opts.maxAgeSeconds}`);
  if (opts.domain) parts.push(`Domain=${opts.domain}`);
  if (opts.httpOnly !== false) parts.push('HttpOnly');
  if (opts.secure !== false) parts.push('Secure');
  parts.push(`SameSite=${opts.sameSite ?? 'Lax'}`);
  return parts.join('; ');
}

export function clearCookie(name: string, opts: CookieOpts = {}): string {
  return makeCookie(name, '', { ...opts, maxAgeSeconds: 0 });
}

export { shouldUseSecureCookies, userAgent } from '../request-trust.ts';

export function parseCookies(
  header: string | null | undefined
): Record<string, string> {
  const out: Record<string, string> = {};
  if (!header) return out;
  for (const part of header.split(';')) {
    const eq = part.indexOf('=');
    if (eq < 0) continue;
    const k = part.slice(0, eq).trim();
    const v = part.slice(eq + 1).trim();
    if (k) out[k] = v;
  }
  return out;
}

export function jsonResponse(
  body: unknown,
  status = 200,
  extraHeaders: Record<string, string> = {}
): SyncResponse {
  return new SyncResponse(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json', ...extraHeaders },
  });
}

export function errorResponse(
  code: string,
  status: number,
  extraHeaders: Record<string, string> = {}
): SyncResponse {
  return jsonResponse({ error: code }, status, extraHeaders);
}

export function redirectResponse(
  location: string,
  extraHeaders: Record<string, string> = {}
): SyncResponse {
  return new SyncResponse('', {
    status: 302,
    headers: { location, ...extraHeaders },
  });
}

export function requireConfig(tx: AuthTransactionCtx): AuthConfig {
  const cfg = tx.db.authConfig.singleton.find(true);
  if (!cfg) throw new ConfigMissingError();
  return cfg;
}

export class ConfigMissingError extends Error {
  constructor() {
    super('auth_config singleton missing; call setAuthConfig first');
  }
}

export function microsToSeconds(t: { microsSinceUnixEpoch: bigint }): number {
  return Number(t.microsSinceUnixEpoch / 1_000_000n);
}

export function secondsToTimestamp(seconds: number | bigint): {
  microsSinceUnixEpoch: bigint;
} {
  return { microsSinceUnixEpoch: BigInt(seconds) * 1_000_000n };
}

export function readBearer(req: Request, cookieName: string): string | null {
  const auth = req.headers.get('authorization');
  if (auth && auth.toLowerCase().startsWith('bearer ')) {
    return auth.slice(7).trim();
  }
  const cookies = parseCookies(req.headers.get('cookie'));
  return cookies[cookieName] ?? null;
}

export function readSession(
  req: Request,
  cookieName: string,
  publicKey: Uint8Array,
  issuer?: string
): JwtClaims | null {
  const token = readBearer(req, cookieName);
  if (!token) return null;
  const r = verifyJwt(publicKey, token, { issuer });
  return r.ok ? r.claims : null;
}

export function configKeys(cfg: AuthConfig): {
  privateKey: Uint8Array;
  publicKey: Uint8Array;
} {
  return {
    privateKey: privateKeyFromPem(cfg.es256PrivateKeyPem),
    publicKey: publicKeyFromPem(cfg.es256PublicKeyPem),
  };
}

export function safeJson<T>(req: Request): T | null {
  try {
    return req.json() as T;
  } catch {
    return null;
  }
}

/** STDB V8 isolate has no globalThis.URL. */
export function parseQueryString(uri: string): Record<string, string> {
  const q = uri.indexOf('?');
  if (q < 0) return {};
  const out: Record<string, string> = {};
  for (const pair of uri.slice(q + 1).split('&')) {
    const eq = pair.indexOf('=');
    try {
      if (eq < 0) {
        out[decodeURIComponent(pair)] = '';
      } else {
        out[decodeURIComponent(pair.slice(0, eq))] = decodeURIComponent(
          pair.slice(eq + 1)
        );
      }
    } catch {
      // Ignore malformed percent-encoding. Callers will treat the missing
      // parameter as a controlled bad request and keep the handler available.
    }
  }
  return out;
}
