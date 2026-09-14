import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import type { AddressInfo } from "node:net";
import test from "node:test";
import express from "express";
import jwt from "jsonwebtoken";
import mongoose from "mongoose";
import { Server } from "socket.io";
import { Order, User } from "./models.js";
import { Payment } from "./progression-models.js";
import { installProgressionRoutes } from "./progression.js";

test("reading state between order and payment creation cannot claim the purchase payment", {
  skip: !process.env.MONGODB_TEST_URL,
}, async t => {
  await mongoose.connect(process.env.MONGODB_TEST_URL!, {
    dbName: `stack_bench_payment_race_${randomUUID().replaceAll("-", "")}`,
  });
  t.after(async () => { await mongoose.connection.dropDatabase(); await mongoose.disconnect(); });
  await Payment.init();
  const user = await User.create({ username: "payment-race", passwordHash: "unused" });
  const order = await Order.create({ userId: user._id, total: 25, items: [] });
  const app = express();
  const secret = "local-test-only";
  installProgressionRoutes(app, new Server(), { jwtSecret: secret, ordersForUser: async () => [] });
  const server = app.listen(0, "127.0.0.1");
  await new Promise<void>(resolve => server.once("listening", resolve));
  t.after(() => new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve())));
  const token = jwt.sign({ sub: String(user._id) }, secret);
  const response = await fetch(`http://127.0.0.1:${(server.address() as AddressInfo).port}/api/progression/state`, {
    headers: { authorization: `Bearer ${token}` },
  });
  assert.equal(response.status, 200);
  // Force the exact interleaving: the order exists, a state read completes,
  // then the purchase handler creates its payment. No timing or sleeps.
  await assert.doesNotReject(() => Payment.create({ orderId: order._id, userId: user._id,
    amount: order.total, status: "paid" }), "a state read must not insert a competing payment");
  assert.equal(await Payment.countDocuments({ orderId: order._id }), 1);
});
