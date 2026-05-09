import { DbConnection, tables } from '../module_bindings';
import type { Person } from '../module_bindings/types';

const HOST =
  import.meta.env.SPACETIMEDB_HOST ??
  import.meta.env.PUBLIC_SPACETIMEDB_HOST ??
  'ws://localhost:3000';
const DB_NAME =
  import.meta.env.SPACETIMEDB_DB_NAME ??
  import.meta.env.PUBLIC_SPACETIMEDB_DB_NAME ??
  'astro-ts';

export type PersonData = Person;

/**
 * Fetches the initial list of people from SpacetimeDB during Astro SSR.
 * The page renders this snapshot first, then the interactive client hydrates
 * into a live subscription.
 */
export async function fetchPeople(): Promise<PersonData[]> {
  return new Promise((resolve, reject) => {
    let connection: DbConnection | undefined;

    const timeoutId = setTimeout(() => {
      connection?.disconnect();
      reject(new Error('SpacetimeDB connection timeout'));
    }, 10_000);

    connection = DbConnection.builder()
      .withUri(HOST)
      .withDatabaseName(DB_NAME)
      .onConnect(conn => {
        conn
          .subscriptionBuilder()
          .onApplied(() => {
            clearTimeout(timeoutId);
            const people = Array.from(conn.db.person.iter());
            conn.disconnect();
            resolve(people);
          })
          .onError(ctx => {
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
