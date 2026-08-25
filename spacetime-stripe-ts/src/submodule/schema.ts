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
import type Stripe from 'stripe';
import { installStripe } from './install';

// Internal ingest lifecycle for webhook rows. Received = stored. Processed =
// applied to the data model. Ignored = duplicate or unhandled event type.
// Failed = signature/format error.
//
// The other status columns on this schema (subscription, checkout, invoice,
// and payment) stay as t.string() because they reflect Stripe-owned vocabulary
// delivered by webhooks. An open string preserves new provider states.
// Stripe's TS types lift them to literal unions on the SDK side; consumers
// can do `subscription.status === 'active'` directly against the wire value.
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
export type WebhookEventStatusValue =
  (typeof WebhookEventStatus)[keyof typeof WebhookEventStatus];

export const stripeCustomerRow = {
  stripeCustomerId: t.string().primaryKey(),
  appUserId: t.option(t.string()),
  email: t.option(t.string()),
  name: t.option(t.string()),
  metadataJson: t.option(t.string()),
  userId: t.option(t.string()),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
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
  insertedAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

export const stripeCheckoutSessionRow = {
  stripeCheckoutSessionId: t.string().primaryKey(),
  stripeCustomerId: t.option(t.string()),
  status: t.string(),
  mode: t.string(),
  metadataJson: t.option(t.string()),
  insertedAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

export const stripePaymentRow = {
  stripePaymentIntentId: t.string().primaryKey(),
  stripeCustomerId: t.option(t.string()),
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
  insertedAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

export const stripeWebhookEventRow = {
  eventId: t.string().primaryKey(),
  eventType: t.string(),
  livemode: t.bool(),
  signatureHeader: t.option(t.string()),
  payloadJson: t.string(),
  status: webhookEventStatus,
  errorMessage: t.option(t.string()),
  receivedAt: t.timestamp(),
  processedAt: t.option(t.timestamp()),
};

export const stripeCustomerTable = table(
  {
    name: 'stripe_customer',
    public: false,
    indexes: [
      { accessor: 'byEmail', algorithm: 'btree', columns: ['email'] },
      { accessor: 'byUserId', algorithm: 'btree', columns: ['userId'] },
    ],
  },
  stripeCustomerRow
);
export const stripeSubscriptionTable = table(
  {
    name: 'stripe_subscription',
    public: false,
    indexes: [
      {
        accessor: 'byCustomer',
        algorithm: 'btree',
        columns: ['stripeCustomerId'],
      },
      {
        accessor: 'byCustomerInsertedAt',
        algorithm: 'btree',
        columns: ['stripeCustomerId', 'insertedAt'],
      },
      { accessor: 'byOrgId', algorithm: 'btree', columns: ['orgId'] },
      {
        accessor: 'byOrgInsertedAt',
        algorithm: 'btree',
        columns: ['orgId', 'insertedAt'],
      },
      { accessor: 'byUserId', algorithm: 'btree', columns: ['userId'] },
      {
        accessor: 'byUserInsertedAt',
        algorithm: 'btree',
        columns: ['userId', 'insertedAt'],
      },
    ],
  },
  stripeSubscriptionRow
);
export const stripeCheckoutSessionTable = table(
  {
    name: 'stripe_checkout_session',
    public: false,
    indexes: [
      {
        accessor: 'byCustomer',
        algorithm: 'btree',
        columns: ['stripeCustomerId'],
      },
    ],
  },
  stripeCheckoutSessionRow
);
export const stripePaymentTable = table(
  {
    name: 'stripe_payment',
    public: false,
    indexes: [
      {
        accessor: 'byCustomer',
        algorithm: 'btree',
        columns: ['stripeCustomerId'],
      },
      { accessor: 'byOrgId', algorithm: 'btree', columns: ['orgId'] },
      { accessor: 'byUserId', algorithm: 'btree', columns: ['userId'] },
    ],
  },
  stripePaymentRow
);
export const stripeInvoiceTable = table(
  {
    name: 'stripe_invoice',
    public: false,
    indexes: [
      {
        accessor: 'byCustomer',
        algorithm: 'btree',
        columns: ['stripeCustomerId'],
      },
      {
        accessor: 'bySubscription',
        algorithm: 'btree',
        columns: ['stripeSubscriptionId'],
      },
      { accessor: 'byOrgId', algorithm: 'btree', columns: ['orgId'] },
      { accessor: 'byUserId', algorithm: 'btree', columns: ['userId'] },
    ],
  },
  stripeInvoiceRow
);
export const stripeWebhookEventTable = table(
  {
    name: 'stripe_webhook_event',
    public: false,
    indexes: [
      { accessor: 'byStatus', algorithm: 'btree', columns: ['status'] },
    ],
  },
  stripeWebhookEventRow
);

// Singleton row holding deploy-time secrets. Private, never subscribable.
export const stripeConfigRow = {
  singleton: t.bool().primaryKey(),
  secretKey: t.string(),
  stripeVersion: t.option(t.string()),
  webhookSigningSecret: t.option(t.string()),
  updatedAt: t.timestamp(),
};

export const stripeConfigTable = table(
  { name: 'stripe_config', public: false, indexes: [] },
  stripeConfigRow
);

// Allowlist of identities permitted to call privileged procedures.
export const stripeAdminIdentityRow = {
  identity: t.identity().primaryKey(),
  addedAtMicros: t.i64(),
};

export const stripeAdminIdentityTable = table(
  { name: 'stripe_admin_identity', public: false, indexes: [] },
  stripeAdminIdentityRow
);

export const spacetimedb = schema({
  stripeCustomer: stripeCustomerTable,
  stripeSubscription: stripeSubscriptionTable,
  stripeCheckoutSession: stripeCheckoutSessionTable,
  stripePayment: stripePaymentTable,
  stripeInvoice: stripeInvoiceTable,
  stripeWebhookEvent: stripeWebhookEventTable,
  stripeConfig: stripeConfigTable,
  stripeAdminIdentity: stripeAdminIdentityTable,
});

export const init = spacetimedb.init(ctx => {
  installStripe(ctx);
});

export default spacetimedb;

export type ReducerModuleCtx = ReducerCtx<typeof spacetimedb.schemaType>;
export type ProcedureModuleCtx = ProcedureCtx<typeof spacetimedb.schemaType>;
export type TransactionModuleCtx = TransactionCtx<
  typeof spacetimedb.schemaType
>;
export type WriteCtx = ReducerModuleCtx | TransactionModuleCtx;
export type JsonRecord = Record<string, unknown>;
export type ModuleTimestamp = ReducerModuleCtx['timestamp'];

export const stripeHttpResponse = t.object('StripeHttpResponse', {
  status: t.u16(),
  body: t.string(),
});

export const checkoutSessionResult = t.object('CheckoutSessionResult', {
  sessionId: t.string(),
  url: t.option(t.string()),
});

export const createCustomerResult = t.object('CreateCustomerResult', {
  customerId: t.string(),
});

export const getOrCreateCustomerResult = t.object('GetOrCreateCustomerResult', {
  customerId: t.string(),
  isNew: t.bool(),
});

export const portalSessionResult = t.object('PortalSessionResult', {
  url: t.string(),
});

export const subscriptionWithCreationTime = t.object(
  'SubscriptionWithCreationTime',
  {
    insertedAtMicros: t.i64(),
    stripeSubscriptionId: t.string(),
    stripeCustomerId: t.string(),
    status: t.string(),
  }
);

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
  mode: v.optional(v.string()),
  customer: v.optional(vExpandableIdOrNull),
  metadata: vMetadata,
});

const vInvoiceObject = v.object({
  id: v.string(),
  // Stripe.Invoice.customer is nullable; the apply function rejects null.
  customer: vExpandableIdOrNull,
  subscription: v.optional(vExpandableIdOrNull),
  status: v.optional(v.union([v.string(), v.null()])),
  amount_due: v.optional(v.number()),
  amount_paid: v.optional(v.number()),
  created: v.optional(v.number()),
});

const vPaymentIntentObject = v.object({
  id: v.string(),
  customer: v.optional(vExpandableIdOrNull),
  invoice: v.optional(vExpandableIdOrNull),
  amount: v.optional(v.number()),
  currency: v.optional(v.string()),
  status: v.optional(v.string()),
  created: v.optional(v.number()),
  metadata: vMetadata,
});

// Discriminated union over the 12 handled event types; unknown types route to status=failed.
export const vStripeEvent = v.variant('type', [
  v.object({
    type: v.literal('customer.created'),
    data: v.object({ object: vCustomerObject }),
  }),
  v.object({
    type: v.literal('customer.updated'),
    data: v.object({ object: vCustomerObject }),
  }),
  v.object({
    type: v.literal('customer.subscription.created'),
    data: v.object({ object: vSubscriptionObject }),
  }),
  v.object({
    type: v.literal('customer.subscription.updated'),
    data: v.object({ object: vSubscriptionObject }),
  }),
  v.object({
    type: v.literal('customer.subscription.deleted'),
    data: v.object({ object: vSubscriptionObject }),
  }),
  v.object({
    type: v.literal('checkout.session.completed'),
    data: v.object({ object: vCheckoutSessionObject }),
  }),
  v.object({
    type: v.literal('invoice.created'),
    data: v.object({ object: vInvoiceObject }),
  }),
  v.object({
    type: v.literal('invoice.finalized'),
    data: v.object({ object: vInvoiceObject }),
  }),
  v.object({
    type: v.literal('invoice.paid'),
    data: v.object({ object: vInvoiceObject }),
  }),
  v.object({
    type: v.literal('invoice.payment_succeeded'),
    data: v.object({ object: vInvoiceObject }),
  }),
  v.object({
    type: v.literal('invoice.payment_failed'),
    data: v.object({ object: vInvoiceObject }),
  }),
  v.object({
    type: v.literal('payment_intent.succeeded'),
    data: v.object({ object: vPaymentIntentObject }),
  }),
]);

export type ParsedStripeEvent = v.InferOutput<typeof vStripeEvent>;

// Compile-time SDK alignment: SDK event types must be assignable to our valibot variants.
function _alignCustomer(
  e: Stripe.CustomerCreatedEvent | Stripe.CustomerUpdatedEvent
) {
  const _: Extract<
    ParsedStripeEvent,
    { type: 'customer.created' | 'customer.updated' }
  > = e;
  return _;
}
function _alignSubscription(
  e:
    | Stripe.CustomerSubscriptionCreatedEvent
    | Stripe.CustomerSubscriptionUpdatedEvent
    | Stripe.CustomerSubscriptionDeletedEvent
) {
  const _: Extract<
    ParsedStripeEvent,
    {
      type:
        | 'customer.subscription.created'
        | 'customer.subscription.updated'
        | 'customer.subscription.deleted';
    }
  > = e;
  return _;
}
function _alignCheckout(e: Stripe.CheckoutSessionCompletedEvent) {
  const _: Extract<ParsedStripeEvent, { type: 'checkout.session.completed' }> =
    e;
  return _;
}
function _alignInvoice(
  e:
    | Stripe.InvoiceCreatedEvent
    | Stripe.InvoiceFinalizedEvent
    | Stripe.InvoicePaidEvent
    | Stripe.InvoicePaymentSucceededEvent
    | Stripe.InvoicePaymentFailedEvent
) {
  const _: Extract<
    ParsedStripeEvent,
    {
      type:
        | 'invoice.created'
        | 'invoice.finalized'
        | 'invoice.paid'
        | 'invoice.payment_succeeded'
        | 'invoice.payment_failed';
    }
  > = e;
  return _;
}
function _alignPaymentIntent(e: Stripe.PaymentIntentSucceededEvent) {
  const _: Extract<ParsedStripeEvent, { type: 'payment_intent.succeeded' }> = e;
  return _;
}
void _alignCustomer;
void _alignSubscription;
void _alignCheckout;
void _alignInvoice;
void _alignPaymentIntent;

export const vStripeIdResponse = v.object({ id: v.string() });

export const vStripeCheckoutSessionResponse = v.object({
  id: v.string(),
  url: v.optional(v.union([v.string(), v.null()])),
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

function _alignCheckoutSessionResponse(r: Stripe.Checkout.Session) {
  const _: v.InferOutput<typeof vStripeCheckoutSessionResponse> = r;
  return _;
}
function _alignBillingPortalSessionResponse(r: Stripe.BillingPortal.Session) {
  const _: v.InferOutput<typeof vStripeBillingPortalSessionResponse> = r;
  return _;
}
void _alignCheckoutSessionResponse;
void _alignBillingPortalSessionResponse;
