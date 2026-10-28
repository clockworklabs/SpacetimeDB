import { describe, expect, expectTypeOf, test } from 'vitest';
import { schema, table, t } from '../src/schema/declarations';
import { createClient } from '../src/sdk/create_client';
import { INTERNAL_REMOTE_MODULE } from '../src/sdk/internal';
import WebsocketTestAdapter from '../src/sdk/websocket_test_adapter';

const Person = t.row({
  id: t.u32().primaryKey().autoInc(),
  name: t.string().name('full_name'),
  age: t.u8().default(18),
});
const people = table(
  {
    name: 'people',
    public: true,
    indexes: [{ accessor: 'byAge', algorithm: 'btree', columns: ['age'] }],
  },
  Person
);
const childSchema = schema({ people });
const child = {
  default: childSchema,
  add: childSchema.reducer({ name: 'insert_person' }, { name: t.string() }),
};
const root = schema({ people, auth: { name: 'auth_wire', module: child } });
const declarations = {
  default: root,
  add: root.reducer({ name: 'add_person' }, { name: t.string() }),
  sayHello: root.reducer({}),
  greet: root.procedure({ name: t.string() }, t.string()),
  names: root.anonymousView(
    { public: true },
    t.array(t.row({ name: t.string() }))
  ),
  named: root.view({ public: true }, { name: t.string() }, t.array(Person)),
  me: root.view({ public: true }, t.option(Person)),
  Person,
};

const client = createClient(declarations, {
  tableAliases: { people_legacy: 'people' },
});

// Consumer coverage: the inferred interface must reject invalid code as well as
// accept valid code, rather than silently widening the module to `any`.
function consumer(conn: InstanceType<typeof client.DbConnection>) {
  conn.reducers.add({ name: 'Alice' });
  conn.reducers.auth.add({ name: 'Bob' });
  conn.reducers.sayHello({});
  expectTypeOf(conn.procedures.greet({ name: 'Alice' })).toEqualTypeOf<
    Promise<string>
  >();
  conn.db.people.onInsert((_ctx, row) => {
    expectTypeOf(row.name).toEqualTypeOf<string>();
    // @ts-expect-error unknown row property
    row.missing;
  });
  // @ts-expect-error wrong parameter type
  conn.reducers.add({ name: 123 });
  // @ts-expect-error missing parameter
  conn.reducers.add({});
  // @ts-expect-error unknown function
  conn.reducers.missing({});
  // @ts-expect-error unknown table
  conn.db.missing;
  // @ts-expect-error unknown nested table
  conn.db.auth.missing;
  // @ts-expect-error unknown column
  client.tables.people.where(row => row.missing.eq('x'));
}
void consumer;

describe('module declarations', () => {
  test('retains table and function metadata and produces the typed client API', async () => {
    expect(declarations.add.params.name.algebraicType.tag).toBe('String');
    expect(declarations.greet.returnType.algebraicType.tag).toBe('String');
    expect(people.rowType.row.id.columnMetadata.isAutoIncrement).toBe(true);
    expect(people.rowType.row.age.columnMetadata.defaultValue).toBe(18);
    expect(client.tables.people_legacy).toBe(client.tables.people);
    expect(client.tables.people.toSql()).toBe('SELECT * FROM "people"');
    expect(client.tables.auth.people.toSql()).toBe(
      'SELECT * FROM "auth_wire"."people"'
    );
    expect(client.reducers.add.name).toBe('add_person');
    expect(client.reducers.auth.add.name).toBe('auth_wire.insert_person');

    const ws = new WebsocketTestAdapter();
    const conn = client.DbConnection.builder()
      .withUri('ws://localhost:3000')
      .withDatabaseName('example')
      .withWSFn(ws.openWebSocket)
      .build();
    expect(conn).toBeInstanceOf(client.DbConnection);
    expect(conn.db.people_legacy).toBe(conn.db.people);
    const module = conn[INTERNAL_REMOTE_MODULE]();
    expect(module.viewsWithArgs).toEqual(['named']);
    expect(Object.keys(module.tables)).toEqual([
      'people',
      'auth.people',
      'names',
      'named',
      'me',
    ]);
    expect(module.tables.people.sourceName).toBe('people');
    expect(module.tables.people.columns.name.columnMetadata.name).toBe(
      'full_name'
    );
    await conn['wsPromise'];
    conn.disconnect();
  });

  test('rejects declarations exported from a different schema', () => {
    expect(() => {
      createClient({ default: schema({}), add: declarations.add });
    }).toThrow('another schema');
  });
});
