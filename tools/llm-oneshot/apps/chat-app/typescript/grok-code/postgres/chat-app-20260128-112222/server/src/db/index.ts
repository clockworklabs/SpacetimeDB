import { drizzle } from 'drizzle-orm/postgres-js';
import postgres from 'postgres';
import dotenv from 'dotenv';
import * as schema from './schema';

// Load environment variables
dotenv.config();

// Database connection
const connectionString = process.env.DATABASE_URL || 'postgresql://postgres:postgres@localhost:5432/chat-app';
const client = postgres(connectionString, { prepare: false });
export const db = drizzle(client, { schema });

// Export schema for convenience
export * from './schema';