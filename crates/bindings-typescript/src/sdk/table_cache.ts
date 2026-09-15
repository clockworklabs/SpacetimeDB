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
} from '../lib/indexes.ts';
import type { Bound } from '../server/range.ts';
import type { Prettify } from '../lib/type_util.ts';
import { TableCacheIndex } from './table_cache_index.ts';

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
  private readonly hasPrimaryKey: boolean;
  private rows: Map<
    ComparablePrimitive,
    [RowType<TableDefForTableName<RemoteModule, TableName>>, number]
  >;
  private tableDef: TableDefForTableName<RemoteModule, TableName>;
  private emitter: EventEmitter<'insert' | 'delete' | 'update'>;
  readonly #indexes: TableCacheIndex[] = [];

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
    this.hasPrimaryKey = Object.values(this.tableDef.columns).some(
      col => col.columnMetadata.isPrimaryKey === true
    );
    // Build index views from the resolved runtime index metadata.
    //
    // We intentionally use `resolvedIndexes` rather than `indexes`:
    // - `indexes` is declarative table-level config (`IndexOpts`) used mainly for typing.
    // - `resolvedIndexes` is the runtime shape (`UntypedIndex`) that includes both
    //   field-level and explicit table-level indexes.
    // Later entries win for duplicate accessor names, matching the previous
    // assignment semantics. Only maintain dictionaries for visible accessors.
    const indexesByName = new Map(
      this.tableDef.resolvedIndexes.map(index => [index.name, index])
    );
    for (const idxDef of indexesByName.values()) {
      const index = this.#makeReadonlyIndex(this.tableDef, idxDef);
      (this as any)[idxDef.name] = index;
    }
  }

  #makeReadonlyIndex<
    I extends TableDefForTableName<
      RemoteModule,
      TableName
    >['resolvedIndexes'][number],
  >(
    tableDef: TableDefForTableName<RemoteModule, TableName>,
    idx: I
  ): ReadonlyIndex<TableDefForTableName<RemoteModule, TableName>, I> {
    type TableDef = TableDefForTableName<RemoteModule, TableName>;
    type Row = Prettify<RowType<TableDef>>;

    // We do not yet support non-btree indexes
    if (idx.algorithm !== 'btree') {
      throw new Error('Only btree indexes are supported in TableCacheImpl');
    }

    const columns = idx.columns;
    const index = new TableCacheIndex(tableDef, columns);
    this.#indexes.push(index);

    // Prefix equality is handled by the dictionary. Apply bounds only to the
    // last provided term; any remaining index columns are unconstrained.
    const matchRange = (
      value: unknown,
      { from, to }: { from: Bound<any>; to: Bound<any> }
    ): boolean => {
      if (from.tag !== 'unbounded') {
        const c = scalarCompare(value, from.value);
        if (c < 0) return false;
        if (c === 0 && from.tag === 'excluded') return false;
      }
      if (to.tag !== 'unbounded') {
        const c = scalarCompare(value, to.value);
        if (c > 0) return false;
        if (c === 0 && to.tag === 'excluded') return false;
      }
      return true;
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
          if (expected.length !== columns.length) return null;
          const rowIds = index.lookup(expected);
          if (rowIds) {
            for (const rowId of rowIds) {
              return self.rows.get(rowId)![0] as Row;
            }
          }
          return null;
        },
      };
      return impl as ReadonlyIndex<TableDef, I>;
    } else {
      const impl: ReadonlyRangedIndex<TableDef, I> = {
        *filter(range: any): IteratorObject<Row, undefined> {
          const terms = Array.isArray(range) ? range : [range];
          const last = terms[terms.length - 1];
          const isRange =
            last && typeof last === 'object' && 'from' in last && 'to' in last;
          const prefix = isRange ? terms.slice(0, -1) : terms;
          if (isRange && prefix.length === 0) {
            // A range without an equality prefix still needs a scan.
            for (const row of self.iter()) {
              const value = (row as Record<string, unknown>)[columns[0]];
              if (matchRange(value, last)) yield row;
            }
          } else {
            const rowIds = index.lookup(prefix);
            if (rowIds) {
              for (const rowId of rowIds) {
                const row = self.rows.get(rowId)![0] as Row;
                if (
                  !isRange ||
                  matchRange(
                    (row as Record<string, unknown>)[columns[prefix.length]],
                    last
                  )
                )
                  yield row;
              }
            }
          }
        },
      };
      return impl as ReadonlyIndex<TableDef, I>;
    }
  }

  #setRow(
    rowId: ComparablePrimitive,
    row: RowType<TableDefForTableName<RemoteModule, TableName>>,
    refCount: number
  ): void {
    const oldRow = this.rows.get(rowId)?.[0];
    for (const index of this.#indexes) {
      index.replace(rowId, oldRow, row);
    }
    this.rows.set(rowId, [row, refCount]);
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
  iter(): IteratorObject<
    Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>,
    undefined
  > {
    function* generator(
      rows: Map<
        ComparablePrimitive,
        [RowType<TableDefForTableName<RemoteModule, TableName>>, number]
      >
    ): IteratorObject<
      Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>,
      undefined
    > {
      for (const [row] of rows.values()) {
        yield row as Prettify<
          RowType<TableDefForTableName<RemoteModule, TableName>>
        >;
      }
    }
    return generator(this.rows);
  }

  /**
   * Allows iteration over the rows in the table
   * @returns An iterator over the rows in the table
   */
  [Symbol.iterator](): IteratorObject<
    Prettify<RowType<TableDefForTableName<RemoteModule, TableName>>>,
    undefined
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

    // Event tables: fire on_insert callbacks but don't store rows in the cache.
    if (this.tableDef.isEvent) {
      for (const op of operations) {
        if (op.type === 'insert') {
          pendingCallbacks.push({
            type: 'insert',
            table: this.tableDef.sourceName,
            cb: () => {
              this.emitter.emit('insert', ctx, op.row);
            },
          });
        }
      }
      return pendingCallbacks;
    }

    if (this.hasPrimaryKey) {
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
        `Updating a row that was not present in the cache. Table: ${this.tableDef.sourceName}, RowId: ${rowId}`
      );
      return undefined;
    }
    const [oldRow, previousCount] = existingEntry;
    const refCount = Math.max(1, previousCount + refCountDelta);
    if (previousCount + refCountDelta <= 0) {
      stdbLogger(
        'error',
        `Negative reference count for in table ${this.tableDef.sourceName} row ${rowId} (${previousCount} + ${refCountDelta})`
      );
      return undefined;
    }
    this.#setRow(rowId, newRow, refCount);
    // This indicates something is wrong, so we could arguably crash here.
    if (previousCount === 0) {
      stdbLogger(
        'error',
        `Updating a row id in table ${this.tableDef.sourceName} which was not present in the cache (rowId: ${rowId})`
      );
      return {
        type: 'insert',
        table: this.tableDef.sourceName,
        cb: () => {
          this.emitter.emit('insert', ctx, newRow);
        },
      };
    }
    return {
      type: 'update',
      table: this.tableDef.sourceName,
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
    this.#setRow(operation.rowId, operation.row, previousCount + count);
    if (previousCount === 0) {
      return {
        type: 'insert',
        table: this.tableDef.sourceName,
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
    const [oldRow, previousCount] = this.rows.get(operation.rowId) || [
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
      for (const index of this.#indexes) {
        index.remove(operation.rowId, oldRow);
      }
      this.rows.delete(operation.rowId);
      return {
        type: 'delete',
        table: this.tableDef.sourceName,
        cb: () => {
          this.emitter.emit('delete', ctx, operation.row);
        },
      };
    }
    this.#setRow(operation.rowId, operation.row, previousCount - count);
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
