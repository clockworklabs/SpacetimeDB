import type { ReadonlyIndexes } from '../lib/indexes';
import type { TableNamesOf } from '../lib/schema';
import type {
  ReadonlyTableMethods,
  RowType,
  TableIndexes,
  UntypedTableDef,
} from '../lib/table';
import type { ColumnBuilder } from '../lib/type_builders';
import type { Prettify } from '../lib/type_util';
import type { TableDefForTableName } from './client_cache';
import type { DbContext } from './db_context.ts';
import type { EventContextInterface } from './event_context';
import type { UntypedRemoteModule } from './spacetime_module';

export type ClientTablePrimaryKeyMethods<
  RemoteModule extends UntypedRemoteModule,
  TableName extends TableNamesOf<RemoteModule>,
> = {
  /**
   * Registers a callback to be invoked when a row is updated in the table.
   * Requires that the table has a primary key defined.
   * @param cb The callback to invoke when a row is updated.
   */
  onUpdate(
    cb: (
      ctx: EventContextInterface<RemoteModule>,
      oldRow: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>,
      newRow: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
    ) => void
  ): void;

  /**
   * Removes a previously registered update event listener.
   * @param cb The callback to remove from the update event listeners.
   */
  removeOnUpdate(
    cb: (
      ctx: EventContextInterface<RemoteModule>,
      oldRow: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>,
      newRow: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
    ) => void
  ): void;
};

export type ClientTableMethods<
  RemoteModule extends UntypedRemoteModule,
  TableName extends TableNamesOf<RemoteModule>,
> = {
  remoteQuery(filters: string): Promise<IterableIterator<
    Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
  >>;

  /**
   * Registers a callback to be invoked when a row is inserted into the table.
   */
  onInsert(
    cb: (
      ctx: EventContextInterface<RemoteModule>,
      row: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
    ) => void
  ): void;

  /**
   *  Removes a previously registered insert event listener.
   * @param cb The callback to remove from the insert event listeners.
   */
  removeOnInsert(
    cb: (
      ctx: EventContextInterface<RemoteModule>,
      row: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
    ) => void
  ): void;

  /**
   * Registers a callback to be invoked when a row is deleted from the table.
   */
  onDelete(
    cb: (
      ctx: EventContextInterface<RemoteModule>,
      row: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
    ) => void
  ): void;

  /**
   * Removes a previously registered delete event listener.
   * @param cb The callback to remove from the delete event listeners.
   */
  removeOnDelete(
    cb: (
      ctx: EventContextInterface<RemoteModule>,
      row: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
    ) => void
  ): void;
};

/**
 * Table<Row, UniqueConstraintViolation = never, AutoIncOverflow = never>
 *
 * - Row: row shape
 * - UCV: unique-constraint violation error type (never if none)
 * - AIO: auto-increment overflow error type (never if none)
 */
export type ClientTable<
  RemoteModule extends UntypedRemoteModule,
  TableName extends TableNamesOf<RemoteModule>,
> = Prettify<
  ClientTableCore<RemoteModule, TableName> &
    ReadonlyIndexes<
      TableDefForTableName<RemoteModule, TableName>,
      TableIndexes<TableDefForTableName<RemoteModule, TableName>>
    >
>;

type HasPrimaryKey<TableDef extends UntypedTableDef> = ColumnsHavePrimaryKey<
  TableDef['columns']
>;

type ColumnsHavePrimaryKey<
  Cs extends Record<string, ColumnBuilder<any, any, any>>,
> = {
  [K in keyof Cs]: Cs[K] extends ColumnBuilder<any, any, infer M>
    ? M extends { isPrimaryKey: true }
      ? true
      : never
    : never;
}[keyof Cs] extends true
  ? true
  : false;

type MaybePKMethods<
  RemoteModule extends UntypedRemoteModule,
  TableName extends TableNamesOf<RemoteModule>,
> = Partial<ClientTablePrimaryKeyMethods<RemoteModule, TableName>>;

/**
 * A variant of ClientTableCore where the primary key methods are always optional,
 * allowing for classes like TableCache to implement this interface
 */
export type ClientTableCoreImplementable<
  RemoteModule extends UntypedRemoteModule,
  TableName extends TableNamesOf<RemoteModule>,
> = ReadonlyTableMethods<TableDefForTableName<RemoteModule, TableName>> &
  ClientTableMethods<RemoteModule, TableName> &
  // always present but optional -> statically known member set
  MaybePKMethods<RemoteModule, TableName>;

/**
 * Core methods of ClientTable, without the indexes mixed in.
 * Includes only staticly known methods.
 */
export type ClientTableCore<
  RemoteModule extends UntypedRemoteModule,
  TableName extends TableNamesOf<RemoteModule>,
> = ReadonlyTableMethods<TableDefForTableName<RemoteModule, TableName>> &
  ClientTableMethods<RemoteModule, TableName> &
  (HasPrimaryKey<TableDefForTableName<RemoteModule, TableName>> extends true
    ? ClientTablePrimaryKeyMethods<RemoteModule, TableName>
    : {});
