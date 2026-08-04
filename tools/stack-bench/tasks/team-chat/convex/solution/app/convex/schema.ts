// Reference (oracle) Convex schema for the stack-bench team-chat task.
import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export default defineSchema({
  users: defineTable({
    username: v.string(),
    status: v.string(),
    balance: v.number(),
  }).index("by_username", ["username"]),

  rooms: defineTable({
    name: v.string(),
    owner: v.string(),
    nextSeq: v.number(),
  }).index("by_name", ["name"]),

  members: defineTable({
    room: v.string(),
    user: v.string(),
    lastReadSeq: v.number(),
    unread: v.number(),
  })
    .index("by_room", ["room"])
    .index("by_room_user", ["room", "user"]),

  messages: defineTable({
    room: v.string(),
    seq: v.number(),
    clientMsgId: v.string(),
    sender: v.string(),
    text: v.string(),
    edited: v.boolean(),
    deleted: v.boolean(),
    sentAt: v.number(),
  })
    .index("by_room_seq", ["room", "seq"])
    .index("by_room_client", ["room", "clientMsgId"]),
});
