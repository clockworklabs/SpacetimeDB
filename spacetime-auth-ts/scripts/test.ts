// Pure-Node sanity tests. No STDB needed. Run: pnpm run test
// Covers: keys, jwt, crypto.

import { p256 } from '@noble/curves/nist.js';
import type { Request } from 'spacetimedb/server';

import {
  generateEs256Keypair,
  fromPrivateKeyBytes,
  privateKeyFromPem,
  publicKeyFromPem,
} from '../src/keys.ts';
import { signJwt, verifyJwt, decodeJwtPayloadUnsafe } from '../src/jwt.ts';
import {
  hashPassword,
  verifyPassword,
  randomToken,
  randomBytes,
  uuidV7,
  pkceChallenge,
  newPkceVerifier,
} from '../src/crypto.ts';
import {
  clientKey,
  safeRedirectPath,
  shouldUseSecureCookies,
  userAgent,
} from '../src/request-trust.ts';

let pass = 0;
let fail = 0;

function ok(name: string): void {
  pass++;
  process.stdout.write(`  ok   ${name}\n`);
}

function err(name: string, detail: string): void {
  fail++;
  process.stdout.write(`  FAIL ${name}\n       ${detail}\n`);
}

function assert(cond: boolean, name: string, detail = ''): void {
  if (cond) ok(name);
  else err(name, detail);
}

function assertEq(actual: unknown, expected: unknown, name: string): void {
  if (actual === expected) ok(name);
  else err(name, `expected ${String(expected)}, got ${String(actual)}`);
}

function bytesEq(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

// Test-only RandomSource compatible with STDB Random.

const TEST_RNG: { fill<T extends Uint8Array>(a: T): T } = {
  fill<T extends Uint8Array>(a: T): T {
    for (let i = 0; i < a.length; i++) a[i] = Math.floor(Math.random() * 256);
    return a;
  },
};

process.stdout.write('\nhttp trust\n');

{
  const headers = new Map([
    ['x-forwarded-for', '203.0.113.10, 10.0.0.2'],
    ['x-real-ip', '198.51.100.4'],
  ]);
  const req = {
    headers: {
      get(name: string) {
        return headers.get(name.toLowerCase()) ?? null;
      },
    },
  } as unknown as Request;
  assertEq(
    clientKey(req),
    undefined,
    'proxy headers ignored unless configured'
  );
  assertEq(
    clientKey(req, 'x-forwarded-for'),
    '203.0.113.10',
    'trusted forwarded header uses first address'
  );
  assertEq(
    clientKey(req, 'x-real-ip'),
    '198.51.100.4',
    'trusted real IP is accepted'
  );
  assertEq(userAgent(req), undefined, 'missing user agent is omitted');
  const longUserAgentReq = {
    headers: {
      get: (name: string) => (name === 'user-agent' ? 'x'.repeat(513) : null),
    },
  } as unknown as Request;
  assertEq(
    userAgent(longUserAgentReq),
    undefined,
    'oversized user agent is omitted'
  );
  assertEq(shouldUseSecureCookies(), true, 'cookies are secure by default');
  assertEq(
    shouldUseSecureCookies(false),
    false,
    'local HTTP can opt out explicitly'
  );
  assertEq(
    safeRedirectPath('/dashboard?tab=billing'),
    '/dashboard?tab=billing',
    'relative redirect accepted'
  );
  for (const redirect of [
    'https://attacker.example',
    '//attacker.example',
    '/%2f%2fattacker.example',
    '/\\attacker.example',
    '/%5cattacker.example',
    '/ok%0d%0alocation:%20https://attacker.example',
    '/ok\u007fblocked',
    '/path#fragment',
  ]) {
    assertEq(
      safeRedirectPath(redirect),
      undefined,
      `unsafe redirect rejected: ${redirect}`
    );
  }
}

// keys

process.stdout.write('\nkeys\n');

{
  const kp = generateEs256Keypair();
  assertEq(kp.privateKey.length, 32, 'private key is 32 bytes');
  assertEq(kp.publicKey.length, 65, 'public key is 65 bytes uncompressed');
  assertEq(kp.publicKey[0], 0x04, 'public key starts with 0x04');
  assert(
    kp.publicKeyPem.includes('BEGIN PUBLIC KEY'),
    'public PEM has BEGIN PUBLIC KEY'
  );
  assert(
    kp.privateKeyPem.includes('BEGIN PRIVATE KEY'),
    'private PEM has BEGIN PRIVATE KEY'
  );
  assert(kp.kid.length > 0, 'kid (JWK thumbprint) is non-empty');
  assertEq(kp.publicKeyJwk.kty, 'EC', 'JWK kty is EC');
  assertEq(kp.publicKeyJwk.crv, 'P-256', 'JWK crv is P-256');
  assertEq(kp.publicKeyJwk.alg, 'ES256', 'JWK alg is ES256');

  // Re-derive public from stored private.
  const kp2 = fromPrivateKeyBytes(kp.privateKey);
  assert(
    bytesEq(kp.publicKey, kp2.publicKey),
    'public key re-derives from private'
  );
  assertEq(kp.kid, kp2.kid, 'kid is stable across re-derivation');

  // PEM round-trip private.
  const decodedPriv = privateKeyFromPem(kp.privateKeyPem);
  assert(
    bytesEq(kp.privateKey, decodedPriv),
    'private key round-trips through PEM'
  );

  // PEM round-trip public.
  const decodedPub = publicKeyFromPem(kp.publicKeyPem);
  assert(
    bytesEq(kp.publicKey, decodedPub),
    'public key round-trips through PEM'
  );
}

// jwt

process.stdout.write('\njwt\n');

{
  const kp = generateEs256Keypair();
  const now = Math.floor(Date.now() / 1000);

  const token = signJwt(
    kp.privateKey,
    {
      iss: 'https://auth.example.com',
      sub: 'user-1234',
      aud: 'https://auth.example.com',
      iat: now,
      exp: now + 3600,
      jti: 'session-1',
    },
    kp.kid
  );
  assertEq(token.split('.').length, 3, 'JWT has three parts');

  // Header decodes correctly.
  const header = JSON.parse(
    new TextDecoder().decode(base64urlDecode(token.split('.')[0]))
  );
  assertEq(header.alg, 'ES256', 'header alg is ES256');
  assertEq(header.typ, 'JWT', 'header typ is JWT');
  assertEq(header.kid, kp.kid, 'header kid matches');

  // Roundtrip verify.
  const v = verifyJwt(kp.publicKey, token, {
    issuer: 'https://auth.example.com',
    audience: 'https://auth.example.com',
  });
  assert(v.ok, 'sign+verify roundtrip with same key');
  if (v.ok) {
    assertEq(v.claims.sub, 'user-1234', 'verified claims.sub');
    assertEq(v.claims.iss, 'https://auth.example.com', 'verified claims.iss');
  }

  // Wrong key fails.
  const otherKp = generateEs256Keypair();
  const v2 = verifyJwt(otherKp.publicKey, token);
  assert(
    !v2.ok && v2.reason === 'bad-signature',
    'wrong key fails with bad-signature'
  );

  // Expired token fails.
  const expired = signJwt(kp.privateKey, {
    iss: 'x',
    sub: 'y',
    iat: now - 7200,
    exp: now - 3600,
  });
  const v3 = verifyJwt(kp.publicKey, expired);
  assert(!v3.ok && v3.reason === 'expired', 'expired token fails with expired');

  // Wrong issuer fails.
  const v4 = verifyJwt(kp.publicKey, token, {
    issuer: 'https://wrong.example.com',
  });
  assert(
    !v4.ok && v4.reason === 'bad-issuer',
    'wrong issuer fails with bad-issuer'
  );

  // Wrong audience fails.
  const v5 = verifyJwt(kp.publicKey, token, {
    audience: 'https://wrong.example.com',
  });
  assert(
    !v5.ok && v5.reason === 'bad-audience',
    'wrong audience fails with bad-audience'
  );

  // Malformed token fails.
  const v6 = verifyJwt(kp.publicKey, 'not.a.jwt');
  assert(
    !v6.ok && v6.reason === 'malformed',
    'malformed token fails with malformed'
  );

  // Decode-unsafe extracts payload.
  const payload = decodeJwtPayloadUnsafe(token);
  assertEq(payload?.sub, 'user-1234', 'decodeJwtPayloadUnsafe returns sub');

  // Signature is 64 bytes (compact r||s).
  const sigBytes = base64urlDecode(token.split('.')[2]);
  assertEq(sigBytes.length, 64, 'ES256 signature is 64 bytes (r||s)');

  // Verify with noble directly to cross-check.
  const signingInput = `${token.split('.')[0]}.${token.split('.')[1]}`;
  const directOk = p256.verify(
    sigBytes,
    new TextEncoder().encode(signingInput),
    kp.publicKey
  );
  assert(directOk, 'noble verifies the produced signature directly');
}

// crypto

process.stdout.write('\ncrypto\n');

{
  // Password hash + verify roundtrip. scrypt is SLOW, this takes ~1s.
  const password = 'correct horse battery staple';
  // Low N so tests don't take forever.
  const hash = hashPassword(TEST_RNG, password, { N: 1 << 10 });
  assert(hash.startsWith('scrypt$1024$'), 'hashPassword encodes scrypt params');
  assert(
    verifyPassword(password, hash),
    'verifyPassword accepts correct password'
  );
  assert(
    !verifyPassword('wrong', hash),
    'verifyPassword rejects wrong password'
  );

  // Random token shape.
  const token = randomToken(TEST_RNG, 32);
  assert(/^[A-Za-z0-9_-]+$/.test(token), 'randomToken is base64url-safe');
  assert(token.length >= 40, 'randomToken has enough entropy bits');

  // Random bytes length.
  const bytes = randomBytes(TEST_RNG, 16);
  assertEq(bytes.length, 16, 'randomBytes returns requested length');

  // UUIDv7 shape.
  const id = uuidV7(TEST_RNG, Date.now());
  assert(
    /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(
      id
    ),
    'uuidV7 matches v7 regex'
  );

  // PKCE challenge.
  const verifier = newPkceVerifier(TEST_RNG);
  const challenge = pkceChallenge(verifier);
  assert(/^[A-Za-z0-9_-]+$/.test(challenge), 'pkceChallenge is base64url-safe');
  assertEq(
    challenge.length,
    43,
    'pkceChallenge is 43 chars (SHA-256 base64url, no pad)'
  );
}

// summary

process.stdout.write(`\n${pass} passed, ${fail} failed\n`);
process.exit(fail === 0 ? 0 : 1);

// helpers

function base64urlDecode(s: string): Uint8Array {
  const pad = s.length % 4 === 0 ? '' : '='.repeat(4 - (s.length % 4));
  const bin = atob(s.replace(/-/g, '+').replace(/_/g, '/') + pad);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
