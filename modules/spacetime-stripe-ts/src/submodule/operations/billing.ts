import { sha256, bytesToHex } from '@spacetimedb/crypto';
import { checkoutForm, type UserCheckoutArgs } from '../checkout.js';
import type { ProcedureExport } from 'spacetimedb/server';
import * as v from 'valibot';
import {
  t,
  spacetimedb,
  checkoutSessionResult,
  getOrCreateCustomerResult,
  portalSessionResult,
  vStripeCheckoutSessionResponse,
  vStripeBillingPortalSessionResponse,
  type ProcedureModuleCtx,
  type JsonRecord,
} from '../schema.js';
import { loadConfigOrThrowFromProcedure } from '../config.js';
import {
  safeJsonParse,
  summarizeIssues,
  throwSenderError,
} from '../validation.js';
import { errors } from '../errors.js';

import {
  requireProcedureAdmin,
  withAdminTx,
  isRecord,
  maybeString,
  maybeBoolean,
  maybeBigIntFromUnknown,
  maybeId,
  stripeErrorSuffix,
  metadataInfo,
  coerceMetadataFromJson,
  deriveCancelAtPeriodEnd,
  formPairsToBody,
  metadataJsonToFormPairs,
  upsertSubscriptionRow,
  unixSeconds,
  callStripe,
  createCustomerInStripeAndSync,
} from '../operations.js';

const validateStripePriceArgs = { priceId: t.string() };
const validateStripePriceResult = t.object('ValidateStripePriceResult', {
  valid: t.bool(),
  status: t.u16(),
  active: t.option(t.bool()),
  currency: t.option(t.string()),
  unitAmount: t.option(t.i64()),
  livemode: t.option(t.bool()),
  type: t.option(t.string()),
  message: t.option(t.string()),
  code: t.option(t.string()),
  errorType: t.option(t.string()),
});
export const validateStripePrice: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof validateStripePriceArgs,
  typeof validateStripePriceResult
> = spacetimedb.procedure(
  validateStripePriceArgs,
  validateStripePriceResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    const cfg = loadConfigOrThrowFromProcedure(ctx);
    const response = callStripe(ctx, {
      method: 'GET',
      path: `/v1/prices/${args.priceId}`,
      secretKey: cfg.secretKey,
      stripeVersion: cfg.stripeVersion,
      idempotencyKey: undefined,
      formBody: undefined,
    });
    const parsed = safeJsonParse(response.body);
    const isOk = response.status >= 200 && response.status < 300;
    if (!isOk) {
      const err =
        isRecord(parsed) && isRecord(parsed.error) ? parsed.error : undefined;
      return {
        valid: false,
        status: response.status,
        active: undefined,
        currency: undefined,
        unitAmount: undefined,
        livemode: undefined,
        type: undefined,
        message:
          maybeString(err?.message) ?? `Stripe returned ${response.status}.`,
        code: maybeString(err?.code),
        errorType: maybeString(err?.type),
      };
    }
    const data = isRecord(parsed) ? parsed : {};
    return {
      valid: true,
      status: response.status,
      active: maybeBoolean(data.active),
      currency: maybeString(data.currency),
      unitAmount: maybeBigIntFromUnknown(data.unit_amount),
      livemode: maybeBoolean(data.livemode),
      type: maybeString(data.type),
      message: undefined,
      code: undefined,
      errorType: undefined,
    };
  }
);

const getRemoteCheckoutSessionArgs = { sessionId: t.string() };
const getRemoteCheckoutSessionResult = t.object('RemoteCheckoutSessionResult', {
  ok: t.bool(),
  status: t.u16(),
  sessionId: t.option(t.string()),
  paymentStatus: t.option(t.string()),
  sessionStatus: t.option(t.string()),
  mode: t.option(t.string()),
  amountTotal: t.option(t.i64()),
  currency: t.option(t.string()),
  customerId: t.option(t.string()),
  paymentIntentId: t.option(t.string()),
  message: t.option(t.string()),
  code: t.option(t.string()),
  errorType: t.option(t.string()),
});
export const getRemoteCheckoutSession: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof getRemoteCheckoutSessionArgs,
  typeof getRemoteCheckoutSessionResult
> = spacetimedb.procedure(
  getRemoteCheckoutSessionArgs,
  getRemoteCheckoutSessionResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    const cfg = loadConfigOrThrowFromProcedure(ctx);
    const response = callStripe(ctx, {
      method: 'GET',
      path: `/v1/checkout/sessions/${args.sessionId}`,
      secretKey: cfg.secretKey,
      stripeVersion: cfg.stripeVersion,
      idempotencyKey: undefined,
      formBody: undefined,
    });
    const parsed = safeJsonParse(response.body);
    const isOk = response.status >= 200 && response.status < 300;
    if (!isOk) {
      const err =
        isRecord(parsed) && isRecord(parsed.error) ? parsed.error : undefined;
      return {
        ok: false,
        status: response.status,
        sessionId: undefined,
        paymentStatus: undefined,
        sessionStatus: undefined,
        mode: undefined,
        amountTotal: undefined,
        currency: undefined,
        customerId: undefined,
        paymentIntentId: undefined,
        message:
          maybeString(err?.message) ?? `Stripe returned ${response.status}.`,
        code: maybeString(err?.code),
        errorType: maybeString(err?.type),
      };
    }
    const data = isRecord(parsed) ? parsed : {};
    return {
      ok: true,
      status: response.status,
      sessionId: maybeString(data.id) ?? args.sessionId,
      paymentStatus: maybeString(data.payment_status),
      sessionStatus: maybeString(data.status),
      mode: maybeString(data.mode),
      amountTotal: maybeBigIntFromUnknown(data.amount_total),
      currency: maybeString(data.currency),
      customerId: maybeId(data.customer),
      paymentIntentId: maybeId(data.payment_intent),
      message: undefined,
      code: undefined,
      errorType: undefined,
    };
  }
);

// Cheap count of stripe_webhook_event rows; exposes the metric without leaking payloads.
const getWebhookEventCountResult = t.i64();
export const getWebhookEventCount: ProcedureExport<
  typeof spacetimedb.schemaType,
  {},
  typeof getWebhookEventCountResult
> = spacetimedb.procedure({}, getWebhookEventCountResult, ctx =>
  withAdminTx(ctx, tx => BigInt(tx.db.stripeWebhookEvent.count()))
);

const unitResult: ReturnType<typeof t.unit> = t.unit();

const updateSubscriptionMetadataArgs = {
  stripeSubscriptionId: t.string(),
  metadataJson: t.string(),
  orgId: t.option(t.string()),
  userId: t.option(t.string()),
};
export const updateSubscriptionMetadata: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof updateSubscriptionMetadataArgs,
  typeof unitResult
> = spacetimedb.procedure(
  updateSubscriptionMetadataArgs,
  unitResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    const parsedDetails = coerceMetadataFromJson(args.metadataJson);
    ctx.withTx(tx => {
      const existing = tx.db.stripeSubscription.stripeSubscriptionId.find(
        args.stripeSubscriptionId
      );
      if (!existing) {
        throwSenderError(errors.subscriptionNotFound);
      }
      upsertSubscriptionRow(tx, ctx.timestamp, {
        stripeSubscriptionId: existing.stripeSubscriptionId,
        stripeCustomerId: existing.stripeCustomerId,
        status: existing.status,
        currentPeriodEndUnix: existing.currentPeriodEndUnix,
        cancelAtPeriodEnd: existing.cancelAtPeriodEnd,
        cancelAtUnix: existing.cancelAtUnix,
        quantity: existing.quantity,
        priceId: existing.priceId,
        metadataJson: parsedDetails.metadataJson ?? args.metadataJson,
        orgId: args.orgId ?? parsedDetails.orgId ?? existing.orgId,
        userId: args.userId ?? parsedDetails.userId ?? existing.userId,
        eventCreatedUnix: existing.eventCreatedUnix,
      });
    });
    return {};
  }
);
type CheckoutSessionArgs = {
  items: Array<{ priceId: string; quantity: bigint }>;
  customerId?: string;
  mode: string;
  successUrl: string;
  cancelUrl: string;
  metadataJson?: string;
  subscriptionMetadataJson?: string;
  paymentIntentMetadataJson?: string;
};

export type CheckoutSessionResult = {
  sessionId: string;
  url: string | undefined;
  clientSecret?: string;
};

export function findUserCustomerId(
  ctx: ProcedureModuleCtx,
  userId: string
): string | undefined {
  return ctx.withTx(tx => {
    for (const customer of tx.db.stripeCustomer.byUserId.filter(userId))
      if (!customer.deleted) return customer.stripeCustomerId;
    for (const sub of tx.db.stripeSubscription.byUserId.filter(userId))
      if (
        !tx.db.stripeCustomer.stripeCustomerId.find(sub.stripeCustomerId)
          ?.deleted
      )
        return sub.stripeCustomerId;
    for (const payment of tx.db.stripePayment.byUserId.filter(userId)) {
      if (
        payment.stripeCustomerId &&
        !tx.db.stripeCustomer.stripeCustomerId.find(payment.stripeCustomerId)
          ?.deleted
      )
        return payment.stripeCustomerId;
    }
    return undefined;
  });
}

/**
 * Returns the Stripe customer for an application user, creating it with
 * `metadata.userId` when none exists. Customers are matched by `userId` only.
 * Performs no authorization: the host resolves `userId` from trusted context.
 */
export function getOrCreateUserCustomer(
  ctx: ProcedureModuleCtx,
  args: {
    userId: string;
    email?: string;
    name?: string;
    metadata?: Record<string, string>;
  }
): { customerId: string; isNew: boolean } {
  const existing = findUserCustomerId(ctx, args.userId);
  if (existing) return { customerId: existing, isNew: false };

  const cfg = loadConfigOrThrowFromProcedure(ctx);
  const generation = ctx.withTx(tx =>
    [...tx.db.stripeCustomer.byUserId.filter(args.userId)]
      .filter(customer => customer.deleted)
      .map(customer => customer.stripeCustomerId)
      .sort()
  );
  const customerId = createCustomerInStripeAndSync(ctx, {
    secretKey: cfg.secretKey,
    stripeVersion: cfg.stripeVersion,
    email: args.email,
    name: args.name,
    metadataJson: JSON.stringify({ ...args.metadata, userId: args.userId }),
    idempotencyKey: `customer:${bytesToHex(sha256(new TextEncoder().encode(JSON.stringify([ctx.databaseIdentity.toHexString(), args.userId, generation]))))}`,
  });
  return { customerId, isNew: true };
}

/** Creates a customer using trusted application metadata. Performs no authorization. */
export function createCustomer(
  ctx: ProcedureModuleCtx,
  args: {
    email?: string;
    name?: string;
    metadata?: Record<string, string>;
    idempotencyKey?: string;
  }
): { customerId: string } {
  const cfg = loadConfigOrThrowFromProcedure(ctx);
  return {
    customerId: createCustomerInStripeAndSync(ctx, {
      ...cfg,
      email: args.email,
      name: args.name,
      metadataJson:
        args.metadata === undefined ? undefined : JSON.stringify(args.metadata),
      idempotencyKey: args.idempotencyKey,
    }),
  };
}

// Stripe enforces one mode per session; all items must share mode.
function createCheckoutSessionInStripe(
  ctx: ProcedureModuleCtx,
  args: CheckoutSessionArgs
): CheckoutSessionResult {
  if (args.items.length === 0) {
    throwSenderError(errors.checkoutSessionRequiresItems);
  }
  const formPairs: Array<[string, string | undefined]> = [
    ['mode', args.mode],
    ['success_url', args.successUrl],
    ['cancel_url', args.cancelUrl],
    ['customer', args.customerId],
  ];
  args.items.forEach((item, i) => {
    formPairs.push([`line_items[${i}][price]`, item.priceId]);
    formPairs.push([`line_items[${i}][quantity]`, String(item.quantity)]);
  });
  formPairs.push(...metadataJsonToFormPairs('metadata', args.metadataJson));
  if (args.mode === 'subscription') {
    formPairs.push(
      ...metadataJsonToFormPairs(
        'subscription_data[metadata]',
        args.subscriptionMetadataJson
      )
    );
  }
  if (args.mode === 'payment') {
    formPairs.push(
      ...metadataJsonToFormPairs(
        'payment_intent_data[metadata]',
        args.paymentIntentMetadataJson
      )
    );
  }

  return requestCheckoutSession(ctx, formPairsToBody(formPairs));
}

function requestCheckoutSession(
  ctx: ProcedureModuleCtx,
  formBody: string,
  idempotencyKey?: string
): CheckoutSessionResult {
  const cfg = loadConfigOrThrowFromProcedure(ctx);
  const response = callStripe(ctx, {
    method: 'POST',
    path: '/v1/checkout/sessions',
    secretKey: cfg.secretKey,
    stripeVersion: cfg.stripeVersion,
    idempotencyKey,
    formBody,
  });
  if (response.status < 200 || response.status >= 300) {
    throwSenderError(
      `${errors.checkoutSessionFailed}:${response.status}${stripeErrorSuffix(response.body)}`
    );
  }

  const sessionResult = v.safeParse(
    vStripeCheckoutSessionResponse,
    safeJsonParse(response.body)
  );
  if (!sessionResult.success) {
    throwSenderError(
      `${errors.checkoutSessionInvalidResponse}:${summarizeIssues(sessionResult.issues)}`
    );
  }
  return {
    sessionId: sessionResult.output.id,
    url: sessionResult.output.url ?? undefined,
    clientSecret: sessionResult.output.client_secret ?? undefined,
  };
}

/**
 * Creates a Checkout session for an application user's own Stripe customer.
 * `userId` is written to the session metadata and to the resulting
 * subscription or PaymentIntent metadata, so webhook rows carry it. Performs
 * no authorization: the host resolves `userId` from trusted context and
 * supplies server-owned price IDs and return URLs.
 */
export function createUserCheckoutSession(
  ctx: ProcedureModuleCtx,
  args: UserCheckoutArgs
): CheckoutSessionResult {
  // Validate before creating a customer or making a provider request.
  checkoutForm(args, '');
  const { customerId } = getOrCreateUserCustomer(ctx, args);
  const result = requestCheckoutSession(
    ctx,
    checkoutForm(args, customerId),
    args.idempotencyKey === undefined
      ? undefined
      : `checkout:${customerId}:${args.idempotencyKey}`
  );
  if (args.uiMode === 'embedded_page' ? !result.clientSecret : !result.url) {
    throwSenderError(errors.checkoutSessionInvalidResponse);
  }
  return result;
}

/**
 * Sends a request to a relative `/v1/` path on api.stripe.com with the stored
 * secret key. Performs no authorization.
 */
export function stripeRequest(
  ctx: ProcedureModuleCtx,
  args: {
    method: 'GET' | 'POST' | 'DELETE';
    path: string;
    formBody?: string;
    idempotencyKey?: string;
  }
): { status: number; body: string } {
  const cfg = loadConfigOrThrowFromProcedure(ctx);
  return callStripe(ctx, {
    method: args.method,
    path: args.path,
    secretKey: cfg.secretKey,
    stripeVersion: cfg.stripeVersion,
    formBody: args.formBody,
    idempotencyKey: args.idempotencyKey,
  });
}

const getOrCreateCustomerArgs = {
  userId: t.string(),
  email: t.option(t.string()),
  name: t.option(t.string()),
};
export const getOrCreateCustomer: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof getOrCreateCustomerArgs,
  typeof getOrCreateCustomerResult
> = spacetimedb.procedure(
  getOrCreateCustomerArgs,
  getOrCreateCustomerResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    return getOrCreateUserCustomer(ctx, args);
  }
);

const createCheckoutSessionArgs = {
  items: t.array(
    t.object('CheckoutLineItem', {
      priceId: t.string(),
      quantity: t.i64(),
    })
  ),
  customerId: t.option(t.string()),
  mode: t.string(),
  successUrl: t.string(),
  cancelUrl: t.string(),
  metadataJson: t.option(t.string()),
  subscriptionMetadataJson: t.option(t.string()),
  paymentIntentMetadataJson: t.option(t.string()),
};
export const createCheckoutSession: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof createCheckoutSessionArgs,
  typeof checkoutSessionResult
> = spacetimedb.procedure(
  createCheckoutSessionArgs,
  checkoutSessionResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    return createCheckoutSessionInStripe(ctx, args);
  }
);

const createCustomerPortalSessionArgs = {
  customerId: t.string(),
  returnUrl: t.string(),
};
export const createCustomerPortalSession: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof createCustomerPortalSessionArgs,
  typeof portalSessionResult
> = spacetimedb.procedure(
  createCustomerPortalSessionArgs,
  portalSessionResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    return createPortalSession(ctx, args);
  }
);

function patchSubscriptionFromStripe(
  ctx: ProcedureModuleCtx,
  args: {
    secretKey: string;
    stripeVersion: string | undefined;
    stripeSubscriptionId: string;
    formBody: string;
    idempotencyKey?: string;
  }
) {
  const response = callStripe(ctx, {
    method: 'POST',
    path: `/v1/subscriptions/${encodeURIComponent(args.stripeSubscriptionId)}`,
    secretKey: args.secretKey,
    stripeVersion: args.stripeVersion,
    idempotencyKey: args.idempotencyKey,
    formBody: args.formBody,
  });
  if (response.status < 200 || response.status >= 300) {
    throwSenderError(
      `${errors.subscriptionUpdateFailed}:${response.status}${stripeErrorSuffix(response.body)}`
    );
  }
  const parsed = safeJsonParse(response.body);
  if (!isRecord(parsed))
    throwSenderError(errors.subscriptionUpdateInvalidResponse);
  return parsed;
}

function syncSubscriptionObjectFromStripe(
  ctx: ProcedureModuleCtx,
  stripeSubscription: JsonRecord
) {
  const subscriptionId = maybeString(stripeSubscription.id);
  const customerId = maybeId(stripeSubscription.customer);
  const status = maybeString(stripeSubscription.status);
  if (!subscriptionId || !customerId || !status) {
    throwSenderError(errors.subscriptionPayloadMissingFields);
  }

  const items = isRecord(stripeSubscription.items)
    ? stripeSubscription.items
    : undefined;
  const firstItem = Array.isArray(items?.data) ? items.data[0] : undefined;
  const first = isRecord(firstItem) ? firstItem : undefined;
  const price = isRecord(first?.price) ? first.price : undefined;

  const currentPeriodEnd =
    maybeBigIntFromUnknown(first?.current_period_end) ??
    maybeBigIntFromUnknown(stripeSubscription.current_period_end) ??
    0n;
  const cancelAtUnix = maybeBigIntFromUnknown(stripeSubscription.cancel_at);
  const cancelAtPeriodEnd =
    maybeBoolean(stripeSubscription.cancel_at_period_end) ??
    deriveCancelAtPeriodEnd(cancelAtUnix, currentPeriodEnd);
  const quantity = maybeBigIntFromUnknown(first?.quantity);
  const meta = metadataInfo(stripeSubscription.metadata);

  ctx.withTx(tx => {
    upsertSubscriptionRow(tx, ctx.timestamp, {
      stripeSubscriptionId: subscriptionId,
      stripeCustomerId: customerId,
      status,
      currentPeriodEndUnix: currentPeriodEnd,
      cancelAtPeriodEnd,
      cancelAtUnix,
      quantity,
      priceId: maybeString(price?.id),
      metadataJson: meta.metadataJson,
      orgId: meta.orgId,
      userId: meta.userId,
      eventCreatedUnix: unixSeconds(ctx.timestamp),
    });
  });
}

const cancelSubscriptionArgs = {
  stripeSubscriptionId: t.string(),
  cancelAtPeriodEnd: t.option(t.bool()),
};
export const cancelSubscription: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof cancelSubscriptionArgs,
  typeof unitResult
> = spacetimedb.procedure(cancelSubscriptionArgs, unitResult, (ctx, args) => {
  requireProcedureAdmin(ctx);
  return cancelSubscriptionInStripe(ctx, args);
});

const reactivateSubscriptionArgs = {
  stripeSubscriptionId: t.string(),
};
export const reactivateSubscription: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof reactivateSubscriptionArgs,
  typeof unitResult
> = spacetimedb.procedure(
  reactivateSubscriptionArgs,
  unitResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    return reactivateSubscriptionInStripe(ctx, args);
  }
);

const updateSubscriptionQuantityArgs = {
  stripeSubscriptionId: t.string(),
  quantity: t.i64(),
};
export const updateSubscriptionQuantity: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof updateSubscriptionQuantityArgs,
  typeof unitResult
> = spacetimedb.procedure(
  updateSubscriptionQuantityArgs,
  unitResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    return updateSubscriptionQuantityInStripe(ctx, args);
  }
);

function createPortalSession(
  ctx: ProcedureModuleCtx,
  args: { customerId: string; returnUrl: string; idempotencyKey?: string }
) {
  const cfg = loadConfigOrThrowFromProcedure(ctx);
  const response = callStripe(ctx, {
    method: 'POST',
    path: '/v1/billing_portal/sessions',
    secretKey: cfg.secretKey,
    stripeVersion: cfg.stripeVersion,
    idempotencyKey: args.idempotencyKey,
    formBody: formPairsToBody([
      ['customer', args.customerId],
      ['return_url', args.returnUrl],
    ]),
  });
  if (response.status < 200 || response.status >= 300) {
    throwSenderError(
      `${errors.portalSessionFailed}:${response.status}${stripeErrorSuffix(response.body)}`
    );
  }

  const portalResult = v.safeParse(
    vStripeBillingPortalSessionResponse,
    safeJsonParse(response.body)
  );
  if (!portalResult.success) {
    throwSenderError(
      `${errors.portalSessionInvalidResponse}:${summarizeIssues(portalResult.issues)}`
    );
  }
  return { url: portalResult.output.url };
}

function cancelSubscriptionInStripe(
  ctx: ProcedureModuleCtx,
  args: {
    stripeSubscriptionId: string;
    cancelAtPeriodEnd?: boolean;
    idempotencyKey?: string;
  }
) {
  const cfg = loadConfigOrThrowFromProcedure(ctx);
  const atPeriodEnd = args.cancelAtPeriodEnd ?? true;
  const stripeSubscription = atPeriodEnd
    ? patchSubscriptionFromStripe(ctx, {
        secretKey: cfg.secretKey,
        stripeVersion: cfg.stripeVersion,
        stripeSubscriptionId: args.stripeSubscriptionId,
        idempotencyKey: args.idempotencyKey,
        formBody: formPairsToBody([['cancel_at_period_end', 'true']]),
      })
    : (() => {
        const response = callStripe(ctx, {
          method: 'DELETE',
          path: `/v1/subscriptions/${encodeURIComponent(args.stripeSubscriptionId)}`,
          secretKey: cfg.secretKey,
          stripeVersion: cfg.stripeVersion,
          idempotencyKey: undefined,
          formBody: undefined,
        });
        if (response.status < 200 || response.status >= 300) {
          throwSenderError(
            `${errors.subscriptionCancelFailed}:${response.status}${stripeErrorSuffix(response.body)}`
          );
        }
        const parsed = safeJsonParse(response.body);
        if (!isRecord(parsed)) {
          throwSenderError(errors.subscriptionCancelInvalidResponse);
        }
        return parsed;
      })();

  syncSubscriptionObjectFromStripe(ctx, stripeSubscription);
  return {};
}

function reactivateSubscriptionInStripe(
  ctx: ProcedureModuleCtx,
  args: { stripeSubscriptionId: string; idempotencyKey?: string }
) {
  const cfg = loadConfigOrThrowFromProcedure(ctx);
  const stripeSubscription = patchSubscriptionFromStripe(ctx, {
    secretKey: cfg.secretKey,
    stripeVersion: cfg.stripeVersion,
    stripeSubscriptionId: args.stripeSubscriptionId,
    idempotencyKey: args.idempotencyKey,
    formBody: formPairsToBody([['cancel_at_period_end', 'false']]),
  });
  syncSubscriptionObjectFromStripe(ctx, stripeSubscription);
  return {};
}

function updateSubscriptionQuantityInStripe(
  ctx: ProcedureModuleCtx,
  args: {
    stripeSubscriptionId: string;
    quantity: bigint;
    subscriptionItemId?: string;
    idempotencyKey?: string;
  }
) {
  if (args.quantity <= 0n || args.quantity > BigInt(Number.MAX_SAFE_INTEGER))
    throwSenderError(errors.invalidQuantity);
  const cfg = loadConfigOrThrowFromProcedure(ctx);
  const getResponse = callStripe(ctx, {
    method: 'GET',
    path: `/v1/subscriptions/${encodeURIComponent(args.stripeSubscriptionId)}`,
    secretKey: cfg.secretKey,
    stripeVersion: cfg.stripeVersion,
    idempotencyKey: undefined,
    formBody: undefined,
  });
  if (getResponse.status < 200 || getResponse.status >= 300) {
    throwSenderError(
      `${errors.subscriptionLookupFailed}:${getResponse.status}${stripeErrorSuffix(getResponse.body)}`
    );
  }

  const existing = safeJsonParse(getResponse.body);
  if (!isRecord(existing))
    throwSenderError(errors.subscriptionLookupInvalidResponse);
  const items = isRecord(existing.items) ? existing.items : undefined;
  const data = Array.isArray(items?.data) ? items.data.filter(isRecord) : [];
  if (!args.subscriptionItemId && data.length > 1)
    throwSenderError(errors.subscriptionItemRequired);
  const selected = args.subscriptionItemId
    ? data.find(item => item.id === args.subscriptionItemId)
    : data[0];
  const subscriptionItemId = maybeString(selected?.id);
  if (!subscriptionItemId) throwSenderError(errors.subscriptionItemNotFound);
  if (subscriptionItemId !== data[0]?.id)
    throwSenderError(errors.subscriptionItemNotStored);

  const updateResponse = callStripe(ctx, {
    method: 'POST',
    path: `/v1/subscription_items/${encodeURIComponent(subscriptionItemId)}`,
    secretKey: cfg.secretKey,
    stripeVersion: cfg.stripeVersion,
    idempotencyKey: args.idempotencyKey,
    formBody: formPairsToBody([['quantity', String(args.quantity)]]),
  });
  if (updateResponse.status < 200 || updateResponse.status >= 300) {
    throwSenderError(
      `${errors.subscriptionItemUpdateFailed}:${updateResponse.status}${stripeErrorSuffix(updateResponse.body)}`
    );
  }

  const refreshed = stripeRequest(ctx, {
    method: 'GET',
    path: `/v1/subscriptions/${encodeURIComponent(args.stripeSubscriptionId)}`,
  });
  const parsed = safeJsonParse(refreshed.body);
  if (refreshed.status < 200 || refreshed.status >= 300 || !isRecord(parsed)) {
    throwSenderError(errors.subscriptionLookupInvalidResponse);
  }
  syncSubscriptionObjectFromStripe(ctx, parsed);
  return {};
}

/** The host must supply a user ID from trusted application context. */
export function createUserCustomerPortalSession(
  ctx: ProcedureModuleCtx,
  args: { userId: string; returnUrl: string; idempotencyKey?: string }
): { url: string } {
  const customerId = findUserCustomerId(ctx, args.userId);
  if (!customerId) throwSenderError(errors.customerNotFound);
  return createPortalSession(ctx, { ...args, customerId });
}

function requireUserSubscription(
  ctx: ProcedureModuleCtx,
  userId: string,
  subscriptionId: string
): void {
  ctx.withTx(tx => {
    const subscription =
      tx.db.stripeSubscription.stripeSubscriptionId.find(subscriptionId);
    const customer =
      subscription &&
      tx.db.stripeCustomer.stripeCustomerId.find(subscription.stripeCustomerId);
    if (!subscription || (subscription.userId ?? customer?.userId) !== userId)
      throwSenderError(errors.notAuthorized);
  });
}

export function cancelUserSubscription(
  ctx: ProcedureModuleCtx,
  args: {
    userId: string;
    stripeSubscriptionId: string;
    cancelAtPeriodEnd?: boolean;
    idempotencyKey?: string;
  }
): void {
  requireUserSubscription(ctx, args.userId, args.stripeSubscriptionId);
  cancelSubscriptionInStripe(ctx, args);
}

export function reactivateUserSubscription(
  ctx: ProcedureModuleCtx,
  args: {
    userId: string;
    stripeSubscriptionId: string;
    idempotencyKey?: string;
  }
): void {
  requireUserSubscription(ctx, args.userId, args.stripeSubscriptionId);
  reactivateSubscriptionInStripe(ctx, args);
}

export function updateUserSubscriptionQuantity(
  ctx: ProcedureModuleCtx,
  args: {
    userId: string;
    stripeSubscriptionId: string;
    quantity: bigint;
    subscriptionItemId?: string;
    idempotencyKey?: string;
  }
): void {
  requireUserSubscription(ctx, args.userId, args.stripeSubscriptionId);
  updateSubscriptionQuantityInStripe(ctx, args);
}
