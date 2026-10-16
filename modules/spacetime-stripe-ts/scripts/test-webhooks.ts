import * as assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { Timestamp } from 'spacetimedb';
import { ingestStripeWebhook } from '../src/submodule/operations';

// Signed and handled, but fails payload validation.
const webhookSecret = 'whsec_unit';
const nowSeconds = 1_800_000_000;
const invalidEventJson = JSON.stringify({
  id: 'evt_invalid',
  type: 'customer.created',
  livemode: false,
  created: nowSeconds,
  data: { object: {} },
});
const invalidEventSignature = `t=${nowSeconds},v1=${createHmac('sha256', webhookSecret).update(`${nowSeconds}.${invalidEventJson}`).digest('hex')}`;

// The mock context implements only the tables the reducer reads and writes.
type EventRow = { eventId: string; status: { tag: string } };
const webhookEvents = new Map<string, EventRow>();
const tx = {
  timestamp: new Timestamp(BigInt(nowSeconds) * 1_000_000n),
  db: {
    stripeConfig: {
      singleton: { find: () => ({ webhookSigningSecret: webhookSecret }) },
    },
    stripeWebhookEvent: {
      eventId: {
        find: (id: string) => webhookEvents.get(id),
        update: (row: EventRow) => webhookEvents.set(row.eventId, row),
      },
      insert: (row: EventRow) => webhookEvents.set(row.eventId, row),
    },
  },
};
const ingest = (signatureHeader: string) =>
  ingestStripeWebhook(tx as never, {
    eventId: 'evt_invalid',
    eventType: 'customer.created',
    livemode: false,
    payloadJson: invalidEventJson,
    signatureHeader,
  });

assert.throws(
  () => ingest(`t=${nowSeconds},v1=${'0'.repeat(64)}`),
  /stripe\.webhook_signature_mismatch/
);
assert.equal(webhookEvents.size, 0);
// The reducer returns so the Failed row commits.
ingest(invalidEventSignature);
assert.equal(webhookEvents.get('evt_invalid')?.status.tag, 'Failed');

process.stdout.write('stripe webhook tests passed\n');
