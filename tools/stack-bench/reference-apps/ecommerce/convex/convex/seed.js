import { internalMutationGeneric, internalQueryGeneric } from 'convex/server';
import { v, ConvexError } from 'convex/values';
const CATALOGUE = [
  { name: "Air Purifier", category: "Home", price: 189.0, east: 60, west: 40, description: "HEPA filtration for cleaner indoor air." },
  { name: "Bluetooth Speaker", category: "Audio", price: 79.5, east: 50, west: 50, description: "Portable speaker with rich, room-filling sound." },
  { name: "Coffee Grinder", category: "Home", price: 64.0, east: 70, west: 30, description: "Burr grinder for consistent, fresh grounds." },
  { name: "Desk Lamp", category: "Home", price: 42.0, east: 55, west: 45, description: "Adjustable LED lamp for any desk setup." },
  { name: "Espresso Machine", category: "Home", price: 449.0, east: 80, west: 20, description: "Café-quality espresso at home." },
  { name: "Gaming Mouse", category: "Computing", price: 59.0, east: 50, west: 50, description: "Precision optical mouse built for gaming." },
  { name: "Headphones", category: "Audio", price: 199.0, east: 60, west: 40, description: "Over-ear headphones with active noise cancelling." },
  { name: "Induction Cooktop", category: "Home", price: 329.0, east: 50, west: 50, description: "Fast, efficient induction cooking surface." },
  { name: "Keyboard", category: "Computing", price: 89.0, east: 70, west: 30, description: "Mechanical keyboard with tactile switches." },
  { name: "Laptop Stand", category: "Computing", price: 29.0, east: 90, west: 10, description: "Ergonomic aluminum stand for laptops." },
  { name: "Mirrorless Camera", category: "Photo", price: 1299.0, east: 2, west: 1, description: "Compact mirrorless camera for enthusiasts." },
  { name: "USB Cable", category: "Home", price: 65.0, east: 0, west: 0, description: "USB cable currently awaiting restock." },
  { name: "Webcam", category: "Computing", price: 69.0, east: 60, west: 40, description: "1080p webcam for calls and streaming." },
];
export const catalog = internalMutationGeneric({ args: {}, handler: async ctx => {
  if (await ctx.db.query('item').first()) return;
  const east = await ctx.db.insert('warehouse', { name: 'East' });
  const west = await ctx.db.insert('warehouse', { name: 'West' });
  for (const entry of CATALOGUE) {
    const itemId = await ctx.db.insert('item', { name: entry.name, price: entry.price,
      description: entry.description, category: entry.category, variants: [] });
    await ctx.db.insert('stock', { item_id: itemId, warehouse_id: east, quantity: entry.east });
    await ctx.db.insert('stock', { item_id: itemId, warehouse_id: west, quantity: entry.west });
  }
}});
export const missingAccounts = internalQueryGeneric({ args: {}, handler: async ctx => {
  const missing = [];
  for (const username of ['admin', 'staff', 'customer']) {
    if (!await ctx.db.query('order_account').withIndex('username', q => q.eq('username', username)).unique()) missing.push(username);
  }
  return missing;
}});
export const seedRole = internalMutationGeneric({ args: { username: v.string() }, handler: async (ctx, { username }) => {
  if (!['admin', 'staff', 'customer'].includes(username)) throw new ConvexError('Unknown seed account');
  const user = await ctx.db.query('order_account').withIndex('username', q => q.eq('username', username)).unique();
  if (!user) throw new ConvexError('Seed account is missing');
  await ctx.db.patch(user._id, { roles: username === 'customer' ? [] : [username] });
}});
