import { EventEmitter } from './event_emitter.ts';

import { stdbLogger } from './logger.ts';
import { deepEqual, type ComparablePrimitive } from '../';
import type { EventContextInterface, TableDefForTableName } from './index.ts';
import type { RowType, TableIndexes, UntypedTableDef } from '../lib/table.ts';
import type { ClientTableCoreImplementable } from './client_table.ts';
import type { UntypedRemoteModule } from './spacetime_module.ts';
import type { TableNamesOf } from '../lib/schema.ts';
import type {
  ReadonlyIndex,
  ReadonlyIndexes,
  ReadonlyRangedIndex,
  ReadonlyUniqueIndex,
  UntypedIndex,
} from '../lib/indexes.ts';
import type { Bound } from '../server/range.ts';
import type { Prettify } from '../lib/type_util.ts';

export type Operation<
  RowType extends Record<string, any> = Record<string, any>,
> = {
  type: 'insert' | 'delete';
  // For tables with a primary key, this is the primary key value, as a primitive or string.
  // Otherwise, it is an encoding of the full row.
  rowId: ComparablePrimitive;
  row: RowType;
};

export type TableUpdate<TableDef extends UntypedTableDef> = {
  tableName: string;
  operations: Operation<RowType<TableDef>>[];
};

export type PendingCallback = {
  type: 'insert' | 'delete' | 'update';
  table: string;
  cb: () => void;
};

// Strict scalar compare for index term values.
const scalarCompare = (x: any, y: any): number => {
  if (x === y) return 0;
  // Compare booleans/numbers/bigints/strings with JS ordering.
  return x < y ? -1 : 1;
};

export type TableIndexView<
  RemoteModule extends UntypedRemoteModule,
  TableName extends TableNamesOf<RemoteModule>,
> = ReadonlyIndexes<
  TableDefForTableName<RemoteModule, TableName>,
  TableIndexes<TableDefForTableName<RemoteModule, TableName>>
>;

export type TableCache<
  RemoteModule extends UntypedRemoteModule,
  TableName extends TableNamesOf<RemoteModule>,
> = TableCacheImpl<RemoteModule, TableName> &
  TableIndexView<RemoteModule, TableName>;

/**
 * Builder to generate calls to query a `table` in the database
 */
export class TableCacheImpl<
  RemoteModule extends UntypedRemoteModule,
  TableName extends TableNamesOf<RemoteModule>,
> implements ClientTableCoreImplementable<RemoteModule, TableName>
{
  private rows: Map<
    ComparablePrimitive,
    [RowType<TableDefForTableName<RemoteModule, TableName>>, number]
  >;
  private tableDef: TableDefForTableName<RemoteModule, TableName>;
  private emitter: EventEmitter<'insert' | 'delete' | 'update'>;

  /**
   * @param name the table name
   * @param primaryKeyCol column index designated as `#[primarykey]`
   * @param primaryKey column name designated as `#[primarykey]`
   * @param entityClass the entityClass
   */
  constructor(tableDef: TableDefForTableName<RemoteModule, TableName>) {
    this.tableDef = tableDef;
    this.rows = new Map();
    this.emitter = new EventEmitter();
    // Build indexes
    const indexesDef = this.tableDef.indexes || {};
    for (const idx of indexesDef) {
      const idxDef = idx as UntypedIndex<
        keyof TableDefForTableName<RemoteModule, TableName>['columns'] & string
      >;
      const index = this.#makeReadonlyIndex(this.tableDef, idxDef);
      (this as any)[idx.name!] = index;
    }
  }

  // TODO: this just scans the whole table; we should build proper index structures
  #makeReadonlyIndex<
    I extends UntypedIndex<
      keyof TableDefForTableName<RemoteModule, TableName>['columns'] & string
    >,
  >(
    tableDef: TableDefForTableName<RemoteModule, TableName>,
    idx: I
  ): ReadonlyIndex<TableDefForTableName<RemoteModule, TableName>, I> {
    type TableDef = TableDefForTableName<RemoteModule, TableName>;
    type Row = RowType<TableDef>;

    // We do not yet support non-btree indexes
    if (idx.algorithm !== 'btree') {
      throw new Error('Only btree indexes are supported in TableCacheImpl');
    }

    const columns = idx.columns;

    // Extract the tuple key for this btree index (column order preserved)
    const getKey = (row: Row): readonly unknown[] => columns.map(c => row[c]);

    // The server’s ranged scan fixes all prefix cols to equality and applies
    // the bound only to the *last* term. We mirror that.
    //
    // rangeArg for multi-col index is:
    //   [...prefixEqualValues, (lastTerm | Range<lastTerm>)]
    //
    // If only one element is provided, it’s the last term (scalar or Range).
    const matchRange = (row: Row, rangeArg: any): boolean => {
      const key = getKey(row);

      // Normalize rangeArg into an array.
      // With multi-col b-tree, IndexScanRangeBounds always yields at least one element.
      const arr = Array.isArray(rangeArg) ? rangeArg : [rangeArg];

      const prefixLen = Math.max(0, arr.length - 1);
      // Check equality over the prefix (all but the last provided element)
      for (let i = 0; i < prefixLen; i++) {
        if (!deepEqual(key[i], arr[i])) return false;
      }

      const lastProvided = arr[arr.length - 1];
      const kLast = key[prefixLen];

      // If the last provided is a Range<T>, apply bounds; otherwise equality.
      if (
        lastProvided &&
        typeof lastProvided === 'object' &&
        'from' in lastProvided &&
        'to' in lastProvided
      ) {
        // Range<T>
        const from = lastProvided.from as Bound<any>;
        const to = lastProvided.to as Bound<any>;

        // Lower bound
        if (from.tag !== 'unbounded') {
          const c = scalarCompare(kLast, from.value);
          if (c < 0) return false;
          if (c === 0 && from.tag === 'excluded') return false;
        }

        // Upper bound
        if (to.tag !== 'unbounded') {
          const c = scalarCompare(kLast, to.value);
          if (c > 0) return false;
          if (c === 0 && to.tag === 'excluded') return false;
        }

        // All good on last term; any remaining columns (if any) are unconstrained,
        // which matches server behavior for a prefix scan.
        return true;
      } else {
        // Equality on the last provided element
        if (!deepEqual(kLast, lastProvided)) return false;
        // Any remaining columns are unconstrained (prefix equality only).
        return true;
      }
    };

    // An index is unique if it shares all columns with a unique constraint
    const isUnique = tableDef.constraints.some(constraint => {
      if (constraint.constraint !== 'unique') {
        return false;
      }
      return deepEqual(constraint.columns, idx.columns);
    });

    // eslint-disable-next-line @typescript-eslint/no-this-alias
    const self = this;
    if (isUnique) {
      const impl: ReadonlyUniqueIndex<TableDef, I> = {
        find: (colVal: any): Row | null => {
          // For unique btree, caller supplies the *full* key (tuple if multi-col).
          const expected = Array.isArray(colVal) ? colVal : [colVal];
          for (const row of self.iter()) {
            if (deepEqual(getKey(row), expected)) return row;
          }
          return null;
        },
      };
      return impl as ReadonlyIndex<TableDef, I>;
    } else {
      const impl: ReadonlyRangedIndex<TableDef, I> = {
        *filter(range: any): IterableIterator<Row> {
          for (const row of self.iter()) {
            if (matchRange(row, range)) yield row;
          }
        },
      };
      return impl as ReadonlyIndex<TableDef, I>;
    }
  }

  /**
   * @returns number of rows in the table
   */
  count(): bigint {
    return BigInt(this.rows.size);
  }

  /**
   * @returns The values of the rows in the table
   */
  iter(): IterableIterator<
    RowType<TableDefForTableName<RemoteModule, TableName>>
  > {
    function* generator(
      rows: Map<
        ComparablePrimitive,
        [RowType<TableDefForTableName<RemoteModule, TableName>>, number]
      >
    ): IterableIterator<
      RowType<TableDefForTableName<RemoteModule, TableName>>
    > {
      for (const [row] of rows.values()) {
        yield row;
      }
    }
    return generator(this.rows);
  }

  /**
   * Allows iteration over the rows in the table
   * @returns An iterator over the rows in the table
   */
  [Symbol.iterator](): IterableIterator<
    RowType<TableDefForTableName<RemoteModule, TableName>>
  > {
    return this.iter();
  }

  applyOperations = (
    operations: Operation<
      RowType<TableDefForTableName<RemoteModule, TableName>>
    >[],
    ctx: EventContextInterface<RemoteModule>
  ): PendingCallback[] => {
    const pendingCallbacks: PendingCallback[] = [];
    // TODO: performance
    const hasPrimaryKey = Object.values(this.tableDef.columns).some(
      col => col.columnMetadata.isPrimaryKey === true
    );
    if (hasPrimaryKey) {
      const insertMap = new Map<
        ComparablePrimitive,
        [
          Operation<RowType<TableDefForTableName<RemoteModule, TableName>>>,
          number,
        ]
      >();
      const deleteMap = new Map<
        ComparablePrimitive,
        [
          Operation<RowType<TableDefForTableName<RemoteModule, TableName>>>,
          number,
        ]
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
    ctx: EventContextInterface<RemoteModule>,
    rowId: ComparablePrimitive,
    newRow: RowType<TableDefForTableName<RemoteModule, TableName>>,
    refCountDelta: number = 0
  ): PendingCallback | undefined => {
    const existingEntry = this.rows.get(rowId);
    if (!existingEntry) {
      // TODO: this should throw an error and kill the connection.
      stdbLogger(
        'error',
        `Updating a row that was not present in the cache. Table: ${this.tableDef.name}, RowId: ${rowId}`
      );
      return undefined;
    }
    const [oldRow, previousCount] = existingEntry;
    const refCount = Math.max(1, previousCount + refCountDelta);
    if (previousCount + refCountDelta <= 0) {
      stdbLogger(
        'error',
        `Negative reference count for in table ${this.tableDef.name} row ${rowId} (${previousCount} + ${refCountDelta})`
      );
      return undefined;
    }
    this.rows.set(rowId, [newRow, refCount]);
    // This indicates something is wrong, so we could arguably crash here.
    if (previousCount === 0) {
      stdbLogger(
        'error',
        `Updating a row id in table ${this.tableDef.name} which was not present in the cache (rowId: ${rowId})`
      );
      return {
        type: 'insert',
        table: this.tableDef.name,
        cb: () => {
          this.emitter.emit('insert', ctx, newRow);
        },
      };
    }
    return {
      type: 'update',
      table: this.tableDef.name,
      cb: () => {
        this.emitter.emit('update', ctx, oldRow, newRow);
      },
    };
  };

  insert = (
    ctx: EventContextInterface<RemoteModule>,
    operation: Operation<
      RowType<TableDefForTableName<RemoteModule, TableName>>
    >,
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
        table: this.tableDef.name,
        cb: () => {
          this.emitter.emit('insert', ctx, operation.row);
        },
      };
    }
    // It's possible to get a duplicate insert because rows can be returned from multiple queries.
    return undefined;
  };

  delete = (
    ctx: EventContextInterface<RemoteModule>,
    operation: Operation<
      RowType<TableDefForTableName<RemoteModule, TableName>>
    >,
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
        table: this.tableDef.name,
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
   * ctx.db.user.onInsert((reducerEvent, user) => {
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
  onInsert = (
    cb: (
      ctx: EventContextInterface<RemoteModule>,
      row: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
    ) => void
  ): void => {
    this.emitter.on('insert', cb);
  };

  /**
   * Register a callback for when a row is deleted from the database.
   *
   * ```ts
   * ctx.db.user.onDelete((reducerEvent, user) => {
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
  onDelete = (
    cb: (
      ctx: EventContextInterface<RemoteModule>,
      row: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
    ) => void
  ): void => {
    this.emitter.on('delete', cb);
  };

  /**
   * Register a callback for when a row is updated into the database.
   *
   * ```ts
   * ctx.db.user.onInsert((reducerEvent, oldUser, user) => {
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
  onUpdate = (
    cb: (
      ctx: EventContextInterface<RemoteModule>,
      oldRow: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>,
      row: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
    ) => void
  ): void => {
    this.emitter.on('update', cb);
  };

  /**
   * Remove a callback for when a row is newly inserted into the database.
   *
   * @param cb Callback to be removed
   */
  removeOnInsert = (
    cb: (
      ctx: EventContextInterface<RemoteModule>,
      row: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
    ) => void
  ): void => {
    this.emitter.off('insert', cb);
  };

  /**
   * Remove a callback for when a row is deleted from the database.
   *
   * @param cb Callback to be removed
   */
  removeOnDelete = (
    cb: (
      ctx: EventContextInterface<RemoteModule>,
      row: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
    ) => void
  ): void => {
    this.emitter.off('delete', cb);
  };

  /**
   * Remove a callback for when a row is updated into the database.
   *
   * @param cb Callback to be removed
   */
  removeOnUpdate = (
    cb: (
      ctx: EventContextInterface<RemoteModule>,
      oldRow: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>,
      row: Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>
    ) => void
  ): void => {
    this.emitter.off('update', cb);
  };
}
