import type { Types } from "mongoose";

import { Stock } from "./models.js";

type StockModel = Pick<typeof Stock, "findOneAndUpdate" | "updateOne">;

export async function reserveWithModel(model: StockModel, itemId: Types.ObjectId, quantity: number) {
  const warehouseIds: Types.ObjectId[] = [];
  for (let index = 0; index < quantity; index += 1) {
    const stock = await model.findOneAndUpdate(
      { item_id: itemId, quantity: { $gte: 1 } },
      { $inc: { quantity: -1 } },
      { sort: { quantity: -1 } },
    );
    if (!stock) {
      await releaseWithModel(model, itemId, warehouseIds);
      return null;
    }
    warehouseIds.push(stock.warehouse_id as Types.ObjectId);
  }
  return warehouseIds;
}

export async function releaseWithModel(model: StockModel, itemId: Types.ObjectId,
  warehouseIds: Types.ObjectId[]) {
  for (const warehouseId of warehouseIds) {
    await model.updateOne({ item_id: itemId, warehouse_id: warehouseId },
      { $inc: { quantity: 1 } });
  }
}

export function reserveStock(itemId: Types.ObjectId, quantity: number) {
  return reserveWithModel(Stock, itemId, quantity);
}

export function releaseStock(itemId: Types.ObjectId, warehouseIds: Types.ObjectId[]) {
  return releaseWithModel(Stock, itemId, warehouseIds);
}
