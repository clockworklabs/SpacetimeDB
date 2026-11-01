import { schema, table, t, type DbView, type TableSchema } from './index';
import type { Table, TableOpts, UntypedTableDef } from '../lib/table';
import type { TableToSchema, UntypedSchemaDef } from '../lib/schema';

// Erasing a legacy schema's concrete table types must not remove mutations.
export function clearErasedDb(view: DbView<UntypedSchemaDef>) {
  return view.table.clear();
}

// Existing generic helpers must retain the complete Table API.
export function clearTable<Def extends UntypedTableDef>(handle: Table<Def>) {
  return handle.clear();
}

export function deleteInsertedRow<Def extends UntypedTableDef>(
  handle: Table<Def>,
  row: Parameters<Table<Def>['insert']>[0]
) {
  return handle.delete(handle.insert(row));
}

const row = { id: t.u64().primaryKey(), note: t.string().index() };
const options: TableOpts<typeof row> = { public: true };
const widenedOptions = table(options, row);
const widenedSchema: TableSchema<typeof widenedOptions.rowType.row, []> =
  widenedOptions;
const db = schema({ widenedOptions, widenedSchema });
type OrdinaryDef = TableToSchema<'widenedOptions', typeof widenedOptions>;

db.reducer(ctx => {
  for (const handle of [ctx.db.widenedOptions, ctx.db.widenedSchema]) {
    handle.insert({ id: 1n, note: 'hello' });
    handle.id.update({ id: 1n, note: 'updated' });
    handle.id.delete(1n);
    handle.note.delete('updated');
    handle.delete({ id: 1n, note: 'hello' });
    clearTable<OrdinaryDef>(handle);
    deleteInsertedRow<OrdinaryDef>(handle, { id: 1n, note: 'hello' });
    handle.clear();
  }
});
