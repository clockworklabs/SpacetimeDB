import { schema, table, t } from 'spacetimedb/schema';
import { createClient } from 'spacetimedb';

const s = schema({
  person: table(
    { public: true },
    {
      id: t.u32().primaryKey(),
      name: t.string(),
    }
  ),
});
const { DbConnection, tables, reducers } = createClient({
  default: s,
  add: s.reducer({ name: t.string() }),
});
export type Connection = InstanceType<typeof DbConnection>;
export const builder = DbConnection.builder();
export const query = tables.person.where(p => p.name.eq('Alice'));
export const add = reducers.add;

function consumer(conn: Connection) {
  conn.reducers.add({ name: 'Alice' });
  // @ts-expect-error wrong argument type
  conn.reducers.add({ name: 123 });
  // @ts-expect-error unknown member
  conn.db.unknown;
}
void consumer;
