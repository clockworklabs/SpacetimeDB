import { BinaryAdapter } from './algebraic_value.ts';
import BinaryReader from './binary_reader.ts';
import { EventEmitter } from './event_emitter.ts';
import OperationsMap from './operations_map.ts';
import { ReducerEvent } from './reducer_event.ts';
import { AlgebraicValue, type CallbackInit } from './spacetimedb.ts';

type DBOp = {
  type: 'insert' | 'delete';
  rowPk: string;
  row: any;
};
export class TableOperation {
  /**
   * The type of CRUD operation.
   *
   * NOTE: An update is a `delete` followed by a 'insert' internally.
   */
  type: 'insert' | 'delete';
  rowPk: string;
  row: Uint8Array;

  constructor(type: 'insert' | 'delete', rowPk: string, row: Uint8Array | any) {
    this.type = type;
    this.rowPk = rowPk;
    this.row = row;
  }
}

export class TableUpdate {
  tableName: string;
  operations: TableOperation[];

  constructor(tableName: string, operations: TableOperation[]) {
    this.tableName = tableName;
    this.operations = operations;
  }
}

/**
 * Builder to generate calls to query a `table` in the database
 */
export class Table {
  // TODO: most of this stuff should be probably private
  name: string;
  rows: Map<string, any>;
  emitter: EventEmitter;
  #entityClass: any;
  pkCol?: number;

  /**
   * @param name the table name
   * @param pkCol column designated as `#[primarykey]`
   * @param entityClass the entityClass
   */
  constructor(name: string, pkCol: number | undefined, entityClass: any) {
    this.name = name;
    this.rows = new Map();
    this.emitter = new EventEmitter();
    this.pkCol = pkCol;
    this.#entityClass = entityClass;
  }

  /**
   * @returns number of entries in the table
   */
  count(): number {
    return this.rows.size;
  }

  /**
   * @returns The values of the entries in the table
   */
  getInstances(): any[] {
    return Array.from(this.rows.values());
  }

  applyOperations = (
    operations: TableOperation[],
    reducerEvent: ReducerEvent | undefined
  ): void => {
    let dbOps: DBOp[] = [];
    for (let operation of operations) {
      const pk: string = operation.rowPk;
      const adapter = new BinaryAdapter(new BinaryReader(operation.row));
      const entry = AlgebraicValue.deserialize(
        this.#entityClass.getAlgebraicType(),
        adapter
      );
      const row = this.#entityClass.fromValue(entry);

      dbOps.push({
        type: operation.type,
        rowPk: pk,
        row: row,
      });
    }

    if (this.#entityClass.primaryKey !== undefined) {
      const pkName = this.#entityClass.primaryKey;
      const inserts: any[] = [];
      const deleteMap = new OperationsMap<any, DBOp>();
      for (const dbOp of dbOps) {
        if (dbOp.type === 'insert') {
          inserts.push(dbOp);
        } else {
          deleteMap.set(dbOp.row[pkName], dbOp);
        }
      }
      for (const dbOp of inserts) {
        const deleteOp = deleteMap.get(dbOp.instance[pkName]);
        if (deleteOp) {
          // the pk for updates will differ between insert/delete, so we have to
          // use the instance from delete
          this.update(dbOp, deleteOp, reducerEvent);
          deleteMap.delete(dbOp.instance[pkName]);
        } else {
          this.insert(dbOp, reducerEvent);
        }
      }
      for (const dbOp of deleteMap.values()) {
        this.delete(dbOp, reducerEvent);
      }
    } else {
      for (const dbOp of dbOps) {
        if (dbOp.type === 'insert') {
          this.insert(dbOp, reducerEvent);
        } else {
          this.delete(dbOp, reducerEvent);
        }
      }
    }
  };

  update = (
    newDbOp: DBOp,
    oldDbOp: DBOp,
    reducerEvent: ReducerEvent | undefined
  ): void => {
    const newInstance = newDbOp.row;
    const oldInstance = oldDbOp.row;
    this.rows.delete(oldDbOp.rowPk);
    this.rows.set(newDbOp.rowPk, newInstance);
    this.emitter.emit('update', oldInstance, newInstance, reducerEvent);
  };

  insert = (dbOp: DBOp, reducerEvent: ReducerEvent | undefined): void => {
    this.rows.set(dbOp.rowPk, dbOp.row);
    this.emitter.emit('insert', dbOp.row, reducerEvent);
  };

  delete = (dbOp: DBOp, reducerEvent: ReducerEvent | undefined): void => {
    this.rows.delete(dbOp.rowPk);
    this.emitter.emit('delete', dbOp.row, reducerEvent);
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
    cb: (
      ctx: EventContext,
      value: any,
      reducerEvent: ReducerEvent | undefined
    ) => void,
    init?: CallbackInit
  ): void => {
    this.emitter.on('insert', cb);

    if (init?.signal) {
      init.signal.addEventListener('abort', () => {
        this.emitter.off('insert', cb);
      });
    }
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
    cb: (
      ctx: EventContext,
      value: any,
      reducerEvent: ReducerEvent | undefined
    ) => void,
    init?: CallbackInit
  ): void => {
    this.emitter.on('delete', cb);

    if (init?.signal) {
      init.signal.addEventListener('abort', () => {
        this.emitter.off('delete', cb);
      });
    }
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
    cb: (
      ctx: EventContext,
      value: any,
      oldValue: any,
      reducerEvent: ReducerEvent | undefined
    ) => void,
    init?: CallbackInit
  ): void => {
    this.emitter.on('update', cb);

    if (init?.signal) {
      init.signal.addEventListener('abort', () => {
        this.emitter.off('update', cb);
      });
    }
  };
}
