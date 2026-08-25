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
    discount: t.f64().default(0),
    promotionId: t.option(t.u64()),
    refundedTotal: t.f64().default(0),
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

export const customerProfile = table(
  { name: 'customer_profile' },
  {
    accountId: t.u64().primaryKey(),
    name: t.string(),
    address: t.string(),
  }
);

export const staffRole = table(
  { name: 'staff_role' },
  {
    accountId: t.u64().primaryKey(),
    role: t.string(),
  }
);

export const itemVariant = table(
  { name: 'item_variant', public: true, indexes: [{ accessor: 'itemId', algorithm: 'btree', columns: ['itemId'] }] },
  {
    id: t.u64().primaryKey().autoInc(),
    itemId: t.u64(),
    name: t.string(),
  }
);

export const paymentRecord = table(
  { name: 'payment_record', indexes: [{ accessor: 'orderId', algorithm: 'btree', columns: ['orderId'] }] },
  {
    id: t.u64().primaryKey().autoInc(),
    orderId: t.u64(),
    amount: t.f64(),
    status: t.string(),
  }
);

export const staffActivity = table(
  { name: 'staff_activity', indexes: [{ accessor: 'actorAccountId', algorithm: 'btree', columns: ['actorAccountId'] }] },
  {
    id: t.u64().primaryKey().autoInc(),
    actorAccountId: t.u64(),
    action: t.string(),
    subject: t.string(),
    createdMicros: t.i64(),
  }
);

export const supportTicket = table(
  { name: 'support_ticket', indexes: [{ accessor: 'accountId', algorithm: 'btree', columns: ['accountId'] }] },
  {
    id: t.u64().primaryKey().autoInc(),
    reference: t.string().unique(),
    accountId: t.option(t.u64()),
    email: t.string(),
    subject: t.string(),
    message: t.string(),
    status: t.string(),
    priority: t.string(),
    assigneeId: t.option(t.u64()),
    orderId: t.option(t.u64()),
    refundTotal: t.f64(),
  }
);

export const supportReply = table(
  { name: 'support_reply', indexes: [{ accessor: 'ticketId', algorithm: 'btree', columns: ['ticketId'] }] },
  {
    id: t.u64().primaryKey().autoInc(),
    ticketId: t.u64(),
    accountId: t.u64(),
    body: t.string(),
    createdMicros: t.i64(),
  }
);

export const promotion = table(
  { name: 'promotion' },
  {
    id: t.u64().primaryKey().autoInc(),
    code: t.string().unique(),
    discountPercent: t.f64(),
    startMicros: t.i64(),
    endMicros: t.i64(),
    usageLimit: t.u32(),
    redemptions: t.u32(),
  }
);

export const cartPromotion = table(
  { name: 'cart_promotion' },
  {
    accountId: t.u64().primaryKey(),
    promotionId: t.u64(),
  }
);

export const notificationPreference = table(
  { name: 'notification_preference' },
  {
    accountId: t.u64().primaryKey(),
    orderEnabled: t.bool(),
    stockEnabled: t.bool(),
  }
);

export const stockAlert = table(
  { name: 'stock_alert', indexes: [{ accessor: 'itemId', algorithm: 'btree', columns: ['itemId'] }] },
  {
    id: t.u64().primaryKey().autoInc(),
    accountId: t.u64(),
    itemId: t.u64(),
    fulfilled: t.bool(),
  }
);

export const notification = table(
  { name: 'notification', indexes: [{ accessor: 'accountId', algorithm: 'btree', columns: ['accountId'] }] },
  {
    id: t.u64().primaryKey().autoInc(),
    accountId: t.u64(),
    kind: t.string(),
    message: t.string(),
    unread: t.bool(),
  }
);

export const reservation = table(
  { name: 'reservation', indexes: [{ accessor: 'byAccountItem', algorithm: 'btree', columns: ['accountId', 'itemId'] }] },
  {
    id: t.u64().primaryKey().autoInc(),
    accountId: t.u64(),
    itemId: t.u64(),
    warehouseId: t.u64(),
    quantity: t.u32(),
    expiresMicros: t.i64(),
    expired: t.bool(),
  }
);

export const expiredCartItem = table(
  { name: 'expired_cart_item', indexes: [{ accessor: 'accountId', algorithm: 'btree', columns: ['accountId'] }] },
  {
    id: t.u64().primaryKey().autoInc(),
    accountId: t.u64(),
    itemId: t.u64(),
    quantity: t.u32(),
  }
);

export const cartExpiry = table(
  { name: 'cart_expiry' },
  {
    accountId: t.u64().primaryKey(),
    expiresMicros: t.i64(),
    expired: t.bool(),
  }
);

export const scheduledRestock = table(
  { name: 'scheduled_restock' },
  {
    id: t.u64().primaryKey().autoInc(),
    itemId: t.u64(),
    warehouseId: t.u64(),
    quantity: t.u32(),
    dueMicros: t.i64(),
    status: t.string(),
  }
);

export const stockLedger = table(
  { name: 'stock_ledger' },
  {
    id: t.u64().primaryKey().autoInc(),
    itemId: t.u64(),
    warehouseId: t.u64(),
    quantity: t.u32(),
    createdMicros: t.i64(),
    source: t.string(),
  }
);

export const deliverySchedule = table(
  { name: 'delivery_schedule' },
  {
    orderId: t.u64().primaryKey(),
    dueMicros: t.i64(),
    completed: t.bool(),
  }
);

export const reorderRule = table(
  { name: 'reorder_rule' },
  {
    id: t.u64().primaryKey().autoInc(),
    itemId: t.u64(),
    warehouseId: t.u64(),
    threshold: t.u32(),
    quantity: t.u32(),
  }
);

export const recommendationDismissal = table(
  { name: 'recommendation_dismissal', indexes: [{ accessor: 'byAccountItem', algorithm: 'btree', columns: ['accountId', 'itemId'] }] },
  {
    id: t.u64().primaryKey().autoInc(),
    accountId: t.u64(),
    itemId: t.u64(),
  }
);

export const maintenanceTick = table(
  { name: 'maintenance_tick' },
  {
    id: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
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
  customerProfile,
  staffRole,
  itemVariant,
  paymentRecord,
  staffActivity,
  supportTicket,
  supportReply,
  promotion,
  cartPromotion,
  notificationPreference,
  stockAlert,
  notification,
  reservation,
  expiredCartItem,
  cartExpiry,
  scheduledRestock,
  stockLedger,
  deliverySchedule,
  reorderRule,
  recommendationDismissal,
  maintenanceTick,
});

export default spacetimedb;
