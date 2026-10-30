import {
  schema,
  table,
  t,
  Range,
  SenderError,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
  type SumBuilder,
} from 'spacetimedb/server';
import * as v from 'valibot';
import { install } from './install.js';

// Internal ingest lifecycle for webhook rows. Received = stored. Processed =
// applied to the data model. Ignored = unhandled event type or an event older
// than the stored state. Failed = a handled event type whose payload could not
// be applied. Requests with an invalid signature are rejected and never stored.
//
// The other status columns on this schema (subscription, checkout, invoice,
// and payment) stay as t.string() because they reflect Stripe-owned vocabulary
// delivered by webhooks. An open string preserves new provider states.
// Stripe's TS types lift them to literal unions on the SDK side; consumers
// can do `subscription.status === 'active'` directly against the wire value.
type WebhookEventStatusBuilder = SumBuilder<
  Record<
    'Received' | 'Processed' | 'Ignored' | 'Failed',
    ReturnType<typeof t.unit>
  >
>;
export const webhookEventStatus: WebhookEventStatusBuilder = t.enum(
  'WebhookEventStatus',
  {
    Received: t.unit(),
    Processed: t.unit(),
    Ignored: t.unit(),
    Failed: t.unit(),
  }
);
export const WebhookEventStatus = {
  Received: { tag: 'Received' as const },
  Processed: { tag: 'Processed' as const },
  Ignored: { tag: 'Ignored' as const },
  Failed: { tag: 'Failed' as const },
};
export type WebhookEventStatusValue =
  (typeof WebhookEventStatus)[keyof typeof WebhookEventStatus];

export const stripeCustomerRow = {
  stripeCustomerId: t.string().primaryKey(),
  appUserId: t.option(t.string()),
  email: t.option(t.string()),
  name: t.option(t.string()),
  metadataJson: t.option(t.string()),
  userId: t.option(t.string()),
  eventCreatedUnix: t.i64(),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
  deleted: t.bool().default(false),
};

export const stripeSubscriptionRow = {
  stripeSubscriptionId: t.string().primaryKey(),
  stripeCustomerId: t.string(),
  status: t.string(),
  currentPeriodEndUnix: t.i64(),
  cancelAtPeriodEnd: t.bool(),
  cancelAtUnix: t.option(t.i64()),
  quantity: t.option(t.i64()),
  priceId: t.option(t.string()),
  metadataJson: t.option(t.string()),
  orgId: t.option(t.string()),
  userId: t.option(t.string()),
  eventCreatedUnix: t.i64(),
  insertedAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

export const stripeCheckoutSessionRow = {
  stripeCheckoutSessionId: t.string().primaryKey(),
  stripeCustomerId: t.option(t.string()),
  status: t.string(),
  paymentStatus: t.string(),
  mode: t.string(),
  metadataJson: t.option(t.string()),
  eventCreatedUnix: t.i64(),
  insertedAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

export const stripePaymentRow = {
  stripePaymentIntentId: t.string().primaryKey(),
  stripeCustomerId: t.option(t.string()),
  stripeInvoiceId: t.option(t.string()),
  amount: t.i64(),
  currency: t.string(),
  status: t.string(),
  createdUnix: t.i64(),
  metadataJson: t.option(t.string()),
  orgId: t.option(t.string()),
  userId: t.option(t.string()),
  insertedAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

export const stripeInvoiceRow = {
  stripeInvoiceId: t.string().primaryKey(),
  stripeCustomerId: t.string(),
  stripeSubscriptionId: t.option(t.string()),
  status: t.string(),
  amountDue: t.i64(),
  amountPaid: t.i64(),
  createdUnix: t.i64(),
  orgId: t.option(t.string()),
  userId: t.option(t.string()),
  eventCreatedUnix: t.i64(),
  insertedAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

export const stripeWebhookEventRow = {
  eventId: t.string().primaryKey(),
  eventType: t.string(),
  livemode: t.bool(),
  payloadJson: t.string(),
  status: webhookEventStatus,
  errorMessage: t.option(t.string()),
  receivedAt: t.timestamp(),
  processedAt: t.option(t.timestamp()),
};

const stripeCustomerOptions = {
  name: 'stripe_customer',
  public: false,
  indexes: [
    { accessor: 'byUserId', algorithm: 'btree', columns: ['userId'] } as const,
  ],
};
export const stripeCustomerTable: ReturnType<
  typeof table<typeof stripeCustomerRow, typeof stripeCustomerOptions>
> = table(stripeCustomerOptions, stripeCustomerRow);
const stripeSubscriptionOptions = {
  name: 'stripe_subscription',
  public: false,
  indexes: [
    {
      accessor: 'byCustomer',
      algorithm: 'btree',
      columns: ['stripeCustomerId'],
    } as const,
    { accessor: 'byOrgId', algorithm: 'btree', columns: ['orgId'] } as const,
    {
      accessor: 'byOrgInsertedAt',
      algorithm: 'btree',
      columns: ['orgId', 'insertedAt'],
    } as const,
    { accessor: 'byUserId', algorithm: 'btree', columns: ['userId'] } as const,
    {
      accessor: 'byUserInsertedAt',
      algorithm: 'btree',
      columns: ['userId', 'insertedAt'],
    } as const,
  ],
};
export const stripeSubscriptionTable: ReturnType<
  typeof table<typeof stripeSubscriptionRow, typeof stripeSubscriptionOptions>
> = table(stripeSubscriptionOptions, stripeSubscriptionRow);
const stripeCheckoutSessionOptions = {
  name: 'stripe_checkout_session',
  public: false,
  indexes: [
    {
      accessor: 'byCustomer',
      algorithm: 'btree',
      columns: ['stripeCustomerId'],
    } as const,
  ],
};
export const stripeCheckoutSessionTable: ReturnType<
  typeof table<
    typeof stripeCheckoutSessionRow,
    typeof stripeCheckoutSessionOptions
  >
> = table(stripeCheckoutSessionOptions, stripeCheckoutSessionRow);
const stripePaymentOptions = {
  name: 'stripe_payment',
  public: false,
  indexes: [
    {
      accessor: 'byCustomer',
      algorithm: 'btree',
      columns: ['stripeCustomerId'],
    } as const,
    {
      accessor: 'byInvoice',
      algorithm: 'btree',
      columns: ['stripeInvoiceId'],
    } as const,
    { accessor: 'byOrgId', algorithm: 'btree', columns: ['orgId'] } as const,
    { accessor: 'byUserId', algorithm: 'btree', columns: ['userId'] } as const,
  ],
};
export const stripePaymentTable: ReturnType<
  typeof table<typeof stripePaymentRow, typeof stripePaymentOptions>
> = table(stripePaymentOptions, stripePaymentRow);
const stripeInvoiceOptions = {
  name: 'stripe_invoice',
  public: false,
  indexes: [
    {
      accessor: 'byCustomer',
      algorithm: 'btree',
      columns: ['stripeCustomerId'],
    } as const,
    {
      accessor: 'bySubscription',
      algorithm: 'btree',
      columns: ['stripeSubscriptionId'],
    } as const,
    { accessor: 'byOrgId', algorithm: 'btree', columns: ['orgId'] } as const,
    { accessor: 'byUserId', algorithm: 'btree', columns: ['userId'] } as const,
  ],
};
export const stripeInvoiceTable: ReturnType<
  typeof table<typeof stripeInvoiceRow, typeof stripeInvoiceOptions>
> = table(stripeInvoiceOptions, stripeInvoiceRow);
const stripeWebhookEventOptions = {
  name: 'stripe_webhook_event',
  public: false,
  indexes: [
    {
      accessor: 'byReceivedAt',
      algorithm: 'btree',
      columns: ['receivedAt'],
    } as const,
  ],
};
export const stripeWebhookEventTable: ReturnType<
  typeof table<typeof stripeWebhookEventRow, typeof stripeWebhookEventOptions>
> = table(stripeWebhookEventOptions, stripeWebhookEventRow);

// Schedules the retention sweep of stripe_webhook_event.
const stripeWebhookPruneTickOptions = {
  name: 'stripe_webhook_prune_tick',
  public: false,
};
const stripeWebhookPruneTickColumns = {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
};
export const stripeWebhookPruneTickTable: ReturnType<
  typeof table<
    typeof stripeWebhookPruneTickColumns,
    typeof stripeWebhookPruneTickOptions
  >
> = table(stripeWebhookPruneTickOptions, stripeWebhookPruneTickColumns);

// Singleton row holding deploy-time secrets. Private, never subscribable.
export const stripeConfigRow = {
  singleton: t.bool().primaryKey(),
  secretKey: t.string(),
  stripeVersion: t.option(t.string()),
  webhookSigningSecret: t.option(t.string()),
  updatedAt: t.timestamp(),
};

const stripeConfigOptions = {
  name: 'stripe_config',
  public: false,
  indexes: [],
};
export const stripeConfigTable: ReturnType<
  typeof table<typeof stripeConfigRow, typeof stripeConfigOptions>
> = table(stripeConfigOptions, stripeConfigRow);

// Allowlist of identities permitted to call privileged procedures.
export const stripeAdminIdentityRow = {
  identity: t.identity().primaryKey(),
  addedAt: t.timestamp(),
};

const stripeAdminIdentityOptions = {
  name: 'stripe_admin_identity',
  public: false,
  indexes: [],
};
export const stripeAdminIdentityTable: ReturnType<
  typeof table<typeof stripeAdminIdentityRow, typeof stripeAdminIdentityOptions>
> = table(stripeAdminIdentityOptions, stripeAdminIdentityRow);

const tables: {
  stripeCustomer: typeof stripeCustomerTable;
  stripeSubscription: typeof stripeSubscriptionTable;
  stripeCheckoutSession: typeof stripeCheckoutSessionTable;
  stripePayment: typeof stripePaymentTable;
  stripeInvoice: typeof stripeInvoiceTable;
  stripeWebhookEvent: typeof stripeWebhookEventTable;
  stripeWebhookPruneTick: typeof stripeWebhookPruneTickTable;
  stripeConfig: typeof stripeConfigTable;
  stripeAdminIdentity: typeof stripeAdminIdentityTable;
} = {
  stripeCustomer: stripeCustomerTable,
  stripeSubscription: stripeSubscriptionTable,
  stripeCheckoutSession: stripeCheckoutSessionTable,
  stripePayment: stripePaymentTable,
  stripeInvoice: stripeInvoiceTable,
  stripeWebhookEvent: stripeWebhookEventTable,
  stripeWebhookPruneTick: stripeWebhookPruneTickTable,
  stripeConfig: stripeConfigTable,
  stripeAdminIdentity: stripeAdminIdentityTable,
};
export const spacetimedb: ReturnType<typeof schema<typeof tables>> =
  schema(tables);

export const init: ReturnType<typeof spacetimedb.init> = spacetimedb.init(
  ctx => {
    install(ctx);
  }
);

export default spacetimedb;

export type ReducerModuleCtx = ReducerCtx<typeof spacetimedb.schemaType>;
export type ProcedureModuleCtx = ProcedureCtx<typeof spacetimedb.schemaType>;
export type TransactionModuleCtx = TransactionCtx<
  typeof spacetimedb.schemaType
>;
export type WriteCtx = ReducerModuleCtx | TransactionModuleCtx;
export type JsonRecord = Record<string, unknown>;
export type ModuleTimestamp = ReducerModuleCtx['timestamp'];

export const checkoutSessionResult = t.object('CheckoutSessionResult', {
  sessionId: t.string(),
  url: t.option(t.string()),
});

export const getOrCreateCustomerResult = t.object('GetOrCreateCustomerResult', {
  customerId: t.string(),
  isNew: t.bool(),
});

export const portalSessionResult = t.object('PortalSessionResult', {
  url: t.string(),
});

export { Range, SenderError, t };

const vMetadata = v.optional(
  v.union([v.record(v.string(), v.string()), v.null()])
);

// Stripe "expandable" fields are either a string ID or an object with an id.
const vExpandableId = v.union([v.string(), v.object({ id: v.string() })]);
const vExpandableIdOrNull = v.union([
  v.string(),
  v.object({ id: v.string() }),
  v.null(),
]);

export type ExpandableId = v.InferOutput<typeof vExpandableId>;
export type ExpandableIdOrNull = v.InferOutput<typeof vExpandableIdOrNull>;

export function extractExpandableId(value: ExpandableId): string {
  return typeof value === 'string' ? value : value.id;
}

export function extractExpandableIdOrNull(
  value: ExpandableIdOrNull
): string | null {
  if (value === null) return null;
  return typeof value === 'string' ? value : value.id;
}

const vCustomerObject = v.object({
  id: v.string(),
  email: v.optional(v.union([v.string(), v.null()])),
  name: v.optional(v.union([v.string(), v.null()])),
  metadata: vMetadata,
});

const vSubscriptionItem = v.object({
  current_period_end: v.optional(v.number()),
  quantity: v.optional(v.number()),
  price: v.optional(v.union([v.object({ id: v.string() }), v.null()])),
});

const vSubscriptionObject = v.object({
  id: v.string(),
  customer: vExpandableId,
  status: v.string(),
  current_period_end: v.optional(v.number()),
  cancel_at: v.optional(v.union([v.number(), v.null()])),
  cancel_at_period_end: v.optional(v.boolean()),
  items: v.optional(v.object({ data: v.array(vSubscriptionItem) })),
  metadata: vMetadata,
});

const vCheckoutSessionObject = v.object({
  id: v.string(),
  status: v.optional(v.union([v.string(), v.null()])),
  payment_status: v.string(),
  mode: v.optional(v.string()),
  customer: v.optional(vExpandableIdOrNull),
  metadata: vMetadata,
});

const vInvoiceObject = v.object({
  id: v.string(),
  // Stripe.Invoice.customer is nullable; the apply function rejects null.
  customer: vExpandableIdOrNull,
  // API versions before 2025-03-31.basil.
  subscription: v.optional(vExpandableIdOrNull),
  // API versions from 2025-03-31.basil.
  parent: v.optional(
    v.union([
      v.object({
        subscription_details: v.optional(
          v.union([
            v.object({ subscription: vExpandableId, metadata: vMetadata }),
            v.null(),
          ])
        ),
      }),
      v.null(),
    ])
  ),
  status: v.optional(v.union([v.string(), v.null()])),
  amount_due: v.optional(v.number()),
  amount_paid: v.optional(v.number()),
  created: v.optional(v.number()),
});

const vPaymentIntentObject = v.object({
  id: v.string(),
  customer: v.optional(vExpandableIdOrNull),
  // Removed in 2025-03-31.basil; `invoice_payment.paid` links newer payloads.
  invoice: v.optional(vExpandableIdOrNull),
  amount: v.optional(v.number()),
  currency: v.optional(v.string()),
  status: v.optional(v.string()),
  created: v.optional(v.number()),
  metadata: vMetadata,
});

const vInvoicePaymentObject = v.object({
  invoice: vExpandableId,
  amount_paid: v.union([v.number(), v.null()]),
  currency: v.string(),
  created: v.number(),
  payment: v.object({ payment_intent: v.optional(vExpandableId) }),
});

function vEvent<const T extends string, const O extends v.GenericSchema>(
  type: T,
  object: O
) {
  return v.object({
    type: v.literal(type),
    created: v.number(),
    data: v.object({ object }),
  });
}

// Unknown event types are acknowledged as Ignored.
export const vStripeEvent = v.variant('type', [
  vEvent('customer.created', vCustomerObject),
  vEvent('customer.updated', vCustomerObject),
  vEvent('customer.deleted', vCustomerObject),
  vEvent('customer.subscription.created', vSubscriptionObject),
  vEvent('customer.subscription.updated', vSubscriptionObject),
  vEvent('customer.subscription.deleted', vSubscriptionObject),
  vEvent('checkout.session.completed', vCheckoutSessionObject),
  vEvent('checkout.session.async_payment_succeeded', vCheckoutSessionObject),
  vEvent('checkout.session.async_payment_failed', vCheckoutSessionObject),
  vEvent('invoice.created', vInvoiceObject),
  vEvent('invoice.updated', vInvoiceObject),
  vEvent('invoice.finalized', vInvoiceObject),
  vEvent('invoice.paid', vInvoiceObject),
  vEvent('invoice.payment_succeeded', vInvoiceObject),
  vEvent('invoice.payment_failed', vInvoiceObject),
  vEvent('invoice_payment.paid', vInvoicePaymentObject),
  vEvent('payment_intent.succeeded', vPaymentIntentObject),
  vEvent('payment_intent.payment_failed', vPaymentIntentObject),
]);

export type ParsedStripeEvent = v.InferOutput<typeof vStripeEvent>;

export const vStripeIdResponse = v.object({ id: v.string() });

export const vStripeCheckoutSessionResponse = v.object({
  id: v.string(),
  url: v.optional(v.union([v.string(), v.null()])),
  client_secret: v.optional(v.union([v.string(), v.null()])),
});

export const vStripeBillingPortalSessionResponse = v.object({
  url: v.string(),
});

// Stripe error response: `{ error: { type, code, message, request_log_url } }`.
export const vStripeErrorBody = v.object({
  error: v.object({
    type: v.optional(v.string()),
    code: v.optional(v.string()),
    message: v.optional(v.string()),
    request_log_url: v.optional(v.union([v.string(), v.null()])),
  }),
});
