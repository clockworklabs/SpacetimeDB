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

// Identities allowed to mutate the catalog. Fresh publishes seed the owner via init.
export const storeAdminIdentityRow = {
  identity: t.identity().primaryKey(),
  addedAtMicros: t.i64(),
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

export const storeAdminIdentityTable = table(
  { name: 'store_admin_identity', public: false, indexes: [] },
  storeAdminIdentityRow
);

export const spacetimedb = schema({
  stripe,
  storeProduct: storeProductTable,
  storeAdminIdentity: storeAdminIdentityTable,
});

export const init = spacetimedb.init(ctx => {
  installStore(ctx);
  stripe.installStripe(ctx.as.stripe);
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

export function installStore(ctx: ReducerModuleCtx) {
  if (ctx.db.storeAdminIdentity.identity.find(ctx.sender) != null) return;
  ctx.db.storeAdminIdentity.insert({
    identity: ctx.sender,
    addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
  });
}
