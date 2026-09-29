// Reference docs:
//   Stripe:  https://docs.stripe.com/webhooks/signatures
//   Resend (svix): https://docs.svix.com/receiving/verifying-payloads/how-manual
//   GitHub:  https://docs.github.com/en/webhooks/using-webhooks/validating-webhook-deliveries

import { hmacSha256 } from './hmac';
import { timingSafeEqual, hexToBytes, base64ToBytes } from './timing';

const enc = new TextEncoder();

/** Reasons a webhook signature check fails. */
export const errors = {
  missingSignature: 'crypto.missing_signature',
  invalidTimestamp: 'crypto.invalid_timestamp',
  timestampOutsideTolerance: 'crypto.timestamp_outside_tolerance',
  invalidSecret: 'crypto.invalid_secret',
  signatureMismatch: 'crypto.signature_mismatch',
} as const;

export type VerifyFailure = (typeof errors)[keyof typeof errors];

export type VerifyResult = { ok: true } | { ok: false; reason: VerifyFailure };

const verified: VerifyResult = { ok: true };
const failed = (reason: VerifyFailure): VerifyResult => ({ ok: false, reason });

function checkTimestamp(
  value: string,
  toleranceSeconds: number,
  nowSeconds: number | undefined
): VerifyResult {
  const t = Number.parseInt(value, 10);
  if (!Number.isFinite(t)) return failed(errors.invalidTimestamp);
  const now = nowSeconds ?? Math.floor(Date.now() / 1000);
  if (toleranceSeconds !== Infinity && Math.abs(now - t) > toleranceSeconds) {
    return failed(errors.timestampOutsideTolerance);
  }
  return verified;
}

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
 * Verify a Stripe webhook signature. Succeeds when the signature header
 * contains at least one valid v1 signature and the timestamp is within
 * tolerance.
 */
export function verifyStripeSignature(opts: StripeVerifyOpts): VerifyResult {
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
  if (!tStr || !v1List) return failed(errors.missingSignature);
  const timestamp = checkTimestamp(
    tStr,
    opts.toleranceSeconds ?? 300,
    opts.nowSeconds
  );
  if (!timestamp.ok) return timestamp;

  const signed = enc.encode(`${tStr}.${opts.rawBody}`);
  const expected = hmacSha256(enc.encode(opts.secret), signed);

  for (const v1Hex of v1List) {
    let candidate: Uint8Array;
    try {
      candidate = hexToBytes(v1Hex);
    } catch {
      continue;
    }
    if (timingSafeEqual(expected, candidate)) return verified;
  }
  return failed(errors.signatureMismatch);
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

/** Verify a Svix webhook signature, as sent by Resend, Clerk, and others. */
export function verifySvixSignature(opts: SvixVerifyOpts): VerifyResult {
  const timestamp = checkTimestamp(
    opts.svixTimestamp,
    opts.toleranceSeconds ?? 300,
    opts.nowSeconds
  );
  if (!timestamp.ok) return timestamp;

  // Strip the `whsec_` prefix, base64-decode the rest.
  const secretBody = opts.secret.startsWith('whsec_')
    ? opts.secret.slice('whsec_'.length)
    : opts.secret;
  let secretBytes: Uint8Array;
  try {
    secretBytes = base64ToBytes(secretBody);
  } catch {
    return failed(errors.invalidSecret);
  }

  const signed = enc.encode(
    `${opts.svixId}.${opts.svixTimestamp}.${opts.rawBody}`
  );
  const expected = hmacSha256(secretBytes, signed);

  // Header is `v1,<base64sig> v1,<base64sig> ...`. Any match wins.
  const signatures = opts.svixSignature
    .split(' ')
    .filter(part => part.startsWith('v1,'));
  if (signatures.length === 0) return failed(errors.missingSignature);
  for (const part of signatures) {
    let candidate: Uint8Array;
    try {
      candidate = base64ToBytes(part.slice('v1,'.length));
    } catch {
      continue;
    }
    if (timingSafeEqual(expected, candidate)) return verified;
  }
  return failed(errors.signatureMismatch);
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
export function verifyGithubSignature(opts: GithubVerifyOpts): VerifyResult {
  const prefix = 'sha256=';
  if (!opts.signatureHeader.startsWith(prefix)) {
    return failed(errors.missingSignature);
  }
  let candidate: Uint8Array;
  try {
    candidate = hexToBytes(opts.signatureHeader.slice(prefix.length));
  } catch {
    return failed(errors.signatureMismatch);
  }
  const expected = hmacSha256(
    enc.encode(opts.secret),
    enc.encode(opts.rawBody)
  );
  return timingSafeEqual(expected, candidate)
    ? verified
    : failed(errors.signatureMismatch);
}
