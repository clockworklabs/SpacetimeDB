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
import { latestSubscription } from '../subscription-order';

export const getCustomer = spacetimedb.procedure(
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

export const getCustomerByEmail = spacetimedb.procedure(
  { email: t.string() },
  t.option(stripeCustomerTable.rowType),
  (ctx, { email }) =>
    withAdminTx(ctx, tx => {
      for (const customer of tx.db.stripeCustomer.byEmail.filter(email))
        return customer;
      return undefined;
    })
);

export const getCustomerByUserId = spacetimedb.procedure(
  { userId: t.string() },
  t.option(stripeCustomerTable.rowType),
  (ctx, { userId }) =>
    withAdminTx(ctx, tx => {
      for (const customer of tx.db.stripeCustomer.byUserId.filter(userId))
        return customer;
      return undefined;
    })
);

export const getSubscription = spacetimedb.procedure(
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

export const listSubscriptions = spacetimedb.procedure(
  { stripeCustomerId: t.string() },
  t.array(stripeSubscriptionTable.rowType),
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripeSubscription.byCustomer.filter(stripeCustomerId))
    )
);

export const listSubscriptionsWithCreationTime = spacetimedb.procedure(
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

export const getSubscriptionByOrgId = spacetimedb.procedure(
  { orgId: t.string() },
  t.option(stripeSubscriptionTable.rowType),
  (ctx, { orgId }) =>
    withAdminTx(ctx, tx =>
      latestSubscription(
        tx.db.stripeSubscription.byOrgInsertedAt.filter([orgId, new Range()])
      )
    )
);

export const listSubscriptionsByOrgId = spacetimedb.procedure(
  { orgId: t.string() },
  t.array(stripeSubscriptionTable.rowType),
  (ctx, { orgId }) =>
    withAdminTx(ctx, tx =>
      takeRows(
        tx.db.stripeSubscription.byOrgInsertedAt.filter([orgId, new Range()])
      )
    )
);

export const listSubscriptionsByUserId = spacetimedb.procedure(
  { userId: t.string() },
  t.array(stripeSubscriptionTable.rowType),
  (ctx, { userId }) =>
    withAdminTx(ctx, tx =>
      takeRows(
        tx.db.stripeSubscription.byUserInsertedAt.filter([userId, new Range()])
      )
    )
);

export const getPayment = spacetimedb.procedure(
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

export const listPayments = spacetimedb.procedure(
  { stripeCustomerId: t.string() },
  t.array(stripePaymentTable.rowType),
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripePayment.byCustomer.filter(stripeCustomerId))
    )
);

export const listPaymentsByUserId = spacetimedb.procedure(
  { userId: t.string() },
  t.array(stripePaymentTable.rowType),
  (ctx, { userId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripePayment.byUserId.filter(userId))
    )
);

export const listPaymentsByOrgId = spacetimedb.procedure(
  { orgId: t.string() },
  t.array(stripePaymentTable.rowType),
  (ctx, { orgId }) =>
    withAdminTx(ctx, tx => takeRows(tx.db.stripePayment.byOrgId.filter(orgId)))
);

export const listInvoices = spacetimedb.procedure(
  { stripeCustomerId: t.string() },
  t.array(stripeInvoiceTable.rowType),
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripeInvoice.byCustomer.filter(stripeCustomerId))
    )
);

export const listInvoicesByOrgId = spacetimedb.procedure(
  { orgId: t.string() },
  t.array(stripeInvoiceTable.rowType),
  (ctx, { orgId }) =>
    withAdminTx(ctx, tx => takeRows(tx.db.stripeInvoice.byOrgId.filter(orgId)))
);

export const listInvoicesByUserId = spacetimedb.procedure(
  { userId: t.string() },
  t.array(stripeInvoiceTable.rowType),
  (ctx, { userId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripeInvoice.byUserId.filter(userId))
    )
);

export const getCheckoutSession = spacetimedb.procedure(
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

export const listCheckoutSessions = spacetimedb.procedure(
  { stripeCustomerId: t.string() },
  t.array(stripeCheckoutSessionTable.rowType),
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripeCheckoutSession.byCustomer.filter(stripeCustomerId))
    )
);
