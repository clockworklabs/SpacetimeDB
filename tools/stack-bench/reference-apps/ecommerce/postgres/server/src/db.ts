import "dotenv/config";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { getTableName } from "drizzle-orm";
import pg from "pg";
import { drizzle } from "drizzle-orm/node-postgres";
import * as schema from "./schema.js";

const { Pool } = pg;
const connectionString = process.env.DATABASE_URL;
if (!connectionString) throw new Error("DATABASE_URL is required");

export const pool = new Pool({
  connectionString,
});

export const db = drizzle(pool, { schema });

export async function initializeCoreSchema() {
  const names = Object.values(schema).map(getTableName);
  const result = await pool.query("SELECT count(*)::int AS count FROM information_schema.tables WHERE table_schema = 'public' AND table_name = ANY($1)", [names]);
  if (result.rows[0].count === names.length) return;
  if (result.rows[0].count !== 0) throw new Error("Reference core schema is incomplete");
  // Only a fresh database needs schema creation. A later push can delete extension data.
  execFileSync("npm", ["exec", "--", "drizzle-kit", "push", "--force"],
    { cwd: fileURLToPath(new URL("../", import.meta.url)), stdio: "inherit" });
}
