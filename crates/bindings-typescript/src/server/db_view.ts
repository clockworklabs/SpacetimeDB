import type { UntypedSchemaDef } from '../lib/schema';
import type { OutboxTable, ReadonlyTable, Table } from '../lib/table';
import type { Values } from '../lib/type_util';

/**
 * A type representing a read-only database view, mapping table names to their corresponding read-only Table handles.
 */
export type ReadonlyDbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in Values<
    SchemaDef['tables']
  > as Tbl['accessorName']]: ReadonlyTable<Tbl>;
} & (SchemaDef extends {
  namespaces: infer NS extends Record<string, UntypedSchemaDef>;
}
  ? { readonly [K in keyof NS]: ReadonlyDbView<NS[K]> }
  : {});

/**
 * A type representing the database view, mapping table names to their corresponding Table handles.
 * Only known outboxes use the insert-only API. Erased schema types retain the
 * existing Table API; runtime outbox handles still omit mutation methods.
 */
export type DbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in Values<
    SchemaDef['tables']
  > as Tbl['accessorName']]: boolean extends Tbl['isOutbox']
    ? Table<Tbl>
    : Tbl extends { readonly isOutbox: true }
      ? OutboxTable<Tbl>
      : Table<Tbl>;
} & (SchemaDef extends {
  namespaces: infer NS extends Record<string, UntypedSchemaDef>;
}
  ? { readonly [K in keyof NS]: DbView<NS[K]> }
  : {});
