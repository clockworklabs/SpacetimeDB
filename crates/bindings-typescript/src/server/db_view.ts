import type { UntypedSchemaDef } from '../lib/schema';
import type { ReadonlyTable, Table } from '../lib/table';

/**
 * A type representing a read-only database view, mapping table names to their corresponding read-only Table handles.
 */
export type ReadonlyDbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in SchemaDef['tables'][number] as Tbl['accessorName']]: ReadonlyTable<Tbl>;
};

/**
 * A type representing the database view, mapping table names to their corresponding Table handles.
 */
export type DbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in SchemaDef['tables'][number] as Tbl['accessorName']]: Table<Tbl>;
};
