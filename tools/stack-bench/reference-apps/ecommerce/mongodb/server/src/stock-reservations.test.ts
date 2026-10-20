import assert from "node:assert/strict";
import test from "node:test";

import { reserveWithModel } from "./stock-reservations.js";

test("only one simultaneous cart can reserve the final unit", async () => {
  const itemId = "item-1" as any;
  const warehouseId = "warehouse-1" as any;
  let quantity = 1;
  const model = {
    async findOneAndUpdate() {
      await Promise.resolve();
      if (quantity < 1) return null;
      quantity -= 1;
      return { warehouse_id: warehouseId };
    },
    async updateOne() {
      quantity += 1;
      return { modifiedCount: 1 };
    },
  } as any;

  const results = await Promise.all([
    reserveWithModel(model, itemId, 1),
    reserveWithModel(model, itemId, 1),
  ]);

  assert.equal(results.filter(Boolean).length, 1);
  assert.equal(results.filter(result => result === null).length, 1);
  assert.equal(quantity, 0);
});
