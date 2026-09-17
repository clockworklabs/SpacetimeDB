import {
  schema,
  table,
  t,
  Range,
  SenderError,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
} from 'spacetimedb/server';
import * as v from 'valibot';
import { installResend } from './install';

// Webhook delivery state. Received = ingest accepted. Processed = applied.
// Ignored = duplicate/unknown event type. Failed = signature/format error.
export const webhookEventStatus = t.enum('WebhookEventStatus', [
  'Received',
  'Processed',
  'Ignored',
  'Failed',
]);
export const WebhookEventStatus = {
  Received: { tag: 'Received' as const },
  Processed: { tag: 'Processed' as const },
  Ignored: { tag: 'Ignored' as const },
  Failed: { tag: 'Failed' as const },
};

// Lifecycle of a tracked outbound email. Tags match Resend's
// `email.<type>` webhook events for direct ingestion mapping.
export const emailStatus = t.enum('EmailStatus', [
  'Queued',
  'Sent',
  'Delivered',
  'DeliveryDelayed',
  'Bounced',
  'Failed',
  'Cancelled',
]);
export const EmailStatus = {
  Queued: { tag: 'Queued' as const },
  Sent: { tag: 'Sent' as const },
  Delivered: { tag: 'Delivered' as const },
  DeliveryDelayed: { tag: 'DeliveryDelayed' as const },
  Bounced: { tag: 'Bounced' as const },
  Failed: { tag: 'Failed' as const },
  Cancelled: { tag: 'Cancelled' as const },
};
export type EmailStatusValue = (typeof EmailStatus)[keyof typeof EmailStatus];
export type WebhookEventStatusValue =
  (typeof WebhookEventStatus)[keyof typeof WebhookEventStatus];

export const resendEmailRow = {
  resendId: t.string().primaryKey(),
  fromAddress: t.string(),
  toAddressesJson: t.string(),
  subject: t.option(t.string()),
  status: emailStatus,
  lastError: t.option(t.string()),
  bouncedAt: t.option(t.timestamp()),
  bounceJson: t.option(t.string()),
  failedAt: t.option(t.timestamp()),
  failureReason: t.option(t.string()),
  complained: t.bool(),
  complainedAt: t.option(t.timestamp()),
  opened: t.bool(),
  openedAt: t.option(t.timestamp()),
  clicked: t.bool(),
  clickedAt: t.option(t.timestamp()),
  deliveredAt: t.option(t.timestamp()),
  sentAt: t.option(t.timestamp()),
  html: t.option(t.string()),
  text: t.option(t.string()),
  tagsJson: t.option(t.string()),
  userId: t.option(t.string()),
  orgId: t.option(t.string()),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

export const resendDeliveryEventRow = {
  eventId: t.string().primaryKey(),
  resendId: t.string(),
  eventType: t.string(),
  createdAtIso: t.string(),
  detailJson: t.option(t.string()),
  insertedAt: t.timestamp(),
};

export const resendWebhookEventRow = {
  eventId: t.string().primaryKey(),
  eventType: t.string(),
  payloadJson: t.string(),
  signatureHeader: t.option(t.string()),
  timestampHeader: t.option(t.string()),
  status: webhookEventStatus,
  errorMessage: t.option(t.string()),
  receivedAt: t.timestamp(),
  processedAt: t.option(t.timestamp()),
};

// Private singleton; secrets never leak via subscription.
export const resendConfigRow = {
  singleton: t.bool().primaryKey(),
  apiKey: t.string(),
  webhookSigningSecret: t.option(t.string()),
  defaultFrom: t.option(t.string()),
  updatedAt: t.timestamp(),
};

// Fresh publishes seed the owner via init; public procedures never bootstrap admin state.
export const resendAdminIdentityRow = {
  identity: t.identity().primaryKey(),
  addedAtMicros: t.i64(),
};

export const resendEmailTable = table(
  {
    name: 'resend_email',
    public: false,
    indexes: [
      { accessor: 'byUserId', algorithm: 'btree', columns: ['userId'] },
      { accessor: 'byOrgId', algorithm: 'btree', columns: ['orgId'] },
      { accessor: 'byStatus', algorithm: 'btree', columns: ['status'] },
      { accessor: 'byUpdatedAt', algorithm: 'btree', columns: ['updatedAt'] },
    ],
  },
  resendEmailRow
);

export const resendDeliveryEventTable = table(
  {
    name: 'resend_delivery_event',
    public: false,
    indexes: [
      { accessor: 'byResendId', algorithm: 'btree', columns: ['resendId'] },
      {
        accessor: 'byResendIdEventType',
        algorithm: 'btree',
        columns: ['resendId', 'eventType'],
      },
    ],
  },
  resendDeliveryEventRow
);

// Private because the raw payload and signature headers are operational data.
// Host modules can expose an admin-only view when needed.
export const resendWebhookEventTable = table(
  {
    name: 'resend_webhook_event',
    public: false,
    indexes: [
      { accessor: 'byStatus', algorithm: 'btree', columns: ['status'] },
    ],
  },
  resendWebhookEventRow
);

export const resendConfigTable = table(
  { name: 'resend_config', public: false, indexes: [] },
  resendConfigRow
);

export const resendAdminIdentityTable = table(
  { name: 'resend_admin_identity', public: false, indexes: [] },
  resendAdminIdentityRow
);

export const spacetimedb = schema({
  resendEmail: resendEmailTable,
  resendDeliveryEvent: resendDeliveryEventTable,
  resendWebhookEvent: resendWebhookEventTable,
  resendConfig: resendConfigTable,
  resendAdminIdentity: resendAdminIdentityTable,
});

export const init = spacetimedb.init(ctx => {
  installResend(ctx);
});

export default spacetimedb;

export type ReducerModuleCtx = ReducerCtx<typeof spacetimedb.schemaType>;
export type ProcedureModuleCtx = ProcedureCtx<typeof spacetimedb.schemaType>;
export type TransactionModuleCtx = TransactionCtx<
  typeof spacetimedb.schemaType
>;
export type WriteCtx = ReducerModuleCtx | TransactionModuleCtx;
export type ModuleTimestamp = ReducerModuleCtx['timestamp'];

export const sendEmailResult = t.object('SendEmailResult', {
  resendId: t.string(),
});

export const resendHttpResponse = t.object('ResendHttpResponse', {
  status: t.u16(),
  body: t.string(),
});

export { Range, SenderError, t };

// Mirror Resend SDK's BaseEmailEventData.
const vBaseEmailEventData = {
  broadcast_id: v.optional(v.string()),
  created_at: v.string(),
  email_id: v.string(),
  from: v.string(),
  to: v.array(v.string()),
  subject: v.string(),
  template_id: v.optional(v.string()),
  tags: v.optional(v.record(v.string(), v.string())),
};

const vBaseEvent = {
  created_at: v.string(),
};

export const vEmailEvent = v.variant('type', [
  v.object({
    ...vBaseEvent,
    type: v.literal('email.sent'),
    data: v.object(vBaseEmailEventData),
  }),
  v.object({
    ...vBaseEvent,
    type: v.literal('email.delivered'),
    data: v.object(vBaseEmailEventData),
  }),
  v.object({
    ...vBaseEvent,
    type: v.literal('email.delivery_delayed'),
    data: v.object(vBaseEmailEventData),
  }),
  v.object({
    ...vBaseEvent,
    type: v.literal('email.complained'),
    data: v.object(vBaseEmailEventData),
  }),
  v.object({
    ...vBaseEvent,
    type: v.literal('email.bounced'),
    data: v.object({
      ...vBaseEmailEventData,
      bounce: v.object({
        message: v.string(),
        subType: v.string(),
        type: v.string(),
      }),
    }),
  }),
  v.object({
    ...vBaseEvent,
    type: v.literal('email.opened'),
    data: v.object(vBaseEmailEventData),
  }),
  v.object({
    ...vBaseEvent,
    type: v.literal('email.clicked'),
    data: v.object({
      ...vBaseEmailEventData,
      click: v.object({
        ipAddress: v.string(),
        link: v.string(),
        timestamp: v.string(),
        userAgent: v.string(),
      }),
    }),
  }),
  v.object({
    ...vBaseEvent,
    type: v.literal('email.failed'),
    data: v.object({
      ...vBaseEmailEventData,
      failed: v.object({
        reason: v.string(),
      }),
    }),
  }),
]);

export type EmailEvent = v.InferOutput<typeof vEmailEvent>;
export type EmailEventType = EmailEvent['type'];

export const vResendErrorBody = v.object({
  name: v.optional(v.string()),
  message: v.optional(v.string()),
  statusCode: v.optional(v.union([v.number(), v.null()])),
});

export const vSendEmailResponse = v.object({
  id: v.string(),
});
