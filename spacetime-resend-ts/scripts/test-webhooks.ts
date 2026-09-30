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

// Signed and handled, but fails payload validation.
const invalidPayloadJson = JSON.stringify({
  type: 'email.sent',
  created_at: '2027-01-15T08:00:00Z',
  data: {},
});

function sign(eventId: string, body = payloadJson): string {
  const digest = createHmac('sha256', secretRaw)
    .update(`${eventId}.${timestamp}.${body}`)
    .digest('base64');
  return `v1,${digest}`;
}

// The mock context implements only the tables these paths read and write.
type EventRow = { eventId: string; status: { tag: string } };
const webhookEvents = new Map<string, EventRow>();
const tx = {
  timestamp: new Timestamp(BigInt(nowSeconds) * 1_000_000n),
  db: {
    resendConfig: {
      singleton: { find: () => ({ webhookSigningSecret: secret }) },
    },
    resendWebhookEvent: {
      eventId: {
        find: (id: string) => webhookEvents.get(id),
        update: (row: EventRow) => webhookEvents.set(row.eventId, row),
      },
      insert: (row: EventRow) => webhookEvents.set(row.eventId, row),
    },
  },
};

const handler = makeResendWebhookHandler();
function post(eventId: string, signature: string, body = payloadJson) {
  const headers = new Map([
    ['svix-id', eventId],
    ['svix-timestamp', timestamp],
    ['svix-signature', signature],
  ]);
  const req = {
    method: 'POST',
    headers: { get: (name: string) => headers.get(name) ?? null },
    text: () => body,
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

const invalidHttp = post(
  'evt_invalid_http',
  sign('evt_invalid_http', invalidPayloadJson),
  invalidPayloadJson
);
assert.equal(invalidHttp.status, 400);
assert.match(invalidHttp.text(), /resend\.webhook_payload_invalid/);
assert.equal(webhookEvents.get('evt_invalid_http')?.status.tag, 'Failed');

const ingest = (eventId: string, signature: string, body = payloadJson) =>
  ingestResendWebhook(tx as never, {
    eventId,
    eventType: JSON.parse(body).type,
    payloadJson: body,
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

// The reducer returns so the Failed row commits.
ingest(
  'evt_invalid_reducer',
  sign('evt_invalid_reducer', invalidPayloadJson),
  invalidPayloadJson
);
assert.equal(webhookEvents.get('evt_invalid_reducer')?.status.tag, 'Failed');

process.stdout.write('resend webhook tests passed\n');
