import { expectTypeOf } from 'vitest';
import { createClient } from 'spacetimedb';
import { schema, table, t } from 'spacetimedb/schema';

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
const moduleSchema = {
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
  actions: root.exportGroup({ default: root.reducer({}) }),
};

const client = createClient(moduleSchema, {
  tableAliases: { people_legacy: 'people' },
});

// Consumer coverage: the inferred interface must reject invalid code as well as
// accept valid code, rather than silently widening the module to `any`.
function consumer(conn: InstanceType<typeof client.DbConnection>) {
  conn.reducers.add({ name: 'Alice' });
  conn.reducers.auth.add({ name: 'Bob' });
  conn.reducers.sayHello({});
  conn.reducers.default({});
  expectTypeOf(conn.procedures.greet({ name: 'Alice' })).toEqualTypeOf<
    Promise<string>
  >();
  conn.db.people.onInsert((_ctx, row) => {
    expectTypeOf(row.name).toEqualTypeOf<string>();
    // @ts-expect-error unknown row property
    void row.missing;
  });
  // @ts-expect-error wrong parameter type
  conn.reducers.add({ name: 123 });
  // @ts-expect-error missing parameter
  conn.reducers.add({});
  // @ts-expect-error unknown function
  conn.reducers.missing({});
  // @ts-expect-error unknown table
  void conn.db.missing;
  // @ts-expect-error unknown nested table
  void conn.db.auth.missing;
  // @ts-expect-error unknown column
  client.tables.people.where(row => row.missing.eq('x'));
}
void consumer;
function invalidDeclarations() {
  root.reducer({ onSchedule: people }, { row: people.rowType });
  root.procedure({ onSchedule: people }, { row: people.rowType }, t.unit());
  // @ts-expect-error scheduled procedures must return unit, matching the server
  root.procedure({ onSchedule: people }, { row: people.rowType }, t.string());
  // @ts-expect-error parameter schemas must be builders
  root.reducer({ name: 123 });
  // @ts-expect-error views return rows, not strings
  root.view({ public: true }, t.string());
  // @ts-expect-error multiple primary keys are not supported
  root.view(
    { public: true },
    t.array(t.row({ a: t.u32().primaryKey(), b: t.u32().primaryKey() }))
  );
}
void invalidDeclarations;
