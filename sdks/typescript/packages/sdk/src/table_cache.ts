import { EventEmitter } from './event_emitter.ts';
import OperationsMap from './operations_map.ts';
import type { TableRuntimeTypeInfo } from './spacetime_module.ts';

import { type EventContextInterface } from './db_connection_impl.ts';
import { stdbLogger } from './logger.ts';

export type Operation = {
  type: 'insert' | 'delete';
  rowId: string;
  row: any;
};

export type TableUpdate = {
  tableName: string;
  operations: Operation[];
};

export type PendingCallback = {
  type: 'insert' | 'delete' | 'update';
  table: string;
  cb: () => void;
};
/**
 * Builder to generate calls to query a `table` in the database
 */
export class TableCache<RowType = any> {
  private rows: Map<string, [RowType, number]>;
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
    return Array.from(this.rows.values()).map(([row]) => row);
  }

  applyOperations = (
    operations: Operation[],
    ctx: EventContextInterface
  ): PendingCallback[] => {
    const pendingCallbacks: PendingCallback[] = [];
    if (this.tableTypeInfo.primaryKey !== undefined) {
      const primaryKey = this.tableTypeInfo.primaryKey;
      const insertMap = new OperationsMap<any, [Operation, number]>();
      const deleteMap = new OperationsMap<any, [Operation, number]>();
      for (const op of operations) {
        if (op.type === 'insert') {
          const [_, prevCount] = insertMap.get(op.row[primaryKey]) || [op, 0];
          insertMap.set(op.row[primaryKey], [op, prevCount + 1]);
        } else {
          const [_, prevCount] = deleteMap.get(op.row[primaryKey]) || [op, 0];
          deleteMap.set(op.row[primaryKey], [op, prevCount + 1]);
        }
      }
      for (const {
        key: primaryKey,
        value: [insertOp, refCount],
      } of insertMap) {
        const deleteEntry = deleteMap.get(primaryKey);
        if (deleteEntry) {
          const [deleteOp, deleteCount] = deleteEntry;
          // In most cases the refCountDelta will be either 0 or refCount, but if
          // an update moves a row in or out of the result set of different queries, then
          // other deltas are possible.
          const refCountDelta = refCount - deleteCount;
          const maybeCb = this.update(ctx, insertOp, deleteOp, refCountDelta);
          if (maybeCb) {
            pendingCallbacks.push(maybeCb);
          }
          deleteMap.delete(primaryKey);
        } else {
          const maybeCb = this.insert(ctx, insertOp, refCount);
          if (maybeCb) {
            pendingCallbacks.push(maybeCb);
          }
        }
      }
      for (const [deleteOp, refCount] of deleteMap.values()) {
        const maybeCb = this.delete(ctx, deleteOp, refCount);
        if (maybeCb) {
          pendingCallbacks.push(maybeCb);
        }
      }
    } else {
      for (const op of operations) {
        if (op.type === 'insert') {
          const maybeCb = this.insert(ctx, op);
          if (maybeCb) {
            pendingCallbacks.push(maybeCb);
          }
        } else {
          const maybeCb = this.delete(ctx, op);
          if (maybeCb) {
            pendingCallbacks.push(maybeCb);
          }
        }
      }
    }
    return pendingCallbacks;
  };

  update = (
    ctx: EventContextInterface,
    newDbOp: Operation,
    oldDbOp: Operation,
    refCountDelta: number = 0
  ): PendingCallback | undefined => {
    const [oldRow, previousCount] = this.rows.get(oldDbOp.rowId) || [
      oldDbOp.row,
      0,
    ];
    const refCount = Math.max(1, previousCount + refCountDelta);
    this.rows.delete(oldDbOp.rowId);
    this.rows.set(newDbOp.rowId, [newDbOp.row, refCount]);
    // This indicates something is wrong, so we could arguably crash here.
    if (previousCount === 0) {
      stdbLogger('error', 'Updating a row that was not present in the cache');
      return {
        type: 'insert',
        table: this.tableTypeInfo.tableName,
        cb: () => {
          this.emitter.emit('insert', ctx, newDbOp.row);
        },
      };
    } else if (previousCount + refCountDelta <= 0) {
      stdbLogger('error', 'Negative reference count for row');
      // TODO: We should actually error and kill the connection here.
    }
    return {
      type: 'update',
      table: this.tableTypeInfo.tableName,
      cb: () => {
        this.emitter.emit('update', ctx, oldRow, newDbOp.row);
      },
    };
  };

  insert = (
    ctx: EventContextInterface,
    operation: Operation,
    count: number = 1
  ): PendingCallback | undefined => {
    const [_, previousCount] = this.rows.get(operation.rowId) || [
      operation.row,
      0,
    ];
    this.rows.set(operation.rowId, [operation.row, previousCount + count]);
    if (previousCount === 0) {
      return {
        type: 'insert',
        table: this.tableTypeInfo.tableName,
        cb: () => {
          this.emitter.emit('insert', ctx, operation.row);
        },
      };
    }
    return undefined;
  };

  delete = (
    ctx: EventContextInterface,
    operation: Operation,
    count: number = 1
  ): PendingCallback | undefined => {
    const [_, previousCount] = this.rows.get(operation.rowId) || [
      operation.row,
      0,
    ];
    // This should never happen.
    if (previousCount === 0) {
      stdbLogger('warn', 'Deleting a row that was not present in the cache');
      return undefined;
    }
    // If this was the last reference, we are actually deleting the row.
    if (previousCount <= count) {
      // TODO: Log a warning/error if previousCount is less than count.
      this.rows.delete(operation.rowId);
      return {
        type: 'delete',
        table: this.tableTypeInfo.tableName,
        cb: () => {
          this.emitter.emit('delete', ctx, operation.row);
        },
      };
    }
    this.rows.set(operation.rowId, [operation.row, previousCount - count]);
    return undefined;
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
