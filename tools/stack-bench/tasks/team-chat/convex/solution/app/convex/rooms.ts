// Reference (oracle) room/membership functions.
import { mutation, query } from "./_generated/server";
import { v } from "convex/values";
import { findMember, getRoomDoc, getUserDoc } from "./lib";

export const createRoom = mutation({
  args: { username: v.string(), room: v.string() },
  handler: async (ctx, { username, room }) => {
    await getUserDoc(ctx, username);
    if (room.length === 0) throw new Error("room name must not be empty");
    const existing = await ctx.db
      .query("rooms")
      .withIndex("by_name", (q) => q.eq("name", room))
      .unique();
    if (existing) throw new Error(`room already exists: ${room}`);
    await ctx.db.insert("rooms", { name: room, owner: username, nextSeq: 1 });
    await ctx.db.insert("members", { room, user: username, lastReadSeq: 0, unread: 0 });
  },
});

export const joinRoom = mutation({
  args: { username: v.string(), room: v.string() },
  handler: async (ctx, { username, room }) => {
    await getUserDoc(ctx, username);
    await getRoomDoc(ctx, room);
    // Idempotent: existing membership state is preserved.
    if (await findMember(ctx, room, username)) return;
    const msgs = await ctx.db
      .query("messages")
      .withIndex("by_room_seq", (q) => q.eq("room", room))
      .collect();
    const unread = msgs.filter((m) => m.sender !== username).length;
    await ctx.db.insert("members", { room, user: username, lastReadSeq: 0, unread });
  },
});

export const leaveRoom = mutation({
  args: { username: v.string(), room: v.string() },
  handler: async (ctx, { username, room }) => {
    await getUserDoc(ctx, username);
    const r = await getRoomDoc(ctx, room);
    if (r.owner === username) throw new Error("the room owner cannot leave");
    const member = await findMember(ctx, room, username);
    if (!member) throw new Error("not a member");
    await ctx.db.delete(member._id);
  },
});

export const kick = mutation({
  args: { actor: v.string(), room: v.string(), target: v.string() },
  handler: async (ctx, { actor, room, target }) => {
    await getUserDoc(ctx, actor);
    const r = await getRoomDoc(ctx, room);
    if (r.owner !== actor) throw new Error("only the room owner can kick");
    if (target === r.owner) throw new Error("cannot kick the room owner");
    const member = await findMember(ctx, room, target);
    if (!member) throw new Error("target is not a member");
    await ctx.db.delete(member._id);
  },
});

export const getOwner = query({
  args: { room: v.string() },
  handler: async (ctx, { room }) => {
    const doc = await ctx.db
      .query("rooms")
      .withIndex("by_name", (q) => q.eq("name", room))
      .unique();
    return doc ? doc.owner : null;
  },
});

export const listMembers = query({
  args: { room: v.string() },
  handler: async (ctx, { room }) => {
    const docs = await ctx.db
      .query("members")
      .withIndex("by_room", (q) => q.eq("room", room))
      .collect();
    return docs
      .map((d) => ({ user: d.user, lastReadSeq: d.lastReadSeq, unread: d.unread }))
      .sort((a, b) => (a.user < b.user ? -1 : a.user > b.user ? 1 : 0));
  },
});
