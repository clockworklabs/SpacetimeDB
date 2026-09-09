import { p256 } from '@noble/curves/nist.js';

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder('utf-8');

const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const B64_REV = (() => {
  const m = new Int8Array(256).fill(-1);
  for (let i = 0; i < B64.length; i++) m[B64.charCodeAt(i)] = i;
  return m;
})();

function b64uEncode(bytes: Uint8Array): string {
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
    } else {
      out += B64[((bytes[i] & 3) << 4) | (bytes[i + 1] >> 4)];
      out += B64[(bytes[i + 1] & 15) << 2];
    }
  }
  return out.replace(/\+/g, '-').replace(/\//g, '_');
}

function b64uDecode(str: string): Uint8Array {
  let s = '';
  for (let i = 0; i < str.length; i++) {
    const c = str[i] === '-' ? '+' : str[i] === '_' ? '/' : str[i];
    if (B64_REV[c.charCodeAt(0)] >= 0) s += c;
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

function b64uJson(obj: unknown): string {
  return b64uEncode(textEncoder.encode(JSON.stringify(obj)));
}

export interface JwtHeader {
  alg: 'ES256';
  typ: 'JWT';
  kid?: string;
}

export interface JwtClaims {
  iss: string;
  sub: string;
  aud?: string | string[];
  iat: number;
  exp: number;
  nbf?: number;
  jti?: string;
  [k: string]: unknown;
}

/** Sign a JWT with ES256. privateKey is 32 raw bytes. */
export function signJwt(
  privateKey: Uint8Array,
  claims: JwtClaims,
  kid?: string
): string {
  const header: JwtHeader = { alg: 'ES256', typ: 'JWT' };
  if (kid) header.kid = kid;
  const headPart = b64uJson(header);
  const payloadPart = b64uJson(claims);
  const signingInput = `${headPart}.${payloadPart}`;
  const sig = p256.sign(textEncoder.encode(signingInput), privateKey);
  return `${signingInput}.${b64uEncode(sig)}`;
}

export interface VerifyJwtOptions {
  issuer?: string;
  audience?: string;
  /** Default 60. */
  clockToleranceSeconds?: number;
  /** Default Date.now()/1000. */
  nowSeconds?: number;
}

export type VerifyResult =
  | { ok: true; claims: JwtClaims; header: JwtHeader }
  | {
      ok: false;
      reason:
        | 'malformed'
        | 'bad-signature'
        | 'expired'
        | 'not-yet-valid'
        | 'bad-issuer'
        | 'bad-audience';
    };

/** publicKey: 65-byte uncompressed P-256 key. */
export function verifyJwt(
  publicKey: Uint8Array,
  token: string,
  opts: VerifyJwtOptions = {}
): VerifyResult {
  const parts = token.split('.');
  if (parts.length !== 3) return { ok: false, reason: 'malformed' };
  const [headPart, payloadPart, sigPart] = parts;

  let header: JwtHeader;
  let claims: JwtClaims;
  try {
    header = JSON.parse(textDecoder.decode(b64uDecode(headPart)));
    claims = JSON.parse(textDecoder.decode(b64uDecode(payloadPart)));
  } catch {
    return { ok: false, reason: 'malformed' };
  }
  if (header.alg !== 'ES256') return { ok: false, reason: 'bad-signature' };

  let sig: Uint8Array;
  try {
    sig = b64uDecode(sigPart);
  } catch {
    return { ok: false, reason: 'malformed' };
  }
  if (sig.length !== 64) return { ok: false, reason: 'bad-signature' };

  let ok = false;
  try {
    ok = p256.verify(
      sig,
      textEncoder.encode(`${headPart}.${payloadPart}`),
      publicKey
    );
  } catch {
    ok = false;
  }
  if (!ok) return { ok: false, reason: 'bad-signature' };

  const now = opts.nowSeconds ?? Math.floor(Date.now() / 1000);
  const skew = opts.clockToleranceSeconds ?? 60;
  if (typeof claims.exp === 'number' && claims.exp + skew < now) {
    return { ok: false, reason: 'expired' };
  }
  if (typeof claims.nbf === 'number' && claims.nbf - skew > now) {
    return { ok: false, reason: 'not-yet-valid' };
  }
  if (opts.issuer != null && claims.iss !== opts.issuer) {
    return { ok: false, reason: 'bad-issuer' };
  }
  if (opts.audience != null) {
    const aud = claims.aud;
    const matches = Array.isArray(aud)
      ? aud.includes(opts.audience)
      : aud === opts.audience;
    if (!matches) return { ok: false, reason: 'bad-audience' };
  }
  return { ok: true, claims, header };
}

/** Unsafe: no verification. Use only on trusted input. */
export function decodeJwtPayloadUnsafe(token: string): JwtClaims | null {
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  try {
    return JSON.parse(textDecoder.decode(b64uDecode(parts[1])));
  } catch {
    return null;
  }
}
