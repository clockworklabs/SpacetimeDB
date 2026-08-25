import {
  bytesToHex,
  hexToBytes,
  sha256,
  timingSafeEqual,
} from '@spacetimedb/crypto';

export const LOOKUP_SECRET_CHARS = 10;
const textEncoder = new TextEncoder();

export function base64Url(bytes: Uint8Array): string {
  const alphabet =
    'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';
  let out = '';
  let i = 0;
  for (; i + 2 < bytes.length; i += 3) {
    out += alphabet[bytes[i] >> 2];
    out += alphabet[((bytes[i] & 3) << 4) | (bytes[i + 1] >> 4)];
    out += alphabet[((bytes[i + 1] & 15) << 2) | (bytes[i + 2] >> 6)];
    out += alphabet[bytes[i + 2] & 63];
  }
  if (i < bytes.length) {
    out += alphabet[bytes[i] >> 2];
    if (i + 1 === bytes.length) {
      out += alphabet[(bytes[i] & 3) << 4];
    } else {
      out += alphabet[((bytes[i] & 3) << 4) | (bytes[i + 1] >> 4)];
      out += alphabet[(bytes[i + 1] & 15) << 2];
    }
  }
  return out;
}

export function extractLookupPrefix(key: string): string | undefined {
  const trimmed = key.trim();
  const lastUnderscore = trimmed.lastIndexOf('_');
  if (lastUnderscore <= 0) return undefined;
  const keyPrefix = trimmed.slice(0, lastUnderscore);
  const secret = trimmed.slice(lastUnderscore + 1);
  if (secret.length < LOOKUP_SECRET_CHARS) return undefined;
  return `${keyPrefix}_${secret.slice(0, LOOKUP_SECRET_CHARS)}`;
}

export function hashApiKey(key: string): string {
  return bytesToHex(sha256(textEncoder.encode(key)));
}

export function hashMatches(key: string, expectedHex: string): boolean {
  try {
    return timingSafeEqual(
      hexToBytes(expectedHex),
      sha256(textEncoder.encode(key))
    );
  } catch {
    return false;
  }
}

export function hasScope(
  scopesJson: string,
  requiredScope: string | undefined
): boolean {
  if (requiredScope === undefined || requiredScope.trim() === '') return true;
  const required = requiredScope.trim();
  let scopes: unknown;
  try {
    scopes = JSON.parse(scopesJson);
  } catch {
    return false;
  }
  if (!Array.isArray(scopes)) return false;
  return scopes.some(
    scope =>
      typeof scope === 'string' &&
      (scope === '*' ||
        scope === required ||
        (scope.endsWith(':*') && required.startsWith(scope.slice(0, -1))))
  );
}
