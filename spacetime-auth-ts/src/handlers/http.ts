import { SyncResponse, type Request } from 'spacetimedb/server';
import type { AuthConfig } from '../types.js';
import type { AuthTransactionCtx } from '../context.js';

/** Thrown by handlers; the client() boundary turns it into a JSON error response. */
export class HttpError extends Error {
  constructor(
    public code: string,
    public status: number
  ) {
    super(code);
  }
}

export interface CookieOptions {
  maxAgeSeconds?: number;
  secure: boolean;
}

export function makeCookie(
  name: string,
  value: string,
  options: CookieOptions
): string {
  const parts = [`${name}=${value}`, 'Path=/'];
  if (options.maxAgeSeconds != null)
    parts.push(`Max-Age=${options.maxAgeSeconds}`);
  parts.push('HttpOnly');
  if (options.secure) parts.push('Secure');
  parts.push('SameSite=Lax');
  return parts.join('; ');
}

export function clearCookie(name: string, secure: boolean): string {
  return makeCookie(name, '', { maxAgeSeconds: 0, secure });
}

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
  cookies: string[] = []
): SyncResponse {
  return new SyncResponse(JSON.stringify(body), {
    status,
    headers: [
      ['content-type', 'application/json'],
      ...cookies.map((c): [string, string] => ['set-cookie', c]),
    ],
  });
}

export function errorResponse(
  code: string,
  status: number,
  extraHeaders: Record<string, string> = {}
): SyncResponse {
  return new SyncResponse(JSON.stringify({ error: code }), {
    status,
    headers: { 'content-type': 'application/json', ...extraHeaders },
  });
}

export function redirectResponse(
  location: string,
  cookies: string[] = []
): SyncResponse {
  return new SyncResponse('', {
    status: 302,
    headers: [
      ['location', location],
      ...cookies.map((c): [string, string] => ['set-cookie', c]),
    ],
  });
}

export function requireConfig(tx: AuthTransactionCtx): AuthConfig {
  const cfg = tx.db.authConfig.singleton.find(true);
  if (!cfg) throw new HttpError('config_missing', 500);
  return cfg;
}

/** Bearer token, else the session cookie. */
export function readBearer(req: Request, cookieName: string): string | null {
  const auth = req.headers.get('authorization');
  if (auth && auth.toLowerCase().startsWith('bearer ')) {
    return auth.slice(7).trim();
  }
  return parseCookies(req.headers.get('cookie'))[cookieName] ?? null;
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
