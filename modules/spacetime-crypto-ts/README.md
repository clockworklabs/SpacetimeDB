# @spacetimedb/crypto

Hash data, create HMAC signatures, and verify webhook signatures in SpacetimeDB.
The package also includes byte comparison and hex and base64 conversion helpers.

## Install

```bash
npm install @spacetimedb/crypto
```

## Verify a webhook

In your HTTP handler, verify a Stripe webhook with the raw request body and
the module's timestamp:

```ts
import { SyncResponse } from 'spacetimedb/server';
import { verifyStripeSignature } from '@spacetimedb/crypto';

const result = verifyStripeSignature({
  rawBody,
  signatureHeader: request.headers.get('stripe-signature') ?? '',
  secret: webhookSecret,
  nowSeconds: Number(ctx.timestamp.microsSinceUnixEpoch / 1_000_000n),
});
if (!result.ok) {
  return new SyncResponse(result.reason, { status: 400 });
}
```

Verify the raw webhook body before parsing it. Store webhook secrets in private
tables and keep them out of public rows and procedure results.

## API

- `sha256(data)` and `hmacSha256(key, message)` return `Uint8Array` digests.
- `timingSafeEqual(a, b)` compares every byte in equal-length arrays.
- `hexToBytes`, `bytesToHex`, and `base64ToBytes` convert common encodings.
  Base64 accepts canonical padded or unpadded standard encoding and rejects
  whitespace, misplaced padding, invalid lengths, and non-canonical pad bits.
- `verifyStripeSignature(options)` verifies Stripe's `v1` signature format.
- `verifySvixSignature(options)` verifies Svix-compatible signatures, including
  Resend webhooks.
- `verifyGithubSignature(options)` verifies GitHub's SHA-256 webhook signature.

The verifiers return `{ ok: true }` or `{ ok: false, reason }`, with these
`errors` codes:

| Code                                 | Meaning                                                |
| ------------------------------------ | ------------------------------------------------------ |
| `crypto.missing_signature`           | The header has no timestamp or no supported signature. |
| `crypto.invalid_timestamp`           | The signed timestamp is not an integer.                |
| `crypto.timestamp_outside_tolerance` | The timestamp is outside `toleranceSeconds`.           |
| `crypto.invalid_secret`              | The Svix secret is not valid base64.                   |
| `crypto.signature_mismatch`          | No signature matches the body and secret.              |

Package entrypoints:

- `@spacetimedb/crypto` exports hashing, encoding, comparison, and vendor
  verification helpers.
- `@spacetimedb/crypto/vendors` is the focused webhook-verification
  entrypoint.

## Testing

```bash
pnpm test
pnpm run lint
```

Tests use published vendor vectors and local fixtures; no network is required.

## License

Apache-2.0. See [`LICENSE.txt`](./LICENSE.txt).
