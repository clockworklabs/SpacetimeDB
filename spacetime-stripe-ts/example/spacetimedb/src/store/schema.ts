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
import * as stripe from '@spacetimedb/stripe/submodule';

export const storeProductRow = {
  productId: t.string().primaryKey(),
  name: t.string(),
  description: t.string(),
  mode: t.string(),
  priceLabel: t.string(),
  stripePriceId: t.option(t.string()),
  perksJson: t.option(t.string()),
  active: t.bool(),
  sortOrder: t.i64(),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

// Origin that Stripe Checkout returns buyers to, set by an administrator.
export const storeConfigRow = {
  singleton: t.bool().primaryKey(),
  returnOrigin: t.string(),
  updatedAt: t.timestamp(),
};

export const storeProductTable = table(
  {
    name: 'store_product',
    public: true,
    indexes: [
      {
        accessor: 'byActiveSort',
        algorithm: 'btree',
        columns: ['active', 'sortOrder', 'productId'],
      },
      {
        accessor: 'byModeSort',
        algorithm: 'btree',
        columns: ['mode', 'sortOrder', 'productId'],
      },
      {
        accessor: 'byStripePriceId',
        algorithm: 'btree',
        columns: ['stripePriceId'],
      },
    ],
  },
  storeProductRow
);

export const storeConfigTable = table(
  { name: 'store_config', public: false, indexes: [] },
  storeConfigRow
);

export const spacetimedb = schema({
  stripe,
  storeProduct: storeProductTable,
  storeConfig: storeConfigTable,
});

export const init = spacetimedb.init(ctx => {
  stripe.install(ctx.as.stripe);
});

export default spacetimedb;

export { Range, SenderError, t };
export type ReducerModuleCtx = ReducerCtx<typeof spacetimedb.schemaType>;
export type ProcedureModuleCtx = ProcedureCtx<typeof spacetimedb.schemaType>;
export type TransactionModuleCtx = TransactionCtx<
  typeof spacetimedb.schemaType
>;
export type WriteCtx = ReducerModuleCtx | TransactionModuleCtx;
export type ModuleTimestamp = ReducerModuleCtx['timestamp'];
