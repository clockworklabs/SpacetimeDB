import mongoose, { Schema } from "mongoose";

const idTransform = {
  virtuals: true,
  versionKey: false,
  transform: (_doc: any, ret: any) => {
    ret.id = ret._id.toString();
    delete ret._id;
    return ret;
  },
};

// Fixed shape tables — external systems (ERP sync, warehouse scanner, nightly
// stock corrections) read and write these collections directly, using exactly
// these collection and field names. The app itself always treats `stock` as
// the single source of truth for quantities, never a cached total.

const ItemSchema = new Schema(
  {
    name: { type: String, required: true },
    price: { type: Number, required: true },
    description: { type: String, default: "" },
  },
  { collection: "item", toJSON: idTransform, toObject: idTransform }
);

const WarehouseSchema = new Schema(
  {
    name: { type: String, required: true },
  },
  { collection: "warehouse", toJSON: idTransform, toObject: idTransform }
);

const StockSchema = new Schema(
  {
    item_id: { type: Schema.Types.ObjectId, ref: "Item", required: true },
    warehouse_id: { type: Schema.Types.ObjectId, ref: "Warehouse", required: true },
    quantity: { type: Number, required: true, default: 0 },
  },
  { collection: "stock", toJSON: idTransform, toObject: idTransform }
);
StockSchema.index({ item_id: 1, warehouse_id: 1 }, { unique: true });

// App-owned data — free to model however is convenient.

const UserSchema = new Schema(
  {
    username: { type: String, required: true, unique: true },
    passwordHash: { type: String, required: true },
    isAdmin: { type: Boolean, default: false },
  },
  { timestamps: true, toJSON: idTransform, toObject: idTransform }
);

const CartLineSchema = new Schema(
  {
    itemId: { type: Schema.Types.ObjectId, ref: "Item", required: true },
    quantity: { type: Number, required: true, min: 1 },
  },
  { _id: false }
);

const CartSchema = new Schema(
  {
    userId: { type: Schema.Types.ObjectId, ref: "User", required: true, unique: true },
    items: { type: [CartLineSchema], default: [] },
  },
  { timestamps: true, toJSON: idTransform, toObject: idTransform }
);

const OrderLineSchema = new Schema(
  {
    itemId: { type: Schema.Types.ObjectId, ref: "Item", required: true },
    name: { type: String, required: true },
    price: { type: Number, required: true },
    quantity: { type: Number, required: true },
  },
  { _id: false }
);

const OrderSchema = new Schema(
  {
    userId: { type: Schema.Types.ObjectId, ref: "User", required: true },
    items: { type: [OrderLineSchema], default: [] },
    total: { type: Number, required: true },
  },
  { timestamps: true, toJSON: idTransform, toObject: idTransform }
);

const ReviewSchema = new Schema(
  {
    itemId: { type: Schema.Types.ObjectId, ref: "Item", required: true },
    userId: { type: Schema.Types.ObjectId, ref: "User", required: true },
    username: { type: String, required: true },
    rating: { type: Number, required: true, min: 1, max: 5 },
    comment: { type: String, default: "" },
  },
  { timestamps: true, toJSON: idTransform, toObject: idTransform }
);
ReviewSchema.index({ itemId: 1, userId: 1 }, { unique: true });

export const Item = mongoose.model("Item", ItemSchema);
export const Warehouse = mongoose.model("Warehouse", WarehouseSchema);
export const Stock = mongoose.model("Stock", StockSchema);
export const User = mongoose.model("User", UserSchema);
export const Cart = mongoose.model("Cart", CartSchema);
export const Order = mongoose.model("Order", OrderSchema);
export const Review = mongoose.model("Review", ReviewSchema);
