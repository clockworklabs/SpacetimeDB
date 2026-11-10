import { p256 } from '@noble/curves/nist.js';
import { base64ToBytes, sha256 } from '@spacetimedb/crypto';
import { base64Encode, base64UrlEncode, utf8 } from './crypto.js';

const PRIV_LEN = 32;
const COORD_LEN = 32;

export interface Es256Keypair {
  privateKey: Uint8Array;
  publicKey: Uint8Array;
  privateKeyPem: string;
  publicKeyPem: string;
  publicKeyJwk: PublicKeyJwk;
  kid: string;
}

export interface PublicKeyJwk {
  kty: 'EC';
  crv: 'P-256';
  x: string;
  y: string;
  alg: 'ES256';
  use: 'sig';
  kid?: string;
}

export function fromPrivateKeyBytes(privateKey: Uint8Array): Es256Keypair {
  if (privateKey.length !== PRIV_LEN) {
    throw new TypeError(`ES256 private key must be ${PRIV_LEN} bytes`);
  }
  const publicKey = p256.getPublicKey(privateKey, false);
  return assemble(privateKey, publicKey);
}

function assemble(privateKey: Uint8Array, publicKey: Uint8Array): Es256Keypair {
  const { x, y } = splitUncompressedPublicKey(publicKey);
  const publicKeyJwk: PublicKeyJwk = {
    kty: 'EC',
    crv: 'P-256',
    alg: 'ES256',
    use: 'sig',
    x: base64UrlEncode(x),
    y: base64UrlEncode(y),
  };
  const kid = jwkThumbprint(publicKeyJwk);
  publicKeyJwk.kid = kid;
  return {
    privateKey,
    publicKey,
    privateKeyPem: encodePrivateKeyPem(privateKey, publicKey),
    publicKeyPem: encodePublicKeyPem(publicKey),
    publicKeyJwk,
    kid,
  };
}

function splitUncompressedPublicKey(pub: Uint8Array): {
  x: Uint8Array;
  y: Uint8Array;
} {
  if (pub.length !== 1 + COORD_LEN * 2 || pub[0] !== 0x04) {
    throw new TypeError('expected uncompressed P-256 public key');
  }
  return { x: pub.slice(1, 1 + COORD_LEN), y: pub.slice(1 + COORD_LEN) };
}

/** RFC 7638. */
function jwkThumbprint(jwk: PublicKeyJwk): string {
  const canonical = JSON.stringify({
    crv: jwk.crv,
    kty: jwk.kty,
    x: jwk.x,
    y: jwk.y,
  });
  return base64UrlEncode(sha256(utf8.encode(canonical)));
}

// SPKI ECDSA P-256 algorithm OID prefix.
const SPKI_ALG_DER = new Uint8Array([
  0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08,
  0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07,
]);

function encodePublicKeyPem(publicKey65: Uint8Array): string {
  const bitString = concat([
    new Uint8Array([0x03, publicKey65.length + 1, 0x00]),
    publicKey65,
  ]);
  const body = concat([SPKI_ALG_DER, bitString]);
  const der = wrapSequence(body);
  return pemWrap('PUBLIC KEY', der);
}

function encodePrivateKeyPem(
  privateKey: Uint8Array,
  publicKey65: Uint8Array
): string {
  // RFC 5915 ECPrivateKey wrapped in PKCS#8 PrivateKeyInfo.
  const version = new Uint8Array([0x02, 0x01, 0x01]);
  const privOctet = concat([
    new Uint8Array([0x04, privateKey.length]),
    privateKey,
  ]);
  const namedCurveBody = new Uint8Array([
    0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07,
  ]);
  const namedCurveTagged = concat([
    new Uint8Array([0xa0, namedCurveBody.length]),
    namedCurveBody,
  ]);
  const pubBitString = concat([
    new Uint8Array([0x03, publicKey65.length + 1, 0x00]),
    publicKey65,
  ]);
  const pubTagged = concat([
    new Uint8Array([0xa1, pubBitString.length]),
    pubBitString,
  ]);
  const ecPrivBody = concat([version, privOctet, namedCurveTagged, pubTagged]);
  const ecPrivDer = wrapSequence(ecPrivBody);

  const p8version = new Uint8Array([0x02, 0x01, 0x00]);
  const p8alg = SPKI_ALG_DER;
  const p8privOctet = concat([
    encodeLengthPrefix(0x04, ecPrivDer.length),
    ecPrivDer,
  ]);
  const p8body = concat([p8version, p8alg, p8privOctet]);
  const p8der = wrapSequence(p8body);
  return pemWrap('PRIVATE KEY', p8der);
}

function wrapSequence(body: Uint8Array): Uint8Array {
  return concat([encodeLengthPrefix(0x30, body.length), body]);
}

function encodeLengthPrefix(tag: number, len: number): Uint8Array {
  if (len < 0x80) return new Uint8Array([tag, len]);
  if (len < 0x100) return new Uint8Array([tag, 0x81, len]);
  if (len < 0x10000)
    return new Uint8Array([tag, 0x82, (len >> 8) & 0xff, len & 0xff]);
  throw new RangeError('DER length too large for this encoder');
}

function concat(arrays: Uint8Array[]): Uint8Array {
  let n = 0;
  for (const a of arrays) n += a.length;
  const out = new Uint8Array(n);
  let off = 0;
  for (const a of arrays) {
    out.set(a, off);
    off += a.length;
  }
  return out;
}

function pemWrap(label: string, der: Uint8Array): string {
  const b64 = base64Encode(der);
  const lines: string[] = [];
  for (let i = 0; i < b64.length; i += 64) lines.push(b64.slice(i, i + 64));
  return `-----BEGIN ${label}-----\n${lines.join('\n')}\n-----END ${label}-----\n`;
}

function pemUnwrap(label: string, pem: string): Uint8Array {
  const prefix = `-----BEGIN ${label}-----`;
  const suffix = `-----END ${label}-----`;
  const start = pem.indexOf(prefix);
  const end = pem.indexOf(suffix);
  if (start < 0 || end < 0) throw new TypeError(`PEM ${label} block not found`);
  const inner = pem.slice(start + prefix.length, end).replace(/\s+/g, '');
  return base64ToBytes(inner);
}

export function privateKeyFromPem(pem: string): Uint8Array {
  const der = pemUnwrap('PRIVATE KEY', pem);
  for (let i = 0; i < der.length - 33; i++) {
    if (der[i] === 0x04 && der[i + 1] === 0x20) {
      return der.slice(i + 2, i + 2 + 32);
    }
  }
  throw new TypeError('could not extract raw private key from PKCS#8 PEM');
}

export function publicKeyFromPem(pem: string): Uint8Array {
  const der = pemUnwrap('PUBLIC KEY', pem);
  for (let i = 0; i < der.length - 67; i++) {
    if (
      der[i] === 0x03 &&
      der[i + 1] === 0x42 &&
      der[i + 2] === 0x00 &&
      der[i + 3] === 0x04
    ) {
      return der.slice(i + 3, i + 68);
    }
  }
  throw new TypeError('could not extract raw public key from SPKI PEM');
}
