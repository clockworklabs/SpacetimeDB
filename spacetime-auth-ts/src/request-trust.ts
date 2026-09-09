import type { Request } from 'spacetimedb/server';

export type TrustedProxyHeader =
  | 'cf-connecting-ip'
  | 'x-real-ip'
  | 'x-forwarded-for';

export interface AuthHttpOptions {
  /** Header set by a trusted proxy after it removes any client-supplied value. */
  trustedProxyHeader?: TrustedProxyHeader;
  /** Defaults to true. Set false only for local HTTP development. */
  secureCookies?: boolean;
}

function hasControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index);
    if (code <= 0x1f || code === 0x7f) return true;
  }
  return false;
}

function firstHeaderValue(value: string | null): string | undefined {
  if (!value) return undefined;
  const first = value.split(',')[0]?.trim();
  return first && first.length <= 128 ? first : undefined;
}

export function clientKey(
  req: Request,
  trustedProxyHeader?: TrustedProxyHeader
): string | undefined {
  if (!trustedProxyHeader) return undefined;
  return firstHeaderValue(req.headers.get(trustedProxyHeader));
}

export function userAgent(req: Request): string | undefined {
  const value = req.headers.get('user-agent')?.trim();
  return value && value.length <= 512 ? value : undefined;
}

export function shouldUseSecureCookies(secureCookies?: boolean): boolean {
  return secureCookies !== false;
}

export function safeRedirectPath(
  value: string | undefined
): string | undefined {
  if (value === undefined || value.length === 0 || value.length > 2048)
    return undefined;
  if (!value.startsWith('/') || value.startsWith('//')) return undefined;
  if (
    value.includes('\\') ||
    value.includes('#') ||
    hasControlCharacter(value)
  ) {
    return undefined;
  }
  try {
    const decoded = decodeURIComponent(value);
    if (
      !decoded.startsWith('/') ||
      decoded.startsWith('//') ||
      decoded.includes('\\') ||
      decoded.includes('#') ||
      hasControlCharacter(decoded)
    ) {
      return undefined;
    }
  } catch {
    return undefined;
  }
  return value;
}
