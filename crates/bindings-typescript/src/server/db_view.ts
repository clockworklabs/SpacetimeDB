import type { MountsOf, UntypedSchemaDef } from '../lib/schema';
import type { ReadonlyTable, Table } from '../lib/table';
import type { Values } from '../lib/type_util';

/**
 * A type representing a read-only database view, mapping table names to their corresponding read-only Table handles.
 */
export type ReadonlyDbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in Values<
    SchemaDef['tables']
  > as Tbl['accessorName']]: ReadonlyTable<Tbl>;
} & {
  readonly [Alias in keyof MountsOf<SchemaDef> & string]: ReadonlyDbView<
    MountsOf<SchemaDef>[Alias]
  >;
};

/**
 * A type representing the database view, mapping table names to their corresponding Table handles.
 */
export type DbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in Values<
    SchemaDef['tables']
  > as Tbl['accessorName']]: Table<Tbl>;
} & {
  readonly [Alias in keyof MountsOf<SchemaDef> & string]: DbView<
    MountsOf<SchemaDef>[Alias]
  >;
};
