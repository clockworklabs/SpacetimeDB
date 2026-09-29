import * as assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { Timestamp } from 'spacetimedb';
import { SenderError } from 'spacetimedb/server';
import {
  ingestResendWebhook,
  makeResendWebhookHandler,
} from '../src/submodule/webhooks';

const secretRaw = 'resend_unit_test_secret';
const secret = `whsec_${Buffer.from(secretRaw).toString('base64')}`;
const nowSeconds = 1_800_000_000;
const timestamp = String(nowSeconds);
const payloadJson = JSON.stringify({
  type: 'contact.created',
  created_at: '2027-01-15T08:00:00Z',
  data: { id: 'contact_1' },
});

function sign(eventId: string): string {
  const digest = createHmac('sha256', secretRaw)
    .update(`${eventId}.${timestamp}.${payloadJson}`)
    .digest('base64');
  return `v1,${digest}`;
}

// The mock context implements only the tables these paths read and write.
const webhookEvents = new Map<string, { eventId: string }>();
const tx = {
  timestamp: new Timestamp(BigInt(nowSeconds) * 1_000_000n),
  db: {
    resendConfig: {
      singleton: { find: () => ({ webhookSigningSecret: secret }) },
    },
    resendWebhookEvent: {
      eventId: {
        find: (id: string) => webhookEvents.get(id),
        update: (row: { eventId: string }) =>
          webhookEvents.set(row.eventId, row),
      },
      insert: (row: { eventId: string }) => webhookEvents.set(row.eventId, row),
    },
  },
};

const handler = makeResendWebhookHandler();
function post(eventId: string, signature: string) {
  const headers = new Map([
    ['svix-id', eventId],
    ['svix-timestamp', timestamp],
    ['svix-signature', signature],
  ]);
  const req = {
    method: 'POST',
    headers: { get: (name: string) => headers.get(name) ?? null },
    text: () => payloadJson,
  };
  return handler(
    { withTx: (fn: (ctx: typeof tx) => unknown) => fn(tx) } as never,
    req as never
  );
}

const rejected = post('evt_bad_http', sign('evt_other'));
assert.equal(rejected.status, 401);
assert.match(rejected.text(), /resend\.webhook_signature_mismatch/);
assert.equal(webhookEvents.size, 0);

assert.equal(post('evt_good_http', sign('evt_good_http')).status, 200);
assert.ok(webhookEvents.has('evt_good_http'));

const ingest = (eventId: string, signature: string) =>
  ingestResendWebhook(tx as never, {
    eventId,
    eventType: 'contact.created',
    payloadJson,
    signatureHeader: signature,
    timestampHeader: timestamp,
  });

assert.throws(
  () => ingest('evt_bad_reducer', sign('evt_other')),
  (error: unknown) =>
    error instanceof SenderError &&
    /resend\.webhook_signature_mismatch/.test(error.message)
);
assert.ok(!webhookEvents.has('evt_bad_reducer'));
ingest('evt_good_reducer', sign('evt_good_reducer'));
assert.ok(webhookEvents.has('evt_good_reducer'));

process.stdout.write('resend webhook tests passed\n');
