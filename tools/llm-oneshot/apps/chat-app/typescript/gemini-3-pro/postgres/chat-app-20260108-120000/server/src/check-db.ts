import { drizzle } from 'drizzle-orm/postgres-js/index.js';
import postgres from 'postgres';
import { users } from './db/schema.js';
import dotenv from 'dotenv';

dotenv.config();

const connectionString =
  process.env.DATABASE_URL ||
  'postgres://postgres:password@localhost:5432/chat-app';
console.log('Connecting to:', connectionString);

const client = postgres(connectionString);
const db = drizzle(client);

async function main() {
  try {
    console.log('Checking database connection...');
    const result = await client`SELECT NOW()`;
    console.log('Connected:', result);

    console.log('Checking users table...');
    const allUsers = await db.select().from(users);
    console.log('Users:', allUsers);

    process.exit(0);
  } catch (error) {
    console.error('Error:', error);
    process.exit(1);
  }
}

main();
