import * as fs from 'fs';
import * as path from 'path';
import * as readline from 'readline';
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

// Connection state
let conn: DbConnection | null = null;
let isReady = false;

// Setup interactive CLI
function setupCLI(): void {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  console.log('\nCommands:');
  console.log('  <name>  - Add a person with that name');
  console.log('  list    - Show all people');
  console.log('  hello   - Greet everyone (check server logs)');
  console.log('  Ctrl+C  - Quit\n');

  rl.on('line', input => {
    const text = input.trim();
    if (!text || !conn || !isReady) return;

    if (text.toLowerCase() === 'list') {
      console.log('\nPeople in database:');
      let count = 0;
      for (const person of conn.db.person.iter()) {
        console.log(`  - ${person.name}`);
        count++;
      }
      if (count === 0) {
        console.log('  (none)');
      }
      console.log();
    } else if (text.toLowerCase() === 'hello') {
      conn.reducers.sayHello({});
      console.log('Called say_hello reducer (check server logs)\n');
    } else {
      conn.reducers.add({ name: text });
    }
  });

  rl.on('close', shutdown);
}

// Connection callbacks
function onConnect(
  _conn: DbConnection,
  identity: Identity,
  token: string
): void {
  console.log('\nConnected to SpacetimeDB!');
  console.log(`Identity: ${identity.toHexString().slice(0, 16)}...`);

  // Save token for future connections
  saveToken(token);

  // Subscribe to all tables
  _conn
    .subscriptionBuilder()
    .onApplied(ctx => {
      isReady = true;

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

      setupCLI();
    })
    .onError((_ctx, err) => {
      console.error('Subscription error:', err);
    })
    .subscribeToAllTables();

  // Register callbacks for table changes
  _conn.db.person.onInsert((_ctx: EventContext, person) => {
    console.log(`[Added] ${person.name}`);
  });

  _conn.db.person.onDelete((_ctx: EventContext, person) => {
    console.log(`[Removed] ${person.name}`);
  });
}

function onDisconnect(_ctx: ErrorContext, error?: Error): void {
  isReady = false;
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

// Main entry point
async function main(): Promise<void> {
  console.log(`Connecting to SpacetimeDB...`);
  console.log(`  URI: ${HOST}`);
  console.log(`  Module: ${DB_NAME}`);

  const token = loadToken();

  // Build and establish connection
  conn = DbConnection.builder()
    .withUri(HOST)
    .withModuleName(DB_NAME)
    .withToken(token)
    .onConnect(onConnect)
    .onDisconnect(onDisconnect)
    .onConnectError(onConnectError)
    .build();
}

// Graceful shutdown
function shutdown(): void {
  console.log('\nShutting down...');
  if (conn) {
    conn.disconnect();
  }
  process.exit(0);
}

process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);

// Run the main function
main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
