import {
  Range,
  t,
  spacetimedb,
  stripeCustomerTable,
  stripeSubscriptionTable,
  stripeCheckoutSessionTable,
  stripePaymentTable,
  stripeInvoiceTable,
  subscriptionWithCreationTime,
} from '../schema';
import { withAdminTx, takeRows } from '../operations';

export const get_customer = spacetimedb.procedure(
  { stripeCustomerId: t.string() },
  t.option(stripeCustomerTable.rowType),
  (ctx, { stripeCustomerId }) =>
    withAdminTx(
      ctx,
      tx =>
        tx.db.stripeCustomer.stripeCustomerId.find(stripeCustomerId) ??
        undefined
    )
);

export const get_customer_by_email = spacetimedb.procedure(
  { email: t.string() },
  t.option(stripeCustomerTable.rowType),
  (ctx, { email }) =>
    withAdminTx(ctx, tx => {
      for (const customer of tx.db.stripeCustomer.byEmail.filter(email))
        return customer;
      return undefined;
    })
);

export const get_customer_by_user_id = spacetimedb.procedure(
  { userId: t.string() },
  t.option(stripeCustomerTable.rowType),
  (ctx, { userId }) =>
    withAdminTx(ctx, tx => {
      for (const customer of tx.db.stripeCustomer.byUserId.filter(userId))
        return customer;
      return undefined;
    })
);

export const get_subscription = spacetimedb.procedure(
  { stripeSubscriptionId: t.string() },
  t.option(stripeSubscriptionTable.rowType),
  (ctx, { stripeSubscriptionId }) =>
    withAdminTx(
      ctx,
      tx =>
        tx.db.stripeSubscription.stripeSubscriptionId.find(
          stripeSubscriptionId
        ) ?? undefined
    )
);

export const list_subscriptions = spacetimedb.procedure(
  { stripeCustomerId: t.string() },
  t.array(stripeSubscriptionTable.rowType),
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripeSubscription.byCustomer.filter(stripeCustomerId))
    )
);

export const list_subscriptions_with_creation_time = spacetimedb.procedure(
  { stripeCustomerId: t.string() },
  t.array(subscriptionWithCreationTime),
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(
        tx.db.stripeSubscription.byCustomerInsertedAt.filter([
          stripeCustomerId,
          new Range(),
        ])
      ).map(sub => ({
        insertedAtMicros: sub.insertedAt.microsSinceUnixEpoch,
        stripeSubscriptionId: sub.stripeSubscriptionId,
        stripeCustomerId: sub.stripeCustomerId,
        status: sub.status,
      }))
    )
);

export const get_subscription_by_org_id = spacetimedb.procedure(
  { orgId: t.string() },
  t.option(stripeSubscriptionTable.rowType),
  (ctx, { orgId }) =>
    withAdminTx(ctx, tx => {
      const matches = takeRows(
        tx.db.stripeSubscription.byOrgInsertedAt.filter([orgId, new Range()]),
        5000
      );
      let latest = matches[0];
      for (const sub of matches) {
        if (!latest) {
          latest = sub;
          continue;
        }
        const currentMicros = sub.insertedAt.microsSinceUnixEpoch;
        const latestMicros = latest.insertedAt.microsSinceUnixEpoch;
        if (
          currentMicros > latestMicros ||
          (currentMicros === latestMicros &&
            sub.stripeSubscriptionId > latest.stripeSubscriptionId)
        ) {
          latest = sub;
        }
      }
      return latest;
    })
);

export const list_subscriptions_by_org_id = spacetimedb.procedure(
  { orgId: t.string() },
  t.array(stripeSubscriptionTable.rowType),
  (ctx, { orgId }) =>
    withAdminTx(ctx, tx =>
      takeRows(
        tx.db.stripeSubscription.byOrgInsertedAt.filter([orgId, new Range()])
      )
    )
);

export const list_subscriptions_by_user_id = spacetimedb.procedure(
  { userId: t.string() },
  t.array(stripeSubscriptionTable.rowType),
  (ctx, { userId }) =>
    withAdminTx(ctx, tx =>
      takeRows(
        tx.db.stripeSubscription.byUserInsertedAt.filter([userId, new Range()])
      )
    )
);

export const get_payment = spacetimedb.procedure(
  { stripePaymentIntentId: t.string() },
  t.option(stripePaymentTable.rowType),
  (ctx, { stripePaymentIntentId }) =>
    withAdminTx(
      ctx,
      tx =>
        tx.db.stripePayment.stripePaymentIntentId.find(stripePaymentIntentId) ??
        undefined
    )
);

export const list_payments = spacetimedb.procedure(
  { stripeCustomerId: t.string() },
  t.array(stripePaymentTable.rowType),
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripePayment.byCustomer.filter(stripeCustomerId))
    )
);

export const list_payments_by_user_id = spacetimedb.procedure(
  { userId: t.string() },
  t.array(stripePaymentTable.rowType),
  (ctx, { userId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripePayment.byUserId.filter(userId))
    )
);

export const list_payments_by_org_id = spacetimedb.procedure(
  { orgId: t.string() },
  t.array(stripePaymentTable.rowType),
  (ctx, { orgId }) =>
    withAdminTx(ctx, tx => takeRows(tx.db.stripePayment.byOrgId.filter(orgId)))
);

export const list_invoices = spacetimedb.procedure(
  { stripeCustomerId: t.string() },
  t.array(stripeInvoiceTable.rowType),
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripeInvoice.byCustomer.filter(stripeCustomerId))
    )
);

export const list_invoices_by_org_id = spacetimedb.procedure(
  { orgId: t.string() },
  t.array(stripeInvoiceTable.rowType),
  (ctx, { orgId }) =>
    withAdminTx(ctx, tx => takeRows(tx.db.stripeInvoice.byOrgId.filter(orgId)))
);

export const list_invoices_by_user_id = spacetimedb.procedure(
  { userId: t.string() },
  t.array(stripeInvoiceTable.rowType),
  (ctx, { userId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripeInvoice.byUserId.filter(userId))
    )
);

export const get_checkout_session = spacetimedb.procedure(
  { stripeCheckoutSessionId: t.string() },
  t.option(stripeCheckoutSessionTable.rowType),
  (ctx, { stripeCheckoutSessionId }) =>
    withAdminTx(
      ctx,
      tx =>
        tx.db.stripeCheckoutSession.stripeCheckoutSessionId.find(
          stripeCheckoutSessionId
        ) ?? undefined
    )
);

export const list_checkout_sessions = spacetimedb.procedure(
  { stripeCustomerId: t.string() },
  t.array(stripeCheckoutSessionTable.rowType),
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripeCheckoutSession.byCustomer.filter(stripeCustomerId))
    )
);
