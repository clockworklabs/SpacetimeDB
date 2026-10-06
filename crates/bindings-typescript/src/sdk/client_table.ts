import type { ReadonlyIndexes } from '../lib/indexes';
import type { TableDeclOf, TableNamesOf } from '../lib/schema';
import type {
  ReadonlyTableMethods,
  RowType,
  TableIndexes,
  UntypedTableDecl,
} from '../lib/table';
import type { ColumnBuilder } from '../lib/type_builders';
import type { Prettify } from '../lib/type_util';
import type { EventContextInterface } from './event_context';
import type { UntypedRemoteModuleDecl } from './spacetime_module';

export type ClientTablePrimaryKeyMethods<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
  TableName extends TableNamesOf<RemoteModuleDecl>,
> = {
  /**
   * Registers a callback to be invoked when a row is updated in the table.
   * Requires that the table has a primary key defined.
   * @param cb The callback to invoke when a row is updated.
   */
  onUpdate(
    cb: (
      ctx: EventContextInterface<RemoteModuleDecl>,
      oldRow: Prettify<RowType<TableDeclOf<RemoteModuleDecl, TableName>>>,
      newRow: Prettify<RowType<TableDeclOf<RemoteModuleDecl, TableName>>>
    ) => void
  ): void;

  /**
   * Removes a previously registered update event listener.
   * @param cb The callback to remove from the update event listeners.
   */
  removeOnUpdate(
    cb: (
      ctx: EventContextInterface<RemoteModuleDecl>,
      oldRow: Prettify<RowType<TableDeclOf<RemoteModuleDecl, TableName>>>,
      newRow: Prettify<RowType<TableDeclOf<RemoteModuleDecl, TableName>>>
    ) => void
  ): void;
};

export type ClientTableInsertMethods<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
  TableName extends TableNamesOf<RemoteModuleDecl>,
> = {
  /**
   * Registers a callback to be invoked when a row is inserted into the table.
   */
  onInsert(
    cb: (
      ctx: EventContextInterface<RemoteModuleDecl>,
      row: Prettify<RowType<TableDeclOf<RemoteModuleDecl, TableName>>>
    ) => void
  ): void;

  /**
   *  Removes a previously registered insert event listener.
   * @param cb The callback to remove from the insert event listeners.
   */
  removeOnInsert(
    cb: (
      ctx: EventContextInterface<RemoteModuleDecl>,
      row: Prettify<RowType<TableDeclOf<RemoteModuleDecl, TableName>>>
    ) => void
  ): void;
};

export type ClientTableDeleteMethods<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
  TableName extends TableNamesOf<RemoteModuleDecl>,
> = {
  /**
   * Registers a callback to be invoked when a row is deleted from the table.
   */
  onDelete(
    cb: (
      ctx: EventContextInterface<RemoteModuleDecl>,
      row: Prettify<RowType<TableDeclOf<RemoteModuleDecl, TableName>>>
    ) => void
  ): void;

  /**
   * Removes a previously registered delete event listener.
   * @param cb The callback to remove from the delete event listeners.
   */
  removeOnDelete(
    cb: (
      ctx: EventContextInterface<RemoteModuleDecl>,
      row: Prettify<RowType<TableDeclOf<RemoteModuleDecl, TableName>>>
    ) => void
  ): void;
};

export type ClientTableMethods<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
  TableName extends TableNamesOf<RemoteModuleDecl>,
> = ClientTableInsertMethods<RemoteModuleDecl, TableName> &
  ClientTableDeleteMethods<RemoteModuleDecl, TableName>;

/**
 * Table<Row, UniqueConstraintViolation = never, AutoIncOverflow = never>
 *
 * - Row: row shape
 * - UCV: unique-constraint violation error type (never if none)
 * - AIO: auto-increment overflow error type (never if none)
 */
export type ClientTable<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
  TableName extends TableNamesOf<RemoteModuleDecl>,
> = Prettify<
  ClientTableCore<RemoteModuleDecl, TableName> &
    ReadonlyIndexes<
      TableDeclOf<RemoteModuleDecl, TableName>,
      TableIndexes<TableDeclOf<RemoteModuleDecl, TableName>>
    >
>;

type IsEventTable<TableDecl extends UntypedTableDecl> = TableDecl extends {
  isEvent: true;
}
  ? true
  : false;

type HasPrimaryKey<TableDecl extends UntypedTableDecl> = ColumnsHavePrimaryKey<
  TableDecl['columns']
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
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
  TableName extends TableNamesOf<RemoteModuleDecl>,
> = Partial<ClientTablePrimaryKeyMethods<RemoteModuleDecl, TableName>>;

/**
 * A variant of ClientTableCore where the primary key methods are always optional,
 * allowing for classes like TableCache to implement this interface
 */
export type ClientTableCoreImplementable<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
  TableName extends TableNamesOf<RemoteModuleDecl>,
> = ReadonlyTableMethods<TableDeclOf<RemoteModuleDecl, TableName>> &
  ClientTableMethods<RemoteModuleDecl, TableName> &
  // always present but optional -> statically known member set
  MaybePKMethods<RemoteModuleDecl, TableName>;

/**
 * Core methods of ClientTable, without the indexes mixed in.
 * Includes only statically known methods.
 *
 * Event tables only expose insert callbacks (no delete or update),
 * matching the Rust SDK's `EventTable` trait.
 */
export type ClientTableCore<
  RemoteModuleDecl extends UntypedRemoteModuleDecl,
  TableName extends TableNamesOf<RemoteModuleDecl>,
> = ReadonlyTableMethods<TableDeclOf<RemoteModuleDecl, TableName>> &
  ClientTableInsertMethods<RemoteModuleDecl, TableName> &
  (IsEventTable<TableDeclOf<RemoteModuleDecl, TableName>> extends true
    ? {}
    : ClientTableDeleteMethods<RemoteModuleDecl, TableName> &
        (HasPrimaryKey<TableDeclOf<RemoteModuleDecl, TableName>> extends true
          ? ClientTablePrimaryKeyMethods<RemoteModuleDecl, TableName>
          : {}));
