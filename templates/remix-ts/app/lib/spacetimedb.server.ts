import { DbConnection, Person, tables } from '../../src/module_bindings';
import type { Infer } from 'spacetimedb';

const HOST = process.env.SPACETIMEDB_HOST ?? 'wss://maincloud.spacetimedb.com';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'remix-ts';

export type PersonData = Infer<typeof Person>;

/**
 * Fetches the initial list of people from SpacetimeDB.
 * This function is designed for use in Remix loaders.
 *
 * It establishes a WebSocket connection, subscribes to the person table,
 * waits for the initial data, and then disconnects.
 */
export async function fetchPeople(): Promise<PersonData[]> {
  return new Promise((resolve, reject) => {
    const timeoutId = setTimeout(() => {
      reject(new Error('SpacetimeDB connection timeout'));
    }, 10000);

    const connection = DbConnection.builder()
      .withUri(HOST)
      .withModuleName(DB_NAME)
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
          .onError((_ctx, error) => {
            clearTimeout(timeoutId);
            conn.disconnect();
            reject(error);
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
