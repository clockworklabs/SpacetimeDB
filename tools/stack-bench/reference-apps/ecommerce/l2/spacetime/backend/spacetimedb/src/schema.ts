import { schema, table, t } from 'spacetimedb/server';

// --- Fixed contract tables (exact names/shapes; external systems may write these directly) ---

export const item = table(
  { name: 'item', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    price: t.f64(),
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
      { accessor: 'by_item_warehouse', algorithm: 'btree', columns: ['item_id', 'warehouse_id'] },
    ],
  },
  {
    item_id: t.u64(),
    warehouse_id: t.u64(),
    quantity: t.u32(),
  }
);

// --- App-owned tables ---

export const account = table(
  { name: 'account' },
  {
    id: t.u64().primaryKey().autoInc(),
    username: t.string().unique(),
    passwordHash: t.string(),
    isAdmin: t.bool(),
    isStaff: t.bool().default(false),
  }
);

export const session = table(
  { name: 'session' },
  {
    identity: t.identity().primaryKey(),
    accountId: t.u64(),
  }
);

export const cartItem = table(
  {
    name: 'cart_item',
    indexes: [
      { accessor: 'byAccountItem', algorithm: 'btree', columns: ['accountId', 'itemId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    accountId: t.u64(),
    itemId: t.u64(),
    quantity: t.u32(),
  }
);

export const customerOrder = table(
  {
    name: 'customer_order',
    indexes: [{ accessor: 'accountId', algorithm: 'btree', columns: ['accountId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    accountId: t.u64(),
    createdAt: t.timestamp(),
    total: t.f64(),
    status: t.string().default('pending'),
  }
);

export const orderItem = table(
  {
    name: 'order_item',
    indexes: [{ accessor: 'orderId', algorithm: 'btree', columns: ['orderId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    orderId: t.u64(),
    itemId: t.u64(),
    itemName: t.string(),
    quantity: t.u32(),
    unitPrice: t.f64(),
    returned: t.bool().default(false),
  }
);

export const orderItemStock = table(
  {
    name: 'order_item_stock',
    indexes: [{ accessor: 'orderItemId', algorithm: 'btree', columns: ['orderItemId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    orderItemId: t.u64(),
    warehouseId: t.u64(),
    quantity: t.u32(),
  }
);

export const review = table(
  {
    name: 'review',
    public: true,
    indexes: [
      { accessor: 'itemId', algorithm: 'btree', columns: ['itemId'] },
      { accessor: 'byItemAccount', algorithm: 'btree', columns: ['itemId', 'accountId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    itemId: t.u64(),
    accountId: t.u64(),
    rating: t.u32(),
    comment: t.string(),
    createdAt: t.timestamp(),
  }
);

export const itemStats = table(
  { name: 'item_stats', public: true },
  {
    itemId: t.u64().primaryKey(),
    purchaseCount: t.u32(),
  }
);

export const category = table(
  { name: 'category' },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string().unique(),
  }
);

export const itemCategory = table(
  {
    name: 'item_category',
    indexes: [{ accessor: 'byCategory', algorithm: 'btree', columns: ['categoryId'] }],
  },
  {
    itemId: t.u64().primaryKey(),
    categoryId: t.u64(),
  }
);

const spacetimedb = schema({
  item,
  warehouse,
  stock,
  account,
  session,
  cartItem,
  customerOrder,
  orderItem,
  orderItemStock,
  review,
  itemStats,
  category,
  itemCategory,
});

export default spacetimedb;
