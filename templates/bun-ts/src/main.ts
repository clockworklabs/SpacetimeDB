import { Identity } from 'spacetimedb';
import {
  DbConnection,
  ErrorContext,
  EventContext,
} from './module_bindings/index.js';

// Configuration - Bun supports .env files natively
const HOST = process.env.SPACETIMEDB_HOST ?? 'ws://localhost:3000';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'bun-ts';

// Main entry point
async function main(): Promise<void> {
  console.log(`Connecting to SpacetimeDB...`);
  console.log(`  URI: ${HOST}`);
  console.log(`  Module: ${DB_NAME}`);

  const token = await loadToken();

  // Build and establish connection
  DbConnection.builder()
    .withUri(HOST)
    .withModuleName(DB_NAME)
    .withToken(token)
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

      setupCLI(conn);
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

function setupCLI(conn: DbConnection): void {
  console.log('\nCommands:');
  console.log('  <name>  - Add a person with that name');
  console.log('  list    - Show all people');
  console.log('  hello   - Greet everyone (check server logs)');
  console.log('  Ctrl+C  - Quit\n');

  const prompt = () => process.stdout.write('> ');
  prompt();

  // Use Bun's stdin for reading input
  const decoder = new TextDecoder();
  const stdin = Bun.stdin.stream();
  const reader = stdin.getReader();

  const shutdown = (): void => {
    console.log('\nShutting down...');
    conn.disconnect();
    process.exit(0);
  };

  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);

  const readLoop = async () => {
    while (true) {
      const { done, value } = await reader.read();
      if (done) {
        shutdown();
        break;
      }

      const text = decoder.decode(value).trim();
      if (!text) {
        prompt();
        continue;
      }

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
      prompt();
    }
  };

  readLoop().catch(err => {
    console.error('CLI error:', err);
    shutdown();
  });
}

// Token persistence using Bun APIs
const TOKEN_FILE = '.spacetimedb-token';

async function loadToken(): Promise<string | undefined> {
  try {
    const file = Bun.file(TOKEN_FILE);
    if (await file.exists()) {
      const text = await file.text();
      return text.trim() || undefined;
    }
  } catch (err) {
    console.warn('Could not load token:', err);
  }
  return undefined;
}

async function saveToken(token: string): Promise<void> {
  try {
    await Bun.write(TOKEN_FILE, token);
  } catch (err) {
    console.warn('Could not save token:', err);
  }
}

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
