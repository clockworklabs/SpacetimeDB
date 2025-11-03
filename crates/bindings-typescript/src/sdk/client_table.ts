import type { ReadonlyIndexes } from "../lib/indexes";
import type { ReadonlyTableMethods, RowType, TableIndexes, UntypedTableDef } from "../lib/table";
import type { Prettify } from "../lib/type_util";
import type { EventContextInterface } from "./event_context";
import type { UntypedRemoteModule } from "./spacetime_module";

export type ClientTableMethods<
  RemoteModule extends UntypedRemoteModule,
  TableDef extends UntypedTableDef,
> = {
  /**
   * Registers a callback to be invoked when a row is inserted into the table.
   */
  onInsert(cb: (ctx: EventContextInterface<RemoteModule>, row: RowType<TableDef>) => void): void;

  /**
   *  Removes a previously registered insert event listener. 
   * @param cb The callback to remove from the insert event listeners.
   */
  removeOnInsert(cb: (ctx: EventContextInterface<RemoteModule>, row: RowType<TableDef>) => void): void;

  /**
   * Registers a callback to be invoked when a row is deleted from the table.
   */
  onDelete(cb: (ctx: EventContextInterface<RemoteModule>, row: RowType<TableDef>) => void): void;

  /**
   * Removes a previously registered delete event listener.
   * @param cb The callback to remove from the delete event listeners.
   */
  removeOnDelete(cb: (ctx: EventContextInterface<RemoteModule>, row: RowType<TableDef>) => void): void;
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
  TableDef extends UntypedTableDef
> = Prettify<
  ClientTableCore<RemoteModule, TableDef> &
  ReadonlyIndexes<TableDef, TableIndexes<TableDef>>
>;

/**
 * Core methods of ClientTable, without the indexes mixed in.
 * Includes only staticly known methods.
 */
export type ClientTableCore<
  RemoteModule extends UntypedRemoteModule,
  TableDef extends UntypedTableDef,
> =
  ReadonlyTableMethods<TableDef> &
  ClientTableMethods<RemoteModule, TableDef>;

/**
 * Client database view, mapping table names to their corresponding ClientTable handles.
 */
export type ClientDbView<
  RemoteModule extends UntypedRemoteModule,
> = {
  readonly [Tbl in RemoteModule['tables'][number] as Tbl['name']]: ClientTable<RemoteModule, Tbl>;
};