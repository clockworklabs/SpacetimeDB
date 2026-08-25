import mongoose, { Schema } from "mongoose";

const options = { timestamps: true, toJSON: { virtuals: true, versionKey: false,
  transform: (_doc: unknown, value: any) => { value.id = String(value._id); delete value._id; } } };

const ProfileSchema = new Schema({
  userId: { type: Schema.Types.ObjectId, ref: "User", required: true, unique: true },
  name: { type: String, required: true },
  address: { type: String, required: true },
}, options);

const ReplySchema = new Schema({
  userId: { type: Schema.Types.ObjectId, ref: "User", required: true },
  username: { type: String, required: true },
  body: { type: String, required: true },
  createdAt: { type: Date, default: Date.now },
}, { _id: true, toJSON: { virtuals: true } });

const SupportTicketSchema = new Schema({
  reference: { type: String, required: true, unique: true },
  userId: { type: Schema.Types.ObjectId, ref: "User", default: null },
  email: { type: String, default: "" },
  subject: { type: String, required: true },
  message: { type: String, required: true },
  status: { type: String, default: "new" },
  priority: { type: String, default: "normal" },
  assignee: { type: String, default: "" },
  orderId: { type: Schema.Types.ObjectId, ref: "Order", default: null },
  replies: { type: [ReplySchema], default: [] },
  refundTotal: { type: Number, default: 0 },
}, options);

const PromotionSchema = new Schema({
  code: { type: String, required: true, unique: true, uppercase: true },
  discount: { type: Number, required: true, min: 0, max: 100 },
  start: { type: Date, required: true },
  end: { type: Date, required: true },
  limit: { type: Number, required: true, min: 1 },
  redemptions: { type: Number, default: 0 },
  revenue: { type: Number, default: 0 },
}, options);

const PreferenceSchema = new Schema({
  userId: { type: Schema.Types.ObjectId, ref: "User", required: true, unique: true },
  order: { type: Boolean, default: false },
  stock: { type: Boolean, default: false },
}, options);

const NotificationSchema = new Schema({
  userId: { type: Schema.Types.ObjectId, ref: "User", required: true },
  key: { type: String, required: true },
  type: { type: String, required: true },
  message: { type: String, required: true },
}, options);
NotificationSchema.index({ userId: 1, key: 1 }, { unique: true });

const StockAlertSchema = new Schema({
  userId: { type: Schema.Types.ObjectId, ref: "User", required: true },
  itemId: { type: Schema.Types.ObjectId, ref: "Item", required: true },
  sent: { type: Boolean, default: false },
}, options);
StockAlertSchema.index({ userId: 1, itemId: 1 }, { unique: true });

const ScheduledRestockSchema = new Schema({
  itemId: { type: Schema.Types.ObjectId, ref: "Item", required: true },
  warehouseId: { type: Schema.Types.ObjectId, ref: "Warehouse", required: true },
  quantity: { type: Number, required: true, min: 1 },
  dueAt: { type: Date, required: true },
  status: { type: String, enum: ["pending", "applied", "cancelled"], default: "pending" },
  source: { type: String, default: "scheduled" },
}, options);

const StockLedgerSchema = new Schema({
  restockId: { type: Schema.Types.ObjectId, ref: "ScheduledRestock", required: true, unique: true },
  itemId: { type: Schema.Types.ObjectId, ref: "Item", required: true },
  warehouseId: { type: Schema.Types.ObjectId, ref: "Warehouse", required: true },
  quantity: { type: Number, required: true },
}, options);

const ReorderRuleSchema = new Schema({
  itemId: { type: Schema.Types.ObjectId, ref: "Item", required: true, unique: true },
  threshold: { type: Number, required: true, min: 0 },
  quantity: { type: Number, required: true, min: 1 },
}, options);

const ActivitySchema = new Schema({
  actorId: { type: Schema.Types.ObjectId, ref: "User", required: true },
  actor: { type: String, required: true },
  action: { type: String, required: true },
  subject: { type: String, required: true },
}, options);

const PaymentSchema = new Schema({
  orderId: { type: Schema.Types.ObjectId, ref: "Order", required: true, unique: true },
  userId: { type: Schema.Types.ObjectId, ref: "User", required: true },
  amount: { type: Number, required: true },
  status: { type: String, default: "paid" },
}, options);

const DismissalSchema = new Schema({
  userId: { type: Schema.Types.ObjectId, ref: "User", required: true },
  itemId: { type: Schema.Types.ObjectId, ref: "Item", required: true },
}, options);
DismissalSchema.index({ userId: 1, itemId: 1 }, { unique: true });

const CartArchiveSchema = new Schema({
  userId: { type: Schema.Types.ObjectId, ref: "User", required: true, unique: true },
  items: { type: [{ itemId: Schema.Types.ObjectId, quantity: Number }], default: [] },
}, options);

export const Profile = mongoose.model("ProgressionProfile", ProfileSchema);
export const SupportTicket = mongoose.model("ProgressionSupportTicket", SupportTicketSchema);
export const Promotion = mongoose.model("ProgressionPromotion", PromotionSchema);
export const Preference = mongoose.model("ProgressionPreference", PreferenceSchema);
export const Notification = mongoose.model("ProgressionNotification", NotificationSchema);
export const StockAlert = mongoose.model("ProgressionStockAlert", StockAlertSchema);
export const ScheduledRestock = mongoose.model("ProgressionScheduledRestock", ScheduledRestockSchema);
export const StockLedger = mongoose.model("ProgressionStockLedger", StockLedgerSchema);
export const ReorderRule = mongoose.model("ProgressionReorderRule", ReorderRuleSchema);
export const Activity = mongoose.model("ProgressionActivity", ActivitySchema);
export const Payment = mongoose.model("ProgressionPayment", PaymentSchema);
export const Dismissal = mongoose.model("ProgressionDismissal", DismissalSchema);
export const CartArchive = mongoose.model("ProgressionCartArchive", CartArchiveSchema);
