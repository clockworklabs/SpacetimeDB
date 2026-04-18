import type { UntypedSchemaDef } from '../lib/schema';
import type { ReadonlyTable, Table } from '../lib/table';
import type { Values } from '../lib/type_util';

/**
 * Removes event tables from a schema type.
 */
export type NonEventSchema<SchemaDef extends UntypedSchemaDef> = {
  tables: {
    readonly [AccName in keyof SchemaDef['tables'] as SchemaDef['tables'][AccName]['isEvent'] extends true
      ? never
      : AccName]: SchemaDef['tables'][AccName];
  };
};

/**
 * A type representing a read-only database view, mapping table names to their corresponding read-only Table handles.
 */
export type ReadonlyDbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in Values<
    NonEventSchema<SchemaDef>['tables']
  > as Tbl['accessorName']]: ReadonlyTable<Tbl>;
};

/**
 * A type representing the database view, mapping table names to their corresponding Table handles.
 */
export type DbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in Values<
    SchemaDef['tables']
  > as Tbl['accessorName']]: Table<Tbl>;
};
