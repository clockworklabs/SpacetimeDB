import {
  t,
  spacetimedb,
  stripeHttpResponse,
  checkoutSessionResult,
  createCustomerResult,
  getOrCreateCustomerResult,
  portalSessionResult,
  vStripeCheckoutSessionResponse,
  vStripeBillingPortalSessionResponse,
  type ProcedureModuleCtx,
  type JsonRecord,
} from '../schema';
import { loadConfigOrThrowFromProcedure } from '../config';
import { adminVerdict, denyIfNotAdmin } from '../auth';
import { attemptToParse, safeJsonParse, summarizeIssues } from '../utils';

import {
  requireProcedureAdmin,
  withAdminTx,
  isRecord,
  maybeString,
  maybeBoolean,
  maybeBigIntFromUnknown,
  maybeId,
  maybeJson,
  stripeErrorSuffix,
  metadataInfo,
  coerceMetadataFromJson,
  deriveCancelAtPeriodEnd,
  formPairsToBody,
  metadataJsonToFormPairs,
  throwSenderError,
  upsertCustomer,
  upsertSubscription,
  callStripe,
  createCustomerInStripeAndSync,
} from '../operations';

export const validate_stripe_price = spacetimedb.procedure(
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

export const get_remote_checkout_session = spacetimedb.procedure(
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
export const get_webhook_event_count = spacetimedb.procedure({}, t.i64(), ctx =>
  withAdminTx(ctx, tx => BigInt(tx.db.stripeWebhookEvent.count()))
);

export const stripe_api_request = spacetimedb.procedure(
  {
    method: t.string(),
    path: t.string(),
    formBody: t.option(t.string()),
    idempotencyKey: t.option(t.string()),
  },
  stripeHttpResponse,
  (ctx, args) => {
    const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
    denyIfNotAdmin(verdict);
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
);

export const create_customer = spacetimedb.procedure(
  {
    email: t.option(t.string()),
    name: t.option(t.string()),
    metadataJson: t.option(t.string()),
    idempotencyKey: t.option(t.string()),
  },
  createCustomerResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    const cfg = loadConfigOrThrowFromProcedure(ctx);
    const customerId = createCustomerInStripeAndSync(ctx, {
      secretKey: cfg.secretKey,
      stripeVersion: cfg.stripeVersion,
      email: args.email,
      name: args.name,
      metadataJson: args.metadataJson,
      idempotencyKey: args.idempotencyKey,
    });
    return { customerId };
  }
);

export const create_or_update_customer = spacetimedb.procedure(
  {
    stripeCustomerId: t.string(),
    email: t.option(t.string()),
    name: t.option(t.string()),
    metadataJson: t.option(t.string()),
  },
  t.string(),
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    const details = coerceMetadataFromJson(args.metadataJson);
    ctx.withTx(tx => {
      upsertCustomer(tx, ctx.timestamp, {
        stripeCustomerId: args.stripeCustomerId,
        appUserId: undefined,
        email: args.email,
        name: args.name,
        metadataJson: details.metadataJson,
        userId: details.userId,
      });
    });
    return args.stripeCustomerId;
  }
);

export const update_subscription_metadata = spacetimedb.procedure(
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
        throwSenderError(
          `stripe.subscription_not_found:${args.stripeSubscriptionId}`
        );
      }
      upsertSubscription(tx, ctx.timestamp, {
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
      });
    });
    return {};
  }
);
export const get_or_create_customer = spacetimedb.procedure(
  {
    userId: t.string(),
    email: t.option(t.string()),
    name: t.option(t.string()),
  },
  getOrCreateCustomerResult,
  (ctx, args) => {
    requireProcedureAdmin(ctx);
    const existingByUser = ctx.withTx(tx => {
      for (const customer of tx.db.stripeCustomer.byUserId.filter(args.userId))
        return customer;
      return undefined;
    });
    if (existingByUser) {
      return { customerId: existingByUser.stripeCustomerId, isNew: false };
    }

    if (args.email) {
      const existingByEmail = ctx.withTx(tx => {
        for (const customer of tx.db.stripeCustomer.byEmail.filter(args.email))
          return customer;
        return undefined;
      });
      if (existingByEmail) {
        return { customerId: existingByEmail.stripeCustomerId, isNew: false };
      }
    }

    const existingSub = ctx.withTx(tx => {
      for (const sub of tx.db.stripeSubscription.byUserId.filter(args.userId))
        return sub;
      return undefined;
    });
    if (existingSub) {
      return { customerId: existingSub.stripeCustomerId, isNew: false };
    }

    const existingPayment = ctx.withTx(tx => {
      for (const payment of tx.db.stripePayment.byUserId.filter(args.userId)) {
        if (payment.userId === args.userId && payment.stripeCustomerId)
          return payment;
      }
      return undefined;
    });
    if (existingPayment?.stripeCustomerId) {
      return { customerId: existingPayment.stripeCustomerId, isNew: false };
    }

    const cfg = loadConfigOrThrowFromProcedure(ctx);
    const metadataJson = JSON.stringify({ userId: args.userId });
    const customerId = createCustomerInStripeAndSync(ctx, {
      secretKey: cfg.secretKey,
      stripeVersion: cfg.stripeVersion,
      email: args.email,
      name: args.name,
      metadataJson,
      idempotencyKey: args.userId,
    });
    return { customerId, isNew: true };
  }
);

// Stripe enforces one mode per session; all items must share mode.
export const create_checkout_session = spacetimedb.procedure(
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
    const cfg = loadConfigOrThrowFromProcedure(ctx);
    if (args.items.length === 0) {
      throwSenderError('stripe.checkout_session_requires_items');
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
    for (const [k, v] of metadataJsonToFormPairs(
      'metadata',
      args.metadataJson
    )) {
      formPairs.push([k, v]);
    }
    if (args.mode === 'subscription') {
      for (const [k, v] of metadataJsonToFormPairs(
        'subscription_data[metadata]',
        args.subscriptionMetadataJson
      )) {
        formPairs.push([k, v]);
      }
    }
    if (args.mode === 'payment') {
      for (const [k, v] of metadataJsonToFormPairs(
        'payment_intent_data[metadata]',
        args.paymentIntentMetadataJson
      )) {
        formPairs.push([k, v]);
      }
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
        `stripe.checkout_session_failed:${response.status}${stripeErrorSuffix(response.body)}`
      );
    }

    const sessionResult = attemptToParse(
      vStripeCheckoutSessionResponse,
      safeJsonParse(response.body)
    );
    if (sessionResult.kind === 'error') {
      throwSenderError(
        `stripe.checkout_session_invalid_response:${summarizeIssues(sessionResult.issues)}`
      );
    }
    return {
      sessionId: sessionResult.data.id,
      url: sessionResult.data.url ?? undefined,
    };
  }
);

export const create_customer_portal_session = spacetimedb.procedure(
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
        `stripe.portal_session_failed:${response.status}${stripeErrorSuffix(response.body)}`
      );
    }

    const portalResult = attemptToParse(
      vStripeBillingPortalSessionResponse,
      safeJsonParse(response.body)
    );
    if (portalResult.kind === 'error') {
      throwSenderError(
        `stripe.portal_session_invalid_response:${summarizeIssues(portalResult.issues)}`
      );
    }
    return { url: portalResult.data.url };
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
      `stripe.subscription_update_failed:${response.status}${stripeErrorSuffix(response.body)}`
    );
  }
  const parsed = maybeJson(response.body);
  if (!isRecord(parsed))
    throwSenderError('stripe.subscription_update_invalid_response');
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
    throwSenderError('stripe.subscription_payload_missing_fields');
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
    upsertSubscription(tx, ctx.timestamp, {
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
    });
  });
}

export const cancel_subscription = spacetimedb.procedure(
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
              `stripe.subscription_cancel_failed:${response.status}${stripeErrorSuffix(response.body)}`
            );
          }
          const parsed = maybeJson(response.body);
          if (!isRecord(parsed)) {
            throwSenderError('stripe.subscription_cancel_invalid_response');
          }
          return parsed;
        })();

    syncSubscriptionObjectFromStripe(ctx, stripeSubscription);
    return {};
  }
);

export const reactivate_subscription = spacetimedb.procedure(
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

export const update_subscription_quantity = spacetimedb.procedure(
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
        `stripe.subscription_lookup_failed:${getResponse.status}${stripeErrorSuffix(getResponse.body)}`
      );
    }

    const existing = maybeJson(getResponse.body);
    if (!isRecord(existing))
      throwSenderError('stripe.subscription_lookup_invalid_response');
    const items = isRecord(existing.items) ? existing.items : undefined;
    const firstItem = Array.isArray(items?.data) ? items.data[0] : undefined;
    const firstRecord = isRecord(firstItem) ? firstItem : undefined;
    const subscriptionItemId = maybeString(firstRecord?.id);
    if (!subscriptionItemId)
      throwSenderError('stripe.subscription_missing_line_items');

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
        `stripe.subscription_item_update_failed:${updateResponse.status}${stripeErrorSuffix(updateResponse.body)}`
      );
    }

    ctx.withTx(tx => {
      const localSub = tx.db.stripeSubscription.stripeSubscriptionId.find(
        args.stripeSubscriptionId
      );
      if (!localSub) return;
      upsertSubscription(tx, ctx.timestamp, {
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
      });
    });
    return {};
  }
);
