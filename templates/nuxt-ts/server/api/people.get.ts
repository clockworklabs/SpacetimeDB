import { DbConnection, tables, type PersonRow } from '../../module_bindings';
import type { Infer } from 'spacetimedb';

const HOST = process.env.SPACETIMEDB_HOST ?? 'ws://localhost:3000';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'nuxt-ts';

type PersonData = Infer<typeof PersonRow>;

export default defineEventHandler(async (): Promise<PersonData[]> => {
  return new Promise((resolve, reject) => {
    const timeoutId = setTimeout(() => {
      reject(new Error('SpacetimeDB connection timeout'));
    }, 10000);

    DbConnection.builder()
      .withUri(HOST)
      .withModuleName(DB_NAME)
      .onConnect(conn => {
        conn
          .subscriptionBuilder()
          .onApplied(() => {
            clearTimeout(timeoutId);
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
});
