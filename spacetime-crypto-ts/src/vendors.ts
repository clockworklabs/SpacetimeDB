// Vendor-specific webhook signature verifiers. Each wraps hmacSha256 +
// timingSafeEqual with the per-vendor framing.
//
// Reference docs:
//   Stripe:  https://docs.stripe.com/webhooks/signatures
//   Resend (svix): https://docs.svix.com/receiving/verifying-payloads/how-manual
//   GitHub:  https://docs.github.com/en/webhooks/using-webhooks/validating-webhook-deliveries

import { hmacSha256 } from './hmac.ts';
import { timingSafeEqual, hexToBytes, base64ToBytes } from './timing.ts';

const enc = new TextEncoder();

// Stripe

export interface StripeVerifyOpts {
  /** The raw request body, exactly as received (do NOT re-stringify JSON). */
  rawBody: string;
  /** Value of the `stripe-signature` request header. */
  signatureHeader: string;
  /** The webhook signing secret, e.g. `whsec_...`. */
  secret: string;
  /**
   * Maximum age of the signed timestamp, in seconds. Stripe recommends 300
   * (5 minutes) to protect against replay. Pass `Infinity` to skip the check
   * (only for tests).
   */
  toleranceSeconds?: number;
  /** Current Unix time in seconds. Defaults to `Date.now()/1000` but the
   *  STDB module runtime should pass `ctx.timestamp` converted to seconds. */
  nowSeconds?: number;
}

/**
 * Verify a Stripe webhook signature. Returns true iff the signature header
 * contains at least one valid v1 signature AND the timestamp is within
 * tolerance.
 */
export function verifyStripeSignature(opts: StripeVerifyOpts): boolean {
  const tolerance = opts.toleranceSeconds ?? 300;
  const now = opts.nowSeconds ?? Math.floor(Date.now() / 1000);

  // Parse "t=1709836800,v1=abcdef,v1=12345..." into a map.
  // Multiple v1 entries are possible after key rotation; any match wins.
  const fields: Record<string, string[]> = {};
  for (const part of opts.signatureHeader.split(',')) {
    const eq = part.indexOf('=');
    if (eq < 0) continue;
    const k = part.slice(0, eq).trim();
    const v = part.slice(eq + 1).trim();
    (fields[k] ??= []).push(v);
  }

  const tStr = fields.t?.[0];
  const v1List = fields.v1;
  if (!tStr || !v1List || v1List.length === 0) return false;

  const t = Number.parseInt(tStr, 10);
  if (!Number.isFinite(t)) return false;
  if (tolerance !== Infinity && Math.abs(now - t) > tolerance) return false;

  const signed = enc.encode(`${tStr}.${opts.rawBody}`);
  const expected = hmacSha256(enc.encode(opts.secret), signed);

  for (const v1Hex of v1List) {
    let candidate: Uint8Array;
    try {
      candidate = hexToBytes(v1Hex);
    } catch {
      continue;
    }
    if (timingSafeEqual(expected, candidate)) return true;
  }
  return false;
}

// Resend webhooks use Svix signatures.

export interface SvixVerifyOpts {
  rawBody: string;
  /** `svix-id` header. */
  svixId: string;
  /** `svix-timestamp` header (Unix seconds as string). */
  svixTimestamp: string;
  /** `svix-signature` header, space-separated list like `v1,base64sig`. */
  svixSignature: string;
  /** Endpoint secret, in the form `whsec_<base64>`. */
  secret: string;
  toleranceSeconds?: number;
  nowSeconds?: number;
}

/**
 * Verify a svix-style webhook signature (Resend, Clerk, FormBricks, …).
 */
export function verifySvixSignature(opts: SvixVerifyOpts): boolean {
  const tolerance = opts.toleranceSeconds ?? 5 * 60;
  const now = opts.nowSeconds ?? Math.floor(Date.now() / 1000);

  const ts = Number.parseInt(opts.svixTimestamp, 10);
  if (!Number.isFinite(ts)) return false;
  if (tolerance !== Infinity && Math.abs(now - ts) > tolerance) return false;

  // Strip the `whsec_` prefix, base64-decode the rest.
  const secretBody = opts.secret.startsWith('whsec_')
    ? opts.secret.slice('whsec_'.length)
    : opts.secret;
  let secretBytes: Uint8Array;
  try {
    secretBytes = base64ToBytes(secretBody);
  } catch {
    return false;
  }

  const signed = enc.encode(
    `${opts.svixId}.${opts.svixTimestamp}.${opts.rawBody}`
  );
  const expected = hmacSha256(secretBytes, signed);

  // Header is `v1,<base64sig> v1,<base64sig> ...`. Any match wins.
  for (const part of opts.svixSignature.split(' ')) {
    const comma = part.indexOf(',');
    if (comma < 0) continue;
    const version = part.slice(0, comma);
    if (version !== 'v1') continue;
    const sigB64 = part.slice(comma + 1);
    let candidate: Uint8Array;
    try {
      candidate = base64ToBytes(sigB64);
    } catch {
      continue;
    }
    if (timingSafeEqual(expected, candidate)) return true;
  }
  return false;
}

// GitHub

export interface GithubVerifyOpts {
  rawBody: string;
  /** `x-hub-signature-256` header, of form `sha256=<hex>`. */
  signatureHeader: string;
  /** Webhook secret as configured in the GitHub repo/org webhook settings. */
  secret: string;
}

/** Verify a GitHub webhook signature (HMAC-SHA256 of body, hex-encoded). */
export function verifyGithubSignature(opts: GithubVerifyOpts): boolean {
  const prefix = 'sha256=';
  if (!opts.signatureHeader.startsWith(prefix)) return false;
  let candidate: Uint8Array;
  try {
    candidate = hexToBytes(opts.signatureHeader.slice(prefix.length));
  } catch {
    return false;
  }
  const expected = hmacSha256(
    enc.encode(opts.secret),
    enc.encode(opts.rawBody)
  );
  return timingSafeEqual(expected, candidate);
}
