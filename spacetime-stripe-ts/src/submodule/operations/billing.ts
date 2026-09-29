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
} from '../schema';
import { loadConfigOrThrowFromProcedure } from '../config';
import {
  safeJsonParse,
  summarizeIssues,
  throwSenderError,
} from '../validation';
import { errors } from '../errors';

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
} from '../operations';

export const validateStripePrice = spacetimedb.procedure(
  { priceId: t.string() },
  t.object('ValidateStripePriceResult', {
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
  }),
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

export const getRemoteCheckoutSession = spacetimedb.procedure(
  { sessionId: t.string() },
  t.object('RemoteCheckoutSessionResult', {
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
  }),
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
export const getWebhookEventCount = spacetimedb.procedure({}, t.i64(), ctx =>
  withAdminTx(ctx, tx => BigInt(tx.db.stripeWebhookEvent.count()))
);

export const updateSubscriptionMetadata = spacetimedb.procedure(
  {
    stripeSubscriptionId: t.string(),
    metadataJson: t.string(),
    orgId: t.option(t.string()),
    userId: t.option(t.string()),
  },
  t.unit(),
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
};

function findUserCustomerId(
  ctx: ProcedureModuleCtx,
  userId: string
): string | undefined {
  return ctx.withTx(tx => {
    for (const customer of tx.db.stripeCustomer.byUserId.filter(userId))
      return customer.stripeCustomerId;
    for (const sub of tx.db.stripeSubscription.byUserId.filter(userId))
      return sub.stripeCustomerId;
    for (const payment of tx.db.stripePayment.byUserId.filter(userId)) {
      if (payment.stripeCustomerId) return payment.stripeCustomerId;
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
  args: { userId: string; email?: string; name?: string }
): { customerId: string; isNew: boolean } {
  const existing = findUserCustomerId(ctx, args.userId);
  if (existing) return { customerId: existing, isNew: false };

  const cfg = loadConfigOrThrowFromProcedure(ctx);
  const customerId = createCustomerInStripeAndSync(ctx, {
    secretKey: cfg.secretKey,
    stripeVersion: cfg.stripeVersion,
    email: args.email,
    name: args.name,
    metadataJson: JSON.stringify({ userId: args.userId }),
    idempotencyKey: args.userId,
  });
  return { customerId, isNew: true };
}

// Stripe enforces one mode per session; all items must share mode.
function createCheckoutSessionInStripe(
  ctx: ProcedureModuleCtx,
  args: CheckoutSessionArgs
): CheckoutSessionResult {
  const cfg = loadConfigOrThrowFromProcedure(ctx);
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

  const response = callStripe(ctx, {
    method: 'POST',
    path: '/v1/checkout/sessions',
    secretKey: cfg.secretKey,
    stripeVersion: cfg.stripeVersion,
    idempotencyKey: undefined,
    formBody: formPairsToBody(formPairs),
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
  args: {
    userId: string;
    email?: string;
    name?: string;
    items: Array<{ priceId: string; quantity: bigint }>;
    mode: 'payment' | 'subscription';
    successUrl: string;
    cancelUrl: string;
    metadata?: Record<string, string>;
  }
): CheckoutSessionResult {
  const { customerId } = getOrCreateUserCustomer(ctx, args);
  const metadataJson = JSON.stringify({
    ...args.metadata,
    userId: args.userId,
  });
  return createCheckoutSessionInStripe(ctx, {
    items: args.items,
    customerId,
    mode: args.mode,
    successUrl: args.successUrl,
    cancelUrl: args.cancelUrl,
    metadataJson,
    subscriptionMetadataJson: metadataJson,
    paymentIntentMetadataJson: metadataJson,
  });
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

export const getOrCreateCustomer = spacetimedb.procedure(
  {
    userId: t.string(),
    email: t.option(t.string()),
    name: t.option(t.string()),
  },
  getOrCreateCustomerResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    return getOrCreateUserCustomer(ctx, args);
  }
);

export const createCheckoutSession = spacetimedb.procedure(
  {
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
  },
  checkoutSessionResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    return createCheckoutSessionInStripe(ctx, args);
  }
);

export const createCustomerPortalSession = spacetimedb.procedure(
  {
    customerId: t.string(),
    returnUrl: t.string(),
  },
  portalSessionResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    const cfg = loadConfigOrThrowFromProcedure(ctx);
    const response = callStripe(ctx, {
      method: 'POST',
      path: '/v1/billing_portal/sessions',
      secretKey: cfg.secretKey,
      stripeVersion: cfg.stripeVersion,
      idempotencyKey: undefined,
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
);

function patchSubscriptionFromStripe(
  ctx: ProcedureModuleCtx,
  args: {
    secretKey: string;
    stripeVersion: string | undefined;
    stripeSubscriptionId: string;
    formBody: string;
  }
) {
  const response = callStripe(ctx, {
    method: 'POST',
    path: `/v1/subscriptions/${args.stripeSubscriptionId}`,
    secretKey: args.secretKey,
    stripeVersion: args.stripeVersion,
    idempotencyKey: undefined,
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

export const cancelSubscription = spacetimedb.procedure(
  {
    stripeSubscriptionId: t.string(),
    cancelAtPeriodEnd: t.option(t.bool()),
  },
  t.unit(),
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    const cfg = loadConfigOrThrowFromProcedure(ctx);
    const atPeriodEnd = args.cancelAtPeriodEnd ?? true;
    const stripeSubscription = atPeriodEnd
      ? patchSubscriptionFromStripe(ctx, {
          secretKey: cfg.secretKey,
          stripeVersion: cfg.stripeVersion,
          stripeSubscriptionId: args.stripeSubscriptionId,
          formBody: formPairsToBody([['cancel_at_period_end', 'true']]),
        })
      : (() => {
          const response = callStripe(ctx, {
            method: 'DELETE',
            path: `/v1/subscriptions/${args.stripeSubscriptionId}`,
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
);

export const reactivateSubscription = spacetimedb.procedure(
  {
    stripeSubscriptionId: t.string(),
  },
  t.unit(),
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    const cfg = loadConfigOrThrowFromProcedure(ctx);
    const stripeSubscription = patchSubscriptionFromStripe(ctx, {
      secretKey: cfg.secretKey,
      stripeVersion: cfg.stripeVersion,
      stripeSubscriptionId: args.stripeSubscriptionId,
      formBody: formPairsToBody([['cancel_at_period_end', 'false']]),
    });
    syncSubscriptionObjectFromStripe(ctx, stripeSubscription);
    return {};
  }
);

export const updateSubscriptionQuantity = spacetimedb.procedure(
  {
    stripeSubscriptionId: t.string(),
    quantity: t.i64(),
  },
  t.unit(),
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    const cfg = loadConfigOrThrowFromProcedure(ctx);
    const getResponse = callStripe(ctx, {
      method: 'GET',
      path: `/v1/subscriptions/${args.stripeSubscriptionId}`,
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
    const firstItem = Array.isArray(items?.data) ? items.data[0] : undefined;
    const firstRecord = isRecord(firstItem) ? firstItem : undefined;
    const subscriptionItemId = maybeString(firstRecord?.id);
    if (!subscriptionItemId)
      throwSenderError(errors.subscriptionMissingLineItems);

    const updateResponse = callStripe(ctx, {
      method: 'POST',
      path: `/v1/subscription_items/${subscriptionItemId}`,
      secretKey: cfg.secretKey,
      stripeVersion: cfg.stripeVersion,
      idempotencyKey: undefined,
      formBody: formPairsToBody([['quantity', String(args.quantity)]]),
    });
    if (updateResponse.status < 200 || updateResponse.status >= 300) {
      throwSenderError(
        `${errors.subscriptionItemUpdateFailed}:${updateResponse.status}${stripeErrorSuffix(updateResponse.body)}`
      );
    }

    ctx.withTx(tx => {
      const localSub = tx.db.stripeSubscription.stripeSubscriptionId.find(
        args.stripeSubscriptionId
      );
      if (!localSub) return;
      upsertSubscriptionRow(tx, ctx.timestamp, {
        stripeSubscriptionId: localSub.stripeSubscriptionId,
        stripeCustomerId: localSub.stripeCustomerId,
        status: localSub.status,
        currentPeriodEndUnix: localSub.currentPeriodEndUnix,
        cancelAtPeriodEnd: localSub.cancelAtPeriodEnd,
        cancelAtUnix: localSub.cancelAtUnix,
        quantity: args.quantity,
        priceId: localSub.priceId,
        metadataJson: localSub.metadataJson,
        orgId: localSub.orgId,
        userId: localSub.userId,
        eventCreatedUnix: localSub.eventCreatedUnix,
      });
    });
    return {};
  }
);
