import type {
  ArrayBuilder,
  OptionBuilder,
  ProcedureExport,
} from 'spacetimedb/server';
import {
  Range,
  t,
  spacetimedb,
  stripeCustomerTable,
  stripeSubscriptionTable,
  stripeCheckoutSessionTable,
  stripePaymentTable,
  stripeInvoiceTable,
  stripeWebhookEventTable,
} from '../schema.js';
import { withAdminTx, takeRows } from '../operations.js';
import { latestSubscription } from '../subscription-order.js';

const getCustomerArgs = { stripeCustomerId: t.string() };
const getCustomerResult: OptionBuilder<typeof stripeCustomerTable.rowType> =
  t.option(stripeCustomerTable.rowType);
export const getCustomer: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof getCustomerArgs,
  typeof getCustomerResult
> = spacetimedb.procedure(
  getCustomerArgs,
  getCustomerResult,
  (ctx, { stripeCustomerId }) =>
    withAdminTx(
      ctx,
      tx =>
        tx.db.stripeCustomer.stripeCustomerId.find(stripeCustomerId) ??
        undefined
    )
);

const getCustomerByUserIdArgs = { userId: t.string() };
const getCustomerByUserIdResult: OptionBuilder<
  typeof stripeCustomerTable.rowType
> = t.option(stripeCustomerTable.rowType);
export const getCustomerByUserId: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof getCustomerByUserIdArgs,
  typeof getCustomerByUserIdResult
> = spacetimedb.procedure(
  getCustomerByUserIdArgs,
  getCustomerByUserIdResult,
  (ctx, { userId }) =>
    withAdminTx(ctx, tx => {
      for (const customer of tx.db.stripeCustomer.byUserId.filter(userId))
        return customer;
      return undefined;
    })
);

const getSubscriptionArgs = { stripeSubscriptionId: t.string() };
const getSubscriptionResult: OptionBuilder<
  typeof stripeSubscriptionTable.rowType
> = t.option(stripeSubscriptionTable.rowType);
export const getSubscription: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof getSubscriptionArgs,
  typeof getSubscriptionResult
> = spacetimedb.procedure(
  getSubscriptionArgs,
  getSubscriptionResult,
  (ctx, { stripeSubscriptionId }) =>
    withAdminTx(
      ctx,
      tx =>
        tx.db.stripeSubscription.stripeSubscriptionId.find(
          stripeSubscriptionId
        ) ?? undefined
    )
);

const listSubscriptionsArgs = { stripeCustomerId: t.string() };
const listSubscriptionsResult: ArrayBuilder<
  typeof stripeSubscriptionTable.rowType
> = t.array(stripeSubscriptionTable.rowType);
export const listSubscriptions: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listSubscriptionsArgs,
  typeof listSubscriptionsResult
> = spacetimedb.procedure(
  listSubscriptionsArgs,
  listSubscriptionsResult,
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripeSubscription.byCustomer.filter(stripeCustomerId))
    )
);

const getSubscriptionByOrgIdArgs = { orgId: t.string() };
const getSubscriptionByOrgIdResult: OptionBuilder<
  typeof stripeSubscriptionTable.rowType
> = t.option(stripeSubscriptionTable.rowType);
export const getSubscriptionByOrgId: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof getSubscriptionByOrgIdArgs,
  typeof getSubscriptionByOrgIdResult
> = spacetimedb.procedure(
  getSubscriptionByOrgIdArgs,
  getSubscriptionByOrgIdResult,
  (ctx, { orgId }) =>
    withAdminTx(ctx, tx =>
      latestSubscription(
        tx.db.stripeSubscription.byOrgInsertedAt.filter([orgId, new Range()])
      )
    )
);

const listSubscriptionsByOrgIdArgs = { orgId: t.string() };
const listSubscriptionsByOrgIdResult: ArrayBuilder<
  typeof stripeSubscriptionTable.rowType
> = t.array(stripeSubscriptionTable.rowType);
export const listSubscriptionsByOrgId: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listSubscriptionsByOrgIdArgs,
  typeof listSubscriptionsByOrgIdResult
> = spacetimedb.procedure(
  listSubscriptionsByOrgIdArgs,
  listSubscriptionsByOrgIdResult,
  (ctx, { orgId }) =>
    withAdminTx(ctx, tx =>
      takeRows(
        tx.db.stripeSubscription.byOrgInsertedAt.filter([orgId, new Range()])
      )
    )
);

const listSubscriptionsByUserIdArgs = { userId: t.string() };
const listSubscriptionsByUserIdResult: ArrayBuilder<
  typeof stripeSubscriptionTable.rowType
> = t.array(stripeSubscriptionTable.rowType);
export const listSubscriptionsByUserId: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listSubscriptionsByUserIdArgs,
  typeof listSubscriptionsByUserIdResult
> = spacetimedb.procedure(
  listSubscriptionsByUserIdArgs,
  listSubscriptionsByUserIdResult,
  (ctx, { userId }) =>
    withAdminTx(ctx, tx =>
      takeRows(
        tx.db.stripeSubscription.byUserInsertedAt.filter([userId, new Range()])
      )
    )
);

const getPaymentArgs = { stripePaymentIntentId: t.string() };
const getPaymentResult: OptionBuilder<typeof stripePaymentTable.rowType> =
  t.option(stripePaymentTable.rowType);
export const getPayment: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof getPaymentArgs,
  typeof getPaymentResult
> = spacetimedb.procedure(
  getPaymentArgs,
  getPaymentResult,
  (ctx, { stripePaymentIntentId }) =>
    withAdminTx(
      ctx,
      tx =>
        tx.db.stripePayment.stripePaymentIntentId.find(stripePaymentIntentId) ??
        undefined
    )
);

const listPaymentsArgs = { stripeCustomerId: t.string() };
const listPaymentsResult: ArrayBuilder<typeof stripePaymentTable.rowType> =
  t.array(stripePaymentTable.rowType);
export const listPayments: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listPaymentsArgs,
  typeof listPaymentsResult
> = spacetimedb.procedure(
  listPaymentsArgs,
  listPaymentsResult,
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripePayment.byCustomer.filter(stripeCustomerId))
    )
);

const listPaymentsByUserIdArgs = { userId: t.string() };
const listPaymentsByUserIdResult: ArrayBuilder<
  typeof stripePaymentTable.rowType
> = t.array(stripePaymentTable.rowType);
export const listPaymentsByUserId: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listPaymentsByUserIdArgs,
  typeof listPaymentsByUserIdResult
> = spacetimedb.procedure(
  listPaymentsByUserIdArgs,
  listPaymentsByUserIdResult,
  (ctx, { userId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripePayment.byUserId.filter(userId))
    )
);

const listPaymentsByOrgIdArgs = { orgId: t.string() };
const listPaymentsByOrgIdResult: ArrayBuilder<
  typeof stripePaymentTable.rowType
> = t.array(stripePaymentTable.rowType);
export const listPaymentsByOrgId: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listPaymentsByOrgIdArgs,
  typeof listPaymentsByOrgIdResult
> = spacetimedb.procedure(
  listPaymentsByOrgIdArgs,
  listPaymentsByOrgIdResult,
  (ctx, { orgId }) =>
    withAdminTx(ctx, tx => takeRows(tx.db.stripePayment.byOrgId.filter(orgId)))
);

const listInvoicesArgs = { stripeCustomerId: t.string() };
const listInvoicesResult: ArrayBuilder<typeof stripeInvoiceTable.rowType> =
  t.array(stripeInvoiceTable.rowType);
export const listInvoices: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listInvoicesArgs,
  typeof listInvoicesResult
> = spacetimedb.procedure(
  listInvoicesArgs,
  listInvoicesResult,
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripeInvoice.byCustomer.filter(stripeCustomerId))
    )
);

const listInvoicesByOrgIdArgs = { orgId: t.string() };
const listInvoicesByOrgIdResult: ArrayBuilder<
  typeof stripeInvoiceTable.rowType
> = t.array(stripeInvoiceTable.rowType);
export const listInvoicesByOrgId: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listInvoicesByOrgIdArgs,
  typeof listInvoicesByOrgIdResult
> = spacetimedb.procedure(
  listInvoicesByOrgIdArgs,
  listInvoicesByOrgIdResult,
  (ctx, { orgId }) =>
    withAdminTx(ctx, tx => takeRows(tx.db.stripeInvoice.byOrgId.filter(orgId)))
);

const listInvoicesByUserIdArgs = { userId: t.string() };
const listInvoicesByUserIdResult: ArrayBuilder<
  typeof stripeInvoiceTable.rowType
> = t.array(stripeInvoiceTable.rowType);
export const listInvoicesByUserId: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listInvoicesByUserIdArgs,
  typeof listInvoicesByUserIdResult
> = spacetimedb.procedure(
  listInvoicesByUserIdArgs,
  listInvoicesByUserIdResult,
  (ctx, { userId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripeInvoice.byUserId.filter(userId))
    )
);

const getCheckoutSessionArgs = { stripeCheckoutSessionId: t.string() };
const getCheckoutSessionResult: OptionBuilder<
  typeof stripeCheckoutSessionTable.rowType
> = t.option(stripeCheckoutSessionTable.rowType);
export const getCheckoutSession: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof getCheckoutSessionArgs,
  typeof getCheckoutSessionResult
> = spacetimedb.procedure(
  getCheckoutSessionArgs,
  getCheckoutSessionResult,
  (ctx, { stripeCheckoutSessionId }) =>
    withAdminTx(
      ctx,
      tx =>
        tx.db.stripeCheckoutSession.stripeCheckoutSessionId.find(
          stripeCheckoutSessionId
        ) ?? undefined
    )
);

const listCheckoutSessionsArgs = { stripeCustomerId: t.string() };
const listCheckoutSessionsResult: ArrayBuilder<
  typeof stripeCheckoutSessionTable.rowType
> = t.array(stripeCheckoutSessionTable.rowType);
export const listCheckoutSessions: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listCheckoutSessionsArgs,
  typeof listCheckoutSessionsResult
> = spacetimedb.procedure(
  listCheckoutSessionsArgs,
  listCheckoutSessionsResult,
  (ctx, { stripeCustomerId }) =>
    withAdminTx(ctx, tx =>
      takeRows(tx.db.stripeCheckoutSession.byCustomer.filter(stripeCustomerId))
    )
);

const getWebhookEventArgs = { eventId: t.string() };
const getWebhookEventResult: OptionBuilder<
  typeof stripeWebhookEventTable.rowType
> = t.option(stripeWebhookEventTable.rowType);
export const getWebhookEvent: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof getWebhookEventArgs,
  typeof getWebhookEventResult
> = spacetimedb.procedure(
  getWebhookEventArgs,
  getWebhookEventResult,
  (ctx, { eventId }) =>
    withAdminTx(
      ctx,
      tx => tx.db.stripeWebhookEvent.eventId.find(eventId) ?? undefined
    )
);
