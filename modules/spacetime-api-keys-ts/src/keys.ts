import {
  bytesToHex,
  hexToBytes,
  hmacSha256,
  sha256,
  timingSafeEqual,
} from '@spacetimedb/crypto';

// Hex characters of the secret stored as the lookup prefix (64 bits).
const LOOKUP_SECRET_CHARS = 16;
const textEncoder = new TextEncoder();

// ctx.random is seeded from the call timestamp, so it cannot produce secrets.
// Key material is HMAC-SHA256 keyed by the operator's secret over a
// database-wide counter and the transaction timestamp. The counter makes each
// draw unique; the timestamp separates databases recreated with one secret.
export function deriveKeySecret(
  operatorSecret: string,
  counter: bigint,
  micros: bigint
): Uint8Array {
  return hmacSha256(
    textEncoder.encode(operatorSecret),
    textEncoder.encode(`spacetimedb-api-keys secret ${counter} ${micros}`)
  );
}

/** Formats a key as `${keyPrefix}_${hex secret}` with its stored lookup prefix. */
export function formatApiKey(
  keyPrefix: string,
  secret: Uint8Array
): { key: string; prefix: string } {
  const hex = bytesToHex(secret);
  return {
    key: `${keyPrefix}_${hex}`,
    prefix: `${keyPrefix}_${hex.slice(0, LOOKUP_SECRET_CHARS)}`,
  };
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

export function matchesApiKeyHash(key: string, expectedHex: string): boolean {
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
