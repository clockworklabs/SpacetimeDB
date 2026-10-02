import { p256 } from '@noble/curves/nist.js';
import { base64UrlDecode, base64UrlEncode, utf8 } from './crypto.js';

const textDecoder = new TextDecoder('utf-8');

function b64uJson(obj: unknown): string {
  return base64UrlEncode(utf8.encode(JSON.stringify(obj)));
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
  const sig = p256.sign(utf8.encode(signingInput), privateKey);
  return `${signingInput}.${base64UrlEncode(sig)}`;
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
    header = JSON.parse(textDecoder.decode(base64UrlDecode(headPart)));
    claims = JSON.parse(textDecoder.decode(base64UrlDecode(payloadPart)));
  } catch {
    return { ok: false, reason: 'malformed' };
  }
  if (header.alg !== 'ES256') return { ok: false, reason: 'bad-signature' };

  let sig: Uint8Array;
  try {
    sig = base64UrlDecode(sigPart);
  } catch {
    return { ok: false, reason: 'malformed' };
  }
  if (sig.length !== 64) return { ok: false, reason: 'bad-signature' };

  let ok = false;
  try {
    ok = p256.verify(sig, utf8.encode(`${headPart}.${payloadPart}`), publicKey);
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
