import type { ClientSession, Types } from "mongoose";

import { Stock } from "./models.js";

type StockModel = Pick<typeof Stock, "findOneAndUpdate" | "updateOne">;

export async function reserveWithModel(model: StockModel, itemId: Types.ObjectId, quantity: number, session?: ClientSession) {
  const warehouseIds: Types.ObjectId[] = [];
  for (let index = 0; index < quantity; index += 1) {
    const stock = await model.findOneAndUpdate(
      { item_id: itemId, quantity: { $gte: 1 } },
      { $inc: { quantity: -1 } },
      { sort: { quantity: -1 }, ...(session ? { session } : {}) },
    );
    if (!stock) {
      await releaseWithModel(model, itemId, warehouseIds, session);
      return null;
    }
    warehouseIds.push(stock.warehouse_id as Types.ObjectId);
  }
  return warehouseIds;
}

export async function releaseWithModel(model: StockModel, itemId: Types.ObjectId,
  warehouseIds: Types.ObjectId[], session?: ClientSession) {
  for (const warehouseId of warehouseIds) {
    await model.updateOne({ item_id: itemId, warehouse_id: warehouseId },
      { $inc: { quantity: 1 } }, session ? { session } : {});
  }
}

export function reserveStock(itemId: Types.ObjectId, quantity: number, session?: ClientSession) {
  return reserveWithModel(Stock, itemId, quantity, session);
}

export function releaseStock(itemId: Types.ObjectId, warehouseIds: Types.ObjectId[], session?: ClientSession) {
  return releaseWithModel(Stock, itemId, warehouseIds, session);
}
