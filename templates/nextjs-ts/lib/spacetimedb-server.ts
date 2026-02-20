import { DbConnection, tables } from '../src/module_bindings';
import { Person } from '../src/module_bindings/types';
import type { Infer } from 'spacetimedb';

const HOST = process.env.SPACETIMEDB_HOST ?? 'wss://maincloud.spacetimedb.com';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'nextjs-ts';

export type PersonData = Infer<typeof Person>;

/**
 * Fetches the initial list of people from SpacetimeDB.
 * This function is designed for use in Next.js Server Components.
 *
 * It establishes a WebSocket connection, subscribes to the person table,
 * waits for the initial data, and then disconnects.
 */
export async function fetchPeople(): Promise<PersonData[]> {
  return new Promise((resolve, reject) => {
    const timeoutId = setTimeout(() => {
      reject(new Error('SpacetimeDB connection timeout'));
    }, 10000);

    const _connection = DbConnection.builder()
      .withUri(HOST)
      .withDatabaseName(DB_NAME)
      .onConnect(conn => {
        // Subscribe to all people
        conn
          .subscriptionBuilder()
          .onApplied(() => {
            clearTimeout(timeoutId);
            // Get all people from the cache
            const people = Array.from(conn.db.person.iter());
            conn.disconnect();
            resolve(people);
          })
          .onError((ctx) => {
            clearTimeout(timeoutId);
            conn.disconnect();
            reject(ctx.event ?? new Error('Subscription error'));
          })
          .subscribe(tables.person);
      })
      .onConnectError((_ctx, error) => {
        clearTimeout(timeoutId);
        reject(error);
      })
      .build();
  });
}
