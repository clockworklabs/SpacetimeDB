import { EventEmitter } from './event_emitter.ts';
import OperationsMap from './operations_map.ts';
import type { TableRuntimeTypeInfo } from './spacetime_module.ts';

import { type EventContextInterface } from './db_connection_impl.ts';

export type Operation = {
  type: 'insert' | 'delete';
  rowId: string;
  row: any;
};

export type TableUpdate = {
  tableName: string;
  operations: Operation[];
};

/**
 * Builder to generate calls to query a `table` in the database
 */
export class TableCache<RowType = any> {
  private rows: Map<string, RowType>;
  private tableTypeInfo: TableRuntimeTypeInfo;
  private emitter: EventEmitter<'insert' | 'delete' | 'update'>;

  /**
   * @param name the table name
   * @param primaryKeyCol column index designated as `#[primarykey]`
   * @param primaryKey column name designated as `#[primarykey]`
   * @param entityClass the entityClass
   */
  constructor(tableTypeInfo: TableRuntimeTypeInfo) {
    this.tableTypeInfo = tableTypeInfo;
    this.rows = new Map();
    this.emitter = new EventEmitter();
  }

  /**
   * @returns number of rows in the table
   */
  count(): number {
    return this.rows.size;
  }

  /**
   * @returns The values of the rows in the table
   */
  iter(): any[] {
    return Array.from(this.rows.values());
  }

  applyOperations = (
    operations: Operation[],
    ctx: EventContextInterface
  ): void => {
    if (this.tableTypeInfo.primaryKey !== undefined) {
      const primaryKey = this.tableTypeInfo.primaryKey;
      const inserts: Operation[] = [];
      const deleteMap = new OperationsMap<any, Operation>();
      for (const op of operations) {
        if (op.type === 'insert') {
          inserts.push(op);
        } else {
          deleteMap.set(op.row[primaryKey], op);
        }
      }
      for (const insertOp of inserts) {
        const deleteOp = deleteMap.get(insertOp.row[primaryKey]);
        if (deleteOp) {
          // the pk for updates will differ between insert/delete, so we have to
          // use the row from delete
          this.update(ctx, insertOp, deleteOp);
          deleteMap.delete(insertOp.row[primaryKey]);
        } else {
          this.insert(ctx, insertOp);
        }
      }
      for (const deleteOp of deleteMap.values()) {
        this.delete(ctx, deleteOp);
      }
    } else {
      for (const op of operations) {
        if (op.type === 'insert') {
          this.insert(ctx, op);
        } else {
          this.delete(ctx, op);
        }
      }
    }
  };

  update = (
    ctx: EventContextInterface,
    newDbOp: Operation,
    oldDbOp: Operation
  ): void => {
    const newRow = newDbOp.row;
    const oldRow = oldDbOp.row;
    this.rows.delete(oldDbOp.rowId);
    this.rows.set(newDbOp.rowId, newRow);
    this.emitter.emit('update', ctx, oldRow, newRow);
  };

  insert = (ctx: EventContextInterface, operation: Operation): void => {
    this.rows.set(operation.rowId, operation.row);
    this.emitter.emit('insert', ctx, operation.row);
  };

  delete = (ctx: EventContextInterface, dbOp: Operation): void => {
    this.rows.delete(dbOp.rowId);
    this.emitter.emit('delete', ctx, dbOp.row);
  };

  /**
   * Register a callback for when a row is newly inserted into the database.
   *
   * ```ts
   * User.onInsert((user, reducerEvent) => {
   *   if (reducerEvent) {
   *      console.log("New user on reducer", reducerEvent, user);
   *   } else {
   *      console.log("New user received during subscription update on insert", user);
   *  }
   * });
   * ```
   *
   * @param cb Callback to be called when a new row is inserted
   */
  onInsert = <EventContext>(
    cb: (ctx: EventContext, row: RowType) => void
  ): void => {
    this.emitter.on('insert', cb);
  };

  /**
   * Register a callback for when a row is deleted from the database.
   *
   * ```ts
   * User.onDelete((user, reducerEvent) => {
   *   if (reducerEvent) {
   *      console.log("Deleted user on reducer", reducerEvent, user);
   *   } else {
   *      console.log("Deleted user received during subscription update on update", user);
   *  }
   * });
   * ```
   *
   * @param cb Callback to be called when a new row is inserted
   */
  onDelete = <EventContext>(
    cb: (ctx: EventContext, row: RowType) => void
  ): void => {
    this.emitter.on('delete', cb);
  };

  /**
   * Register a callback for when a row is updated into the database.
   *
   * ```ts
   * User.onInsert((user, reducerEvent) => {
   *   if (reducerEvent) {
   *      console.log("Updated user on reducer", reducerEvent, user);
   *   } else {
   *      console.log("Updated user received during subscription update on delete", user);
   *  }
   * });
   * ```
   *
   * @param cb Callback to be called when a new row is inserted
   */
  onUpdate = <EventContext>(
    cb: (ctx: EventContext, oldRow: RowType, row: RowType) => void
  ): void => {
    this.emitter.on('update', cb);
  };

  /**
   * Remove a callback for when a row is newly inserted into the database.
   *
   * @param cb Callback to be removed
   */
  removeOnInsert = <EventContext>(
    cb: (ctx: EventContext, row: RowType) => void
  ): void => {
    this.emitter.off('insert', cb);
  };

  /**
   * Remove a callback for when a row is deleted from the database.
   *
   * @param cb Callback to be removed
   */
  removeOnDelete = <EventContext>(
    cb: (ctx: EventContext, row: RowType) => void
  ): void => {
    this.emitter.off('delete', cb);
  };

  /**
   * Remove a callback for when a row is updated into the database.
   *
   * @param cb Callback to be removed
   */
  removeOnUpdate = <EventContext>(
    cb: (ctx: EventContext, oldRow: RowType, row: RowType) => void
  ): void => {
    this.emitter.off('update', cb);
  };
}
