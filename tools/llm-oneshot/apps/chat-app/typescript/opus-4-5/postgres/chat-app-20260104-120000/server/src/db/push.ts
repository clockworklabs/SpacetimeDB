import { drizzle } from 'drizzle-orm/postgres-js';
import { migrate } from 'drizzle-orm/postgres-js/migrator';
import postgres from 'postgres';
import * as schema from './schema.js';

const connectionString = process.env.DATABASE_URL || 'postgres://postgres:postgres@localhost:5432/chat-app';

async function push() {
  console.log('Pushing schema to database...');
  
  const client = postgres(connectionString, { max: 1 });
  const db = drizzle(client, { schema });
  
  // Create tables using raw SQL based on schema
  await client`
    CREATE TABLE IF NOT EXISTS users (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      display_name VARCHAR(50) NOT NULL,
      status VARCHAR(20) NOT NULL DEFAULT 'online',
      last_active_at TIMESTAMP NOT NULL DEFAULT NOW(),
      last_action_at TIMESTAMP,
      created_at TIMESTAMP NOT NULL DEFAULT NOW()
    )
  `;
  
  await client`
    CREATE TABLE IF NOT EXISTS rooms (
      id SERIAL PRIMARY KEY,
      name VARCHAR(100) NOT NULL,
      is_private BOOLEAN NOT NULL DEFAULT FALSE,
      is_dm BOOLEAN NOT NULL DEFAULT FALSE,
      created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      created_at TIMESTAMP NOT NULL DEFAULT NOW()
    )
  `;
  
  await client`
    CREATE TABLE IF NOT EXISTS room_members (
      id SERIAL PRIMARY KEY,
      room_id INTEGER NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
      user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      is_admin BOOLEAN NOT NULL DEFAULT FALSE,
      is_banned BOOLEAN NOT NULL DEFAULT FALSE,
      last_read_at TIMESTAMP,
      joined_at TIMESTAMP NOT NULL DEFAULT NOW(),
      UNIQUE(room_id, user_id)
    )
  `;
  
  await client`CREATE INDEX IF NOT EXISTS room_members_room_idx ON room_members(room_id)`;
  await client`CREATE INDEX IF NOT EXISTS room_members_user_idx ON room_members(user_id)`;
  
  await client`
    CREATE TABLE IF NOT EXISTS room_invitations (
      id SERIAL PRIMARY KEY,
      room_id INTEGER NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
      invited_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      invited_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      status VARCHAR(20) NOT NULL DEFAULT 'pending',
      created_at TIMESTAMP NOT NULL DEFAULT NOW(),
      UNIQUE(room_id, invited_user_id)
    )
  `;
  
  await client`CREATE INDEX IF NOT EXISTS invitations_user_idx ON room_invitations(invited_user_id)`;
  
  await client`
    CREATE TABLE IF NOT EXISTS messages (
      id SERIAL PRIMARY KEY,
      room_id INTEGER NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
      user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      content VARCHAR(2000) NOT NULL,
      is_edited BOOLEAN NOT NULL DEFAULT FALSE,
      reply_to_id INTEGER,
      scheduled_for TIMESTAMP,
      expires_at TIMESTAMP,
      created_at TIMESTAMP NOT NULL DEFAULT NOW()
    )
  `;
  
  await client`CREATE INDEX IF NOT EXISTS messages_room_idx ON messages(room_id)`;
  await client`CREATE INDEX IF NOT EXISTS messages_scheduled_idx ON messages(scheduled_for)`;
  await client`CREATE INDEX IF NOT EXISTS messages_expires_idx ON messages(expires_at)`;
  await client`CREATE INDEX IF NOT EXISTS messages_reply_idx ON messages(reply_to_id)`;
  
  await client`
    CREATE TABLE IF NOT EXISTS message_edits (
      id SERIAL PRIMARY KEY,
      message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
      previous_content VARCHAR(2000) NOT NULL,
      edited_at TIMESTAMP NOT NULL DEFAULT NOW()
    )
  `;
  
  await client`CREATE INDEX IF NOT EXISTS edits_message_idx ON message_edits(message_id)`;
  
  await client`
    CREATE TABLE IF NOT EXISTS message_reactions (
      id SERIAL PRIMARY KEY,
      message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
      user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      emoji VARCHAR(10) NOT NULL,
      created_at TIMESTAMP NOT NULL DEFAULT NOW(),
      UNIQUE(message_id, user_id, emoji)
    )
  `;
  
  await client`CREATE INDEX IF NOT EXISTS reactions_message_idx ON message_reactions(message_id)`;
  
  await client`
    CREATE TABLE IF NOT EXISTS read_receipts (
      id SERIAL PRIMARY KEY,
      message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
      user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      read_at TIMESTAMP NOT NULL DEFAULT NOW(),
      UNIQUE(message_id, user_id)
    )
  `;
  
  await client`CREATE INDEX IF NOT EXISTS receipts_message_idx ON read_receipts(message_id)`;
  await client`CREATE INDEX IF NOT EXISTS receipts_user_idx ON read_receipts(user_id)`;
  
  await client`
    CREATE TABLE IF NOT EXISTS typing_indicators (
      id SERIAL PRIMARY KEY,
      room_id INTEGER NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
      user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      started_at TIMESTAMP NOT NULL DEFAULT NOW(),
      UNIQUE(room_id, user_id)
    )
  `;
  
  await client`CREATE INDEX IF NOT EXISTS typing_room_idx ON typing_indicators(room_id)`;
  
  console.log('Schema pushed successfully!');
  await client.end();
  process.exit(0);
}

push().catch((err) => {
  console.error('Error pushing schema:', err);
  process.exit(1);
});
