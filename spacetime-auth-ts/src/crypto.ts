import { scrypt } from '@noble/hashes/scrypt';
import { sha256 } from '@noble/hashes/sha2';

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder('utf-8');

// N=2^14 keeps single-hash under ~300ms in STDB's V8 isolate.
export interface ScryptParams {
  N: number;
  r: number;
  p: number;
  dkLen: number;
}
const DEFAULT_SCRYPT: ScryptParams = { N: 1 << 14, r: 8, p: 1, dkLen: 32 };

const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const B64_REV = (() => {
  const m = new Int8Array(256).fill(-1);
  for (let i = 0; i < B64.length; i++) m[B64.charCodeAt(i)] = i;
  return m;
})();

function b64encode(bytes: Uint8Array): string {
  let out = '';
  let i = 0;
  for (; i + 2 < bytes.length; i += 3) {
    out += B64[bytes[i] >> 2];
    out += B64[((bytes[i] & 3) << 4) | (bytes[i + 1] >> 4)];
    out += B64[((bytes[i + 1] & 15) << 2) | (bytes[i + 2] >> 6)];
    out += B64[bytes[i + 2] & 63];
  }
  if (i < bytes.length) {
    out += B64[bytes[i] >> 2];
    if (i + 1 === bytes.length) {
      out += B64[(bytes[i] & 3) << 4];
      out += '==';
    } else {
      out += B64[((bytes[i] & 3) << 4) | (bytes[i + 1] >> 4)];
      out += B64[(bytes[i + 1] & 15) << 2];
      out += '=';
    }
  }
  return out;
}

function b64decode(str: string): Uint8Array {
  let s = '';
  for (let i = 0; i < str.length; i++) {
    if (B64_REV[str.charCodeAt(i)] >= 0) s += str[i];
  }
  const out = new Uint8Array((s.length * 3) >> 2);
  let oi = 0;
  for (let i = 0; i + 3 < s.length; i += 4) {
    const a = B64_REV[s.charCodeAt(i)];
    const b = B64_REV[s.charCodeAt(i + 1)];
    const c = B64_REV[s.charCodeAt(i + 2)];
    const d = B64_REV[s.charCodeAt(i + 3)];
    out[oi++] = (a << 2) | (b >> 4);
    out[oi++] = ((b & 15) << 4) | (c >> 2);
    out[oi++] = ((c & 3) << 6) | d;
  }
  const tail = s.length & 3;
  if (tail >= 2) {
    const i = s.length - tail;
    const a = B64_REV[s.charCodeAt(i)];
    const b = B64_REV[s.charCodeAt(i + 1)];
    out[oi++] = (a << 2) | (b >> 4);
    if (tail === 3) {
      const c = B64_REV[s.charCodeAt(i + 2)];
      out[oi++] = ((b & 15) << 4) | (c >> 2);
    }
  }
  return out.subarray(0, oi);
}

/** Subset of STDB Random. */
export interface RandomSource {
  fill<T extends Uint8Array>(array: T): T;
}

export function randomBytes(source: RandomSource, n: number): Uint8Array {
  return source.fill(new Uint8Array(n));
}

export function randomToken(source: RandomSource, byteLen = 32): string {
  return b64encode(randomBytes(source, byteLen))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
}

/** Prefer ctx.newUuidV7() if available. */
export function uuidV7(source: RandomSource, nowMs: number): string {
  const rand = randomBytes(source, 10);
  const ts = BigInt(nowMs);
  const bytes = new Uint8Array(16);
  bytes[0] = Number((ts >> 40n) & 0xffn);
  bytes[1] = Number((ts >> 32n) & 0xffn);
  bytes[2] = Number((ts >> 24n) & 0xffn);
  bytes[3] = Number((ts >> 16n) & 0xffn);
  bytes[4] = Number((ts >> 8n) & 0xffn);
  bytes[5] = Number(ts & 0xffn);
  for (let i = 0; i < 10; i++) bytes[6 + i] = rand[i];
  bytes[6] = (bytes[6] & 0x0f) | 0x70;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/** Encoded as `scrypt$N$r$p$saltB64$hashB64`. */
export function hashPassword(
  source: RandomSource,
  password: string,
  params: Partial<ScryptParams> = {}
): string {
  const p = { ...DEFAULT_SCRYPT, ...params };
  const salt = randomBytes(source, 16);
  const hash = scrypt(textEncoder.encode(password), salt, {
    N: p.N,
    r: p.r,
    p: p.p,
    dkLen: p.dkLen,
  });
  return `scrypt$${p.N}$${p.r}$${p.p}$${b64encode(salt)}$${b64encode(hash)}`;
}

export function verifyPassword(password: string, encoded: string): boolean {
  const parts = encoded.split('$');
  if (parts.length !== 6 || parts[0] !== 'scrypt') return false;
  const N = parseInt(parts[1], 10);
  const r = parseInt(parts[2], 10);
  const p = parseInt(parts[3], 10);
  if (!Number.isFinite(N) || !Number.isFinite(r) || !Number.isFinite(p))
    return false;
  const salt = b64decode(parts[4]);
  const expected = b64decode(parts[5]);
  const actual = scrypt(textEncoder.encode(password), salt, {
    N,
    r,
    p,
    dkLen: expected.length,
  });
  return constantTimeEqual(expected, actual);
}

export function newSessionToken(source: RandomSource): string {
  return randomToken(source, 32);
}

export function newPkceVerifier(source: RandomSource): string {
  return randomToken(source, 32);
}

export function pkceChallenge(verifier: string): string {
  const hash = sha256(textEncoder.encode(verifier));
  return b64encode(hash)
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
}

function constantTimeEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i];
  return diff === 0;
}

export { textEncoder as utf8Encoder, textDecoder as utf8Decoder };
