import { db } from "./db.js";
import { item, warehouse, stock, account } from "./schema.js";
import { hashPassword } from "./auth.js";
import { sql } from "drizzle-orm";

const ITEMS: Array<{ name: string; price: string; east: number; west: number; category: string }> = [
  { name: "Air Purifier", price: "189.00", east: 60, west: 40, category: "Home" },
  { name: "Bluetooth Speaker", price: "79.50", east: 50, west: 50, category: "Audio" },
  { name: "Coffee Grinder", price: "64.00", east: 70, west: 30, category: "Home" },
  { name: "Desk Lamp", price: "42.00", east: 55, west: 45, category: "Home" },
  { name: "Espresso Machine", price: "449.00", east: 80, west: 20, category: "Home" },
  { name: "Gaming Mouse", price: "59.00", east: 50, west: 50, category: "Computing" },
  { name: "Headphones", price: "199.00", east: 60, west: 40, category: "Audio" },
  { name: "Induction Cooktop", price: "329.00", east: 50, west: 50, category: "Home" },
  { name: "Keyboard", price: "89.00", east: 70, west: 30, category: "Computing" },
  { name: "Laptop Stand", price: "29.00", east: 90, west: 10, category: "Computing" },
  { name: "Mirrorless Camera", price: "1299.00", east: 2, west: 1, category: "Photo" },
  { name: "Webcam", price: "69.00", east: 60, west: 40, category: "Computing" },
];

export async function seed() {
  const existing = await db.select({ id: item.id }).from(item).limit(1);
  if (existing.length === 0) {
    const [east] = await db.insert(warehouse).values({ name: "East" }).returning();
    const [west] = await db.insert(warehouse).values({ name: "West" }).returning();

    for (const it of ITEMS) {
      const [inserted] = await db
        .insert(item)
        .values({ name: it.name, price: it.price, category: it.category })
        .returning();
      await db.insert(stock).values([
        { itemId: inserted.id, warehouseId: east.id, quantity: it.east },
        { itemId: inserted.id, warehouseId: west.id, quantity: it.west },
      ]);
    }
    console.log("Seeded catalogue:", ITEMS.length, "items across East/West warehouses");
  } else {
    // backfill categories for items that predate this column
    for (const it of ITEMS) {
      await db
        .update(item)
        .set({ category: it.category })
        .where(sql`name = ${it.name} AND category = ''`);
    }
  }

  const existingAdmin = await db.select({ id: account.id }).from(account).where(sql`username = 'admin'`).limit(1);
  if (existingAdmin.length === 0) {
    await db.insert(account).values({
      username: "admin",
      passwordHash: hashPassword("stackbench-admin-2026"),
      isAdmin: true,
    });
    console.log("Seeded admin account");
  }

  const existingStaff = await db.select({ id: account.id }).from(account).where(sql`username = 'staff'`).limit(1);
  if (existingStaff.length === 0) {
    await db.insert(account).values({
      username: "staff",
      passwordHash: hashPassword("stackbench-staff-2026"),
      isAdmin: false,
      isStaff: true,
    });
    console.log("Seeded staff account");
  }
}
