import type { UntypedSchemaDecl } from '../lib/schema';
import type { ReadonlyTable, Table } from '../lib/table';
import type { Values } from '../lib/type_util';

/**
 * A type representing a read-only database view, mapping table names to their corresponding read-only Table handles.
 */
export type ReadonlyDbView<SchemaDecl extends UntypedSchemaDecl> = {
  readonly [Tbl in Values<
    SchemaDecl['tables']
  > as Tbl['accessorName']]: ReadonlyTable<Tbl>;
} & (SchemaDecl extends {
  namespaces: infer NS extends Record<string, UntypedSchemaDecl>;
}
  ? { readonly [K in keyof NS]: ReadonlyDbView<NS[K]> }
  : {});

/**
 * A type representing the database view, mapping table names to their corresponding Table handles.
 */
export type DbView<SchemaDecl extends UntypedSchemaDecl> = {
  readonly [Tbl in Values<
    SchemaDecl['tables']
  > as Tbl['accessorName']]: Table<Tbl>;
} & (SchemaDecl extends {
  namespaces: infer NS extends Record<string, UntypedSchemaDecl>;
}
  ? { readonly [K in keyof NS]: DbView<NS[K]> }
  : {});
