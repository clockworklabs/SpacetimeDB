import { scrypt } from '@noble/hashes/scrypt.js';
import {
  base64ToBytes,
  hmacSha256,
  sha256,
  timingSafeEqual,
} from '@spacetimedb/crypto';

export const utf8 = new TextEncoder();

// N=2^14 keeps single-hash under ~300ms in STDB's V8 isolate.
const SCRYPT = { N: 1 << 14, r: 8, p: 1, dkLen: 32 };

const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

export function base64Encode(bytes: Uint8Array): string {
  let out = '';
  let i = 0;
  for (; i + 2 < bytes.length; i += 3) {
    out += B64[bytes[i] >> 2];
    out += B64[((bytes[i] & 3) << 4) | (bytes[i + 1] >> 4)];
    out += B64[((bytes[i + 1] & 15) << 2) | (bytes[i + 2] >> 6)];
    out += B64[bytes[i + 2] & 63];
  }
  if (i + 1 === bytes.length) {
    out += B64[bytes[i] >> 2] + B64[(bytes[i] & 3) << 4] + '==';
  } else if (i + 2 === bytes.length) {
    out += B64[bytes[i] >> 2];
    out += B64[((bytes[i] & 3) << 4) | (bytes[i + 1] >> 4)];
    out += B64[(bytes[i + 1] & 15) << 2] + '=';
  }
  return out;
}

export function base64UrlEncode(bytes: Uint8Array): string {
  return base64Encode(bytes)
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
}

/** Throws on malformed input. */
export function base64UrlDecode(value: string): Uint8Array {
  if (/[+/=]/.test(value)) throw new Error('base64url: invalid char');
  return base64ToBytes(value.replace(/-/g, '+').replace(/_/g, '/'));
}

/** Encoded as `scrypt$N$r$p$saltB64$hashB64`. */
export function hashPassword(password: string, salt: Uint8Array): string {
  const hash = scrypt(utf8.encode(password), salt, SCRYPT);
  return `scrypt$${SCRYPT.N}$${SCRYPT.r}$${SCRYPT.p}$${base64Encode(salt)}$${base64Encode(hash)}`;
}

/** Verified in place of a missing hash so unknown accounts take as long as known ones. */
export const DUMMY_PASSWORD_HASH = `scrypt$${SCRYPT.N}$${SCRYPT.r}$${SCRYPT.p}$${base64Encode(new Uint8Array(16))}$${base64Encode(new Uint8Array(SCRYPT.dkLen))}`;

export function verifyPassword(password: string, encoded: string): boolean {
  const parts = encoded.split('$');
  if (parts.length !== 6 || parts[0] !== 'scrypt') return false;
  const [N, r, p] = parts.slice(1, 4).map(part => parseInt(part, 10));
  if (!Number.isFinite(N) || !Number.isFinite(r) || !Number.isFinite(p))
    return false;
  let salt: Uint8Array;
  let expected: Uint8Array;
  try {
    salt = base64ToBytes(parts[4]);
    expected = base64ToBytes(parts[5]);
  } catch {
    return false;
  }
  const actual = scrypt(utf8.encode(password), salt, {
    N,
    r,
    p,
    dkLen: expected.length,
  });
  return timingSafeEqual(expected, actual);
}

export function pkceChallenge(verifier: string): string {
  return base64UrlEncode(sha256(utf8.encode(verifier)));
}

export function uuidV7(nowMs: bigint, rand: Uint8Array): string {
  const bytes = new Uint8Array(16);
  for (let i = 0; i < 6; i++) {
    bytes[i] = Number((nowMs >> BigInt(40 - 8 * i)) & 0xffn);
  }
  bytes.set(rand.subarray(0, 10), 6);
  bytes[6] = (bytes[6] & 0x0f) | 0x70;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

// ctx.random is seeded from the call timestamp, so it cannot produce secrets.
// Every token, id, and salt is instead HMAC-SHA256 keyed by the operator's
// signing key over a database-wide counter and the transaction timestamp. The
// counter makes each draw unique; the timestamp separates databases that were
// recreated with the same key.
export function deriveSecret(
  privateKey: Uint8Array,
  counter: bigint,
  micros: bigint
): Uint8Array {
  return hmacSha256(
    privateKey,
    utf8.encode(`spacetimedb-auth secret ${counter} ${micros}`)
  );
}
