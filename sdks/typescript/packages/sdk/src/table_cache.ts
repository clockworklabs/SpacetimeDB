import { EventEmitter } from './event_emitter.ts';
import type { TableRuntimeTypeInfo } from './spacetime_module.ts';

import {
  BinaryWriter,
  type EventContextInterface,
} from './db_connection_impl.ts';
import { stdbLogger } from './logger.ts';
import type { ComparablePrimitive } from 'spacetimedb';

export type Operation<
  RowType extends Record<string, any> = Record<string, any>,
> = {
  type: 'insert' | 'delete';
  // For tables with a primary key, this is the primary key value, as a primitive or string.
  // Otherwise, it is an encoding of the full row.
  rowId: ComparablePrimitive;
  row: RowType;
};

export type TableUpdate<
  RowType extends Record<string, any> = Record<string, any>,
> = {
  tableName: string;
  operations: Operation<RowType>[];
};

export type PendingCallback = {
  type: 'insert' | 'delete' | 'update';
  table: string;
  cb: () => void;
};
/**
 * Builder to generate calls to query a `table` in the database
 */
export class TableCache<
  RowType extends Record<string, any> = Record<string, any>,
> {
  private rows: Map<ComparablePrimitive, [RowType, number]>;
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
  iter(): RowType[] {
    return Array.from(this.rows.values()).map(([row]) => row);
  }

  applyOperations = (
    operations: Operation<RowType>[],
    ctx: EventContextInterface
  ): PendingCallback[] => {
    const pendingCallbacks: PendingCallback[] = [];
    if (this.tableTypeInfo.primaryKeyInfo !== undefined) {
      const insertMap = new Map<
        ComparablePrimitive,
        [Operation<RowType>, number]
      >();
      const deleteMap = new Map<
        ComparablePrimitive,
        [Operation<RowType>, number]
      >();
      for (const op of operations) {
        if (op.type === 'insert') {
          const [_, prevCount] = insertMap.get(op.rowId) || [op, 0];
          insertMap.set(op.rowId, [op, prevCount + 1]);
        } else {
          const [_, prevCount] = deleteMap.get(op.rowId) || [op, 0];
          deleteMap.set(op.rowId, [op, prevCount + 1]);
        }
      }
      for (const [primaryKey, [insertOp, refCount]] of insertMap) {
        const deleteEntry = deleteMap.get(primaryKey);
        if (deleteEntry) {
          const [_, deleteCount] = deleteEntry;
          // In most cases the refCountDelta will be either 0 or refCount, but if
          // an update moves a row in or out of the result set of different queries, then
          // other deltas are possible.
          const refCountDelta = refCount - deleteCount;
          const maybeCb = this.update(
            ctx,
            primaryKey,
            insertOp.row,
            refCountDelta
          );
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
    rowId: ComparablePrimitive,
    newRow: RowType,
    refCountDelta: number = 0
  ): PendingCallback | undefined => {
    const existingEntry = this.rows.get(rowId);
    if (!existingEntry) {
      // TODO: this should throw an error and kill the connection.
      stdbLogger(
        'error',
        `Updating a row that was not present in the cache. Table: ${this.tableTypeInfo.tableName}, RowId: ${rowId}`
      );
      return undefined;
    }
    const [oldRow, previousCount] = existingEntry;
    const refCount = Math.max(1, previousCount + refCountDelta);
    if (previousCount + refCountDelta <= 0) {
      stdbLogger(
        'error',
        `Negative reference count for in table ${this.tableTypeInfo.tableName} row ${rowId} (${previousCount} + ${refCountDelta})`
      );
      return undefined;
    }
    this.rows.set(rowId, [newRow, refCount]);
    // This indicates something is wrong, so we could arguably crash here.
    if (previousCount === 0) {
      stdbLogger(
        'error',
        `Updating a row id in table ${this.tableTypeInfo.tableName} which was not present in the cache (rowId: ${rowId})`
      );
      return {
        type: 'insert',
        table: this.tableTypeInfo.tableName,
        cb: () => {
          this.emitter.emit('insert', ctx, newRow);
        },
      };
    }
    return {
      type: 'update',
      table: this.tableTypeInfo.tableName,
      cb: () => {
        this.emitter.emit('update', ctx, oldRow, newRow);
      },
    };
  };

  insert = (
    ctx: EventContextInterface,
    operation: Operation<RowType>,
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
    // It's possible to get a duplicate insert because rows can be returned from multiple queries.
    return undefined;
  };

  delete = (
    ctx: EventContextInterface,
    operation: Operation<RowType>,
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
