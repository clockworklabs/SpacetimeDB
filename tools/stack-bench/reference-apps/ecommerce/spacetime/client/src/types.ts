import type { Timestamp } from 'spacetimedb';

export interface ItemRow {
  id: bigint;
  name: string;
  price: number;
}

export interface WarehouseRow {
  id: bigint;
  name: string;
}

export interface StockRow {
  itemId: bigint;
  warehouseId: bigint;
  quantity: number;
}

export interface ReviewRow {
  id: bigint;
  itemId: bigint;
  accountId: bigint;
  rating: number;
  comment: string;
  createdAt: Timestamp;
}

export interface ItemStatsRow {
  itemId: bigint;
  purchaseCount: number;
}

export interface CurrentUserRow {
  id: bigint;
  username: string;
  isAdmin: boolean;
  isStaff: boolean;
}

export interface MyCartRow {
  itemId: bigint;
  quantity: number;
}

export interface MyOrderRow {
  orderId: bigint;
  createdAt: Timestamp;
  total: number;
  status: string;
}

export interface MyOrderItemRow {
  orderId: bigint;
  itemId: bigint;
  itemName: string;
  quantity: number;
  unitPrice: number;
  returned: boolean;
}

export interface AdminRevenueRow {
  total: number;
}

export interface QueueOrderRow {
  orderId: bigint;
  createdAt: Timestamp;
  itemNames: string[];
  warehouseNames: string[];
}

export interface CategoryTotalRow {
  categoryId: bigint;
  name: string;
  unitsSold: number;
  revenue: number;
}

export interface RecommendedItemRow {
  itemId: bigint;
  name: string;
  price: number;
}

export function formatMoney(n: number): string {
  return `$${n.toFixed(2)}`;
}
