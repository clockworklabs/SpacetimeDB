// Shared helpers for the oracle's Convex functions.
import type { MutationCtx, QueryCtx } from "./_generated/server";

export const STATUSES = ["online", "away", "offline"];
export const MAX_TEXT = 4000;

export async function getUserDoc(ctx: QueryCtx | MutationCtx, username: string) {
  const doc = await ctx.db
    .query("users")
    .withIndex("by_username", (q) => q.eq("username", username))
    .unique();
  if (!doc) throw new Error(`unknown user: ${username}`);
  return doc;
}

export async function getRoomDoc(ctx: QueryCtx | MutationCtx, name: string) {
  const doc = await ctx.db
    .query("rooms")
    .withIndex("by_name", (q) => q.eq("name", name))
    .unique();
  if (!doc) throw new Error(`unknown room: ${name}`);
  return doc;
}

export async function findMember(ctx: QueryCtx | MutationCtx, room: string, user: string) {
  return await ctx.db
    .query("members")
    .withIndex("by_room_user", (q) => q.eq("room", room).eq("user", user))
    .unique();
}

export function validateText(text: string) {
  if (text.length === 0) throw new Error("text must not be empty");
  if (text.length > MAX_TEXT) throw new Error(`text exceeds ${MAX_TEXT} characters`);
}
