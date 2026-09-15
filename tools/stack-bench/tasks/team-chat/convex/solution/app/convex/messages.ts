// Reference (oracle) message functions: idempotent sends, gapless per-room
// seq, atomic unread increments, edit, tombstone delete, monotone mark-read.
// Convex mutations are serializable transactions (OCC with retries), so the
// read-increment-write on rooms.nextSeq and the multi-row unread updates are
// atomic and the concurrency checks hold.
import { mutation, query } from "./_generated/server";
import { v } from "convex/values";
import { findMember, getRoomDoc, getUserDoc, validateText } from "./lib";

export const send = mutation({
  args: { sender: v.string(), room: v.string(), text: v.string(), clientMsgId: v.string() },
  handler: async (ctx, { sender, room, text, clientMsgId }) => {
    await getUserDoc(ctx, sender);
    const r = await getRoomDoc(ctx, room);
    if (!(await findMember(ctx, room, sender))) throw new Error("sender is not a member");
    validateText(text);
    // Idempotent retry: same clientMsgId in this room -> success, no new row.
    const dupe = await ctx.db
      .query("messages")
      .withIndex("by_room_client", (q) => q.eq("room", room).eq("clientMsgId", clientMsgId))
      .unique();
    if (dupe) return;
    const seq = r.nextSeq;
    await ctx.db.patch(r._id, { nextSeq: seq + 1 });
    await ctx.db.insert("messages", {
      room,
      seq,
      clientMsgId,
      sender,
      text,
      edited: false,
      deleted: false,
      sentAt: Date.now(),
    });
    const members = await ctx.db
      .query("members")
      .withIndex("by_room", (q) => q.eq("room", room))
      .collect();
    for (const m of members) {
      if (m.user !== sender) await ctx.db.patch(m._id, { unread: m.unread + 1 });
    }
  },
});

async function findMessage(ctx: any, room: string, clientMsgId: string) {
  const msg = await ctx.db
    .query("messages")
    .withIndex("by_room_client", (q: any) => q.eq("room", room).eq("clientMsgId", clientMsgId))
    .unique();
  if (!msg) throw new Error(`no message ${clientMsgId} in ${room}`);
  return msg;
}

export const edit = mutation({
  args: { actor: v.string(), room: v.string(), clientMsgId: v.string(), newText: v.string() },
  handler: async (ctx, { actor, room, clientMsgId, newText }) => {
    await getUserDoc(ctx, actor);
    await getRoomDoc(ctx, room);
    const msg = await findMessage(ctx, room, clientMsgId);
    if (msg.sender !== actor) throw new Error("only the original sender can edit");
    if (msg.deleted) throw new Error("cannot edit a deleted message");
    validateText(newText);
    await ctx.db.patch(msg._id, { text: newText, edited: true });
  },
});

export const remove = mutation({
  args: { actor: v.string(), room: v.string(), clientMsgId: v.string() },
  handler: async (ctx, { actor, room, clientMsgId }) => {
    await getUserDoc(ctx, actor);
    const r = await getRoomDoc(ctx, room);
    const msg = await findMessage(ctx, room, clientMsgId);
    if (msg.sender !== actor && r.owner !== actor) {
      throw new Error("only the sender or the room owner can delete");
    }
    if (msg.deleted) throw new Error("message is already deleted");
    await ctx.db.patch(msg._id, { deleted: true, text: "" });
  },
});

export const markRead = mutation({
  args: { user: v.string(), room: v.string(), upToSeq: v.number() },
  handler: async (ctx, { user, room, upToSeq }) => {
    await getUserDoc(ctx, user);
    await getRoomDoc(ctx, room);
    const member = await findMember(ctx, room, user);
    if (!member) throw new Error("not a member");
    // Monotone: lastReadSeq never decreases.
    const lastReadSeq = Math.max(member.lastReadSeq, upToSeq);
    const msgs = await ctx.db
      .query("messages")
      .withIndex("by_room_seq", (q) => q.eq("room", room))
      .collect();
    const unread = msgs.filter((m) => m.seq > lastReadSeq && m.sender !== user).length;
    await ctx.db.patch(member._id, { lastReadSeq, unread });
  },
});

export const list = query({
  args: { room: v.string() },
  handler: async (ctx, { room }) => {
    const docs = await ctx.db
      .query("messages")
      .withIndex("by_room_seq", (q) => q.eq("room", room))
      .collect();
    return docs.map((d) => ({
      seq: d.seq,
      clientMsgId: d.clientMsgId,
      sender: d.sender,
      text: d.text,
      edited: d.edited,
      deleted: d.deleted,
    }));
  },
});
