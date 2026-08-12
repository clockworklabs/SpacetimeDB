import { schema, table, t } from 'spacetimedb/server';

// --- Fixed shape (required by spec) -----------------------------------

export const item = table(
  { name: 'item', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string().index('btree'),
    price: t.f64(),
    purchaseCount: t.u32().default(0),
  }
);

export const warehouse = table(
  { name: 'warehouse', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
  }
);

export const stock = table(
  {
    name: 'stock',
    public: true,
    indexes: [
      { accessor: 'byItemWarehouse', algorithm: 'btree', columns: ['itemId', 'warehouseId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    itemId: t.u64(),
    warehouseId: t.u64(),
    quantity: t.u32(),
  }
);

// --- Accounts & sessions -------------------------------------------------

export const account = table(
  { name: 'account' },
  {
    id: t.u64().primaryKey().autoInc(),
    username: t.string().unique(),
    passwordSalt: t.string(),
    passwordHash: t.string(),
    isAdmin: t.bool(),
  }
);

// Maps a live connection identity to the account it is currently signed in as.
export const session = table(
  { name: 'session' },
  {
    identity: t.identity().primaryKey(),
    accountId: t.u64(),
  }
);

// --- Cart ------------------------------------------------------------------

export const cart = table(
  { name: 'cart' },
  {
    id: t.u64().primaryKey().autoInc(),
    accountId: t.u64().unique(),
  }
);

export const cartItem = table(
  {
    name: 'cart_item',
    indexes: [
      { accessor: 'byCartItem', algorithm: 'btree', columns: ['cartId', 'itemId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    cartId: t.u64(),
    itemId: t.u64(),
    quantity: t.u32(),
  }
);

// --- Orders ------------------------------------------------------------------

export const order = table(
  {
    name: 'order',
    indexes: [{ accessor: 'byAccount', algorithm: 'btree', columns: ['accountId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    accountId: t.u64(),
    total: t.f64(),
    createdAt: t.timestamp(),
  }
);

export const orderItem = table(
  {
    name: 'order_item',
    indexes: [{ accessor: 'byOrder', algorithm: 'btree', columns: ['orderId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    orderId: t.u64(),
    itemId: t.u64(),
    itemName: t.string(),
    quantity: t.u32(),
    price: t.f64(),
  }
);

// --- Reviews ------------------------------------------------------------------

export const review = table(
  {
    name: 'review',
    public: true,
    indexes: [
      { accessor: 'byItemAccount', algorithm: 'btree', columns: ['itemId', 'accountId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    itemId: t.u64(),
    accountId: t.u64(),
    rating: t.u8(),
    comment: t.string(),
    createdAt: t.timestamp(),
  }
);

const spacetimedb = schema({
  item,
  warehouse,
  stock,
  account,
  session,
  cart,
  cartItem,
  order,
  orderItem,
  review,
});

export default spacetimedb;
