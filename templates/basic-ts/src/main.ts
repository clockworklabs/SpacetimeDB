import { Identity } from 'spacetimedb';
import {
  DbConnection,
  ErrorContext,
  EventContext,
  tables,
} from './module_bindings/index.js';

const HOST = process.env.SPACETIMEDB_HOST ?? 'ws://localhost:3000';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'basic-ts';

DbConnection.builder()
  .withUri(HOST)
  .withDatabaseName(DB_NAME)
  .onConnect((conn: DbConnection, identity: Identity, _token: string) => {
    console.log('Connected to SpacetimeDB!');
    console.log(`Identity: ${identity.toHexString().slice(0, 16)}...`);

    conn.db.person.onInsert((_ctx: EventContext, person) => {
      console.log(`New person: ${person.name}`);
    });

    conn
      .subscriptionBuilder()
      .subscribe(tables.person);
  })
  .onDisconnect(() => {
    console.log('Disconnected from SpacetimeDB');
  })
  .onConnectError((_ctx: ErrorContext, error: Error) => {
    console.error('Connection error:', error);
    process.exit(1);
  })
  .build();
