import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { Identity } from 'spacetimedb';
import {
  DbConnection,
  ErrorContext,
  EventContext,
} from './module_bindings/index.js';

// Configuration
const HOST = process.env.SPACETIMEDB_HOST ?? 'ws://localhost:3000';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'nodejs-ts';

// Main entry point
async function main(): Promise<void> {
  console.log(`Connecting to SpacetimeDB...`);
  console.log(`  URI: ${HOST}`);
  console.log(`  Module: ${DB_NAME}`);

  // Build and establish connection
  DbConnection.builder()
    .withUri(HOST)
    .withDatabaseName(DB_NAME)
    .withToken(loadToken())
    .onConnect(onConnect)
    .onDisconnect(onDisconnect)
    .onConnectError(onConnectError)
    .build();
}

function onConnect(
  conn: DbConnection,
  identity: Identity,
  token: string
): void {
  console.log('\nConnected to SpacetimeDB!');
  console.log(`Identity: ${identity.toHexString().slice(0, 16)}...`);

  // Save token for future connections
  saveToken(token);

  // Subscribe to all tables
  conn
    .subscriptionBuilder()
    .onApplied(ctx => {
      // Show current people
      const people = [...ctx.db.person.iter()];
      console.log(`\nCurrent people (${people.length}):`);
      if (people.length === 0) {
        console.log('  (none yet)');
      } else {
        for (const person of people) {
          console.log(`  - ${person.name}`);
        }
      }

      console.log('\nPress Ctrl+C to exit');
    })
    .onError((_ctx, err) => {
      console.error('Subscription error:', err);
    })
    .subscribeToAllTables();

  // Register callbacks for table changes
  conn.db.person.onInsert((_ctx: EventContext, person) => {
    console.log(`[Added] ${person.name}`);
  });

  conn.db.person.onDelete((_ctx: EventContext, person) => {
    console.log(`[Removed] ${person.name}`);
  });
}

function onDisconnect(_ctx: ErrorContext, error?: Error): void {
  if (error) {
    console.error('Disconnected with error:', error);
  } else {
    console.log('Disconnected from SpacetimeDB');
  }
}

function onConnectError(_ctx: ErrorContext, error: Error): void {
  console.error('Connection error:', error);
  process.exit(1);
}

// Token persistence (file-based for Node.js instead of localStorage)
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TOKEN_FILE = path.join(__dirname, '..', '.spacetimedb-token');

function loadToken(): string | undefined {
  try {
    if (fs.existsSync(TOKEN_FILE)) {
      return fs.readFileSync(TOKEN_FILE, 'utf-8').trim();
    }
  } catch (err) {
    console.warn('Could not load token:', err);
  }
  return undefined;
}

function saveToken(token: string): void {
  try {
    fs.writeFileSync(TOKEN_FILE, token, 'utf-8');
  } catch (err) {
    console.warn('Could not save token:', err);
  }
}

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
