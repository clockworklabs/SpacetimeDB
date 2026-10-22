import type { DbConnectionImpl } from './db_connection_impl';
import { procedures } from './procedures';
import { reducers } from './reducers';
import { schema, withViewParams } from './schema';
import type { RemoteModule } from './spacetime_module';
import { table } from '../lib/table';
import t from '../lib/type_builders';

// A module declared the way generated client bindings declare one.
const person = table(
  { name: 'person' },
  {
    id: t.u32().primaryKey(),
    name: t.string(),
  }
);

const tablesSchema = schema({
  person,
  peopleBy: withViewParams(table({ name: 'people_by' }, person.rowType), {
    minId: t.u32(),
  }),
  'lib.lineItems': table({ name: 'lib.line_items' }, { id: t.u32() }),
  'lib.peopleBy': withViewParams(
    table({ name: 'lib.people_by' }, person.rowType),
    { minId: t.u32() }
  ),
});
const reducersSchema = reducers();
const proceduresSchema = procedures();

// eslint-disable-next-line @typescript-eslint/no-unused-vars
const REMOTE_MODULE = {
  versionInfo: {
    cliVersion: '2.8.3' as const,
  },
  tables: tablesSchema.schemaType.tables,
  reducers: reducersSchema.reducersType.reducers,
  ...proceduresSchema,
} satisfies RemoteModule<
  typeof tablesSchema.schemaType,
  typeof reducersSchema.reducersType,
  typeof proceduresSchema
>;

declare const conn: DbConnectionImpl<typeof REMOTE_MODULE>;
const sub = conn.subscriptionBuilder();

// The `subscribe` callback's tables are typed like the generated `tables` export.
sub.subscribe(tables => {
  const _byName = tables.person.where(row => row.name.eq('alice')).build();
  // @ts-expect-error `id` is a number.
  const _wrongType = tables.person.where(row => row.id.eq('1')).build();
  // @ts-expect-error `age` is not a column of `person`.
  const _unknownColumn = tables.person.where(row => row.age.eq(1)).build();
  // @ts-expect-error `nobody` is not a table.
  const _unknownTable = tables.nobody;
  return [tables.person.build(), tables.person.where(row => row.id.eq(1))];
});

// A view with args is called with them, also in the callback.
sub.subscribe(tables => tables.peopleBy({ minId: 2 }));
// @ts-expect-error `minId` is a number.
sub.subscribe(tables => tables.peopleBy({ minId: 'two' }));
// @ts-expect-error `minId` is required.
sub.subscribe(tables => tables.peopleBy({}));
// @ts-expect-error a view with args can't be queried without them.
sub.subscribe(tables => tables.peopleBy.build());

// A submodule's table is nested by its accessor name, as in `conn.db`.
sub.subscribe(tables => tables.lib.lineItems.where(row => row.id.eq(1)));
// @ts-expect-error `name` is not a column of `lib.lineItems`.
sub.subscribe(tables => tables.lib.lineItems.where(row => row.name.eq('x')));
// @ts-expect-error it isn't keyed by its source name.
sub.subscribe(tables => tables.lib.line_items.build());
// @ts-expect-error nor by its dotted accessor name at the root.
sub.subscribe(tables => tables['lib.lineItems'].build());

// So is a submodule's view with args, which is called with them.
sub.subscribe(tables => tables.lib.peopleBy({ minId: 2 }));
// @ts-expect-error `minId` is a number.
sub.subscribe(tables => tables.lib.peopleBy({ minId: 'two' }));
// @ts-expect-error it isn't keyed by its source name.
sub.subscribe(tables => tables.lib.people_by({ minId: 2 }));
