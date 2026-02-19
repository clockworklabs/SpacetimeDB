import { ConnectionId, ProductBuilder, ProductType } from '../';
import { AlgebraicType, type ComparablePrimitive } from '../';
import { BinaryReader } from '../';
import { BinaryWriter } from '../';
import {
  BsatnRowList,
  ClientMessage,
  QueryRows,
  QuerySetUpdate,
  ServerMessage,
  TableUpdateRows,
  UnsubscribeFlags,
} from './client_api/types';
import { ClientCache } from './client_cache.ts';
import { DbConnectionBuilder } from './db_connection_builder.ts';
import { INTERNAL_REMOTE_MODULE } from './internal.ts';
import { type DbContext } from './db_context.ts';
import type { Event } from './event.ts';
import {
  type ErrorContextInterface,
  type EventContextInterface,
  type ReducerEventContextInterface,
  type SubscriptionEventContextInterface,
} from './event_context.ts';
import { EventEmitter } from './event_emitter.ts';
import type { Deserializer, Identity, InferTypeOfRow, Serializer } from '../';
import type {
  ProcedureResultMessage,
  ReducerResultMessage,
} from './message_types.ts';
import type { ReducerEvent } from './reducer_event.ts';
import { type UntypedRemoteModule } from './spacetime_module.ts';
import { makeQueryBuilder } from '../lib/query';
import {
  type TableCache,
  type Operation,
  type PendingCallback,
  type TableUpdate as CacheTableUpdate,
} from './table_cache.ts';
import { WebsocketDecompressAdapter } from './websocket_decompress_adapter.ts';
import type { WebsocketTestAdapter } from './websocket_test_adapter.ts';
import {
  SubscriptionBuilderImpl,
  SubscriptionHandleImpl,
  SubscriptionManager,
  type SubscribeEvent,
} from './subscription_builder_impl.ts';
import { stdbLogger } from './logger.ts';
import { fromByteArray } from 'base64-js';
import type {
  ReducerEventInfo,
  ReducersView,
  SubscriptionEventCallback,
} from './reducers.ts';
import type { ClientDbView } from './db_view.ts';
import type { RowType, UntypedTableDef } from '../lib/table.ts';
import { toCamelCase } from '../lib/util.ts';
import type { ProceduresView } from './procedures.ts';
import type { Values } from '../lib/type_util.ts';
import type { TransactionUpdate } from './client_api/types.ts';
import { InternalError, SenderError } from '../lib/errors.ts';

export {
  DbConnectionBuilder,
  SubscriptionBuilderImpl,
  SubscriptionHandleImpl,
  type TableCache,
  type Event,
};

export type RemoteModuleOf<C> =
  C extends DbConnectionImpl<infer RM> ? RM : never;

export type {
  DbContext,
  EventContextInterface,
  ReducerEventContextInterface,
  SubscriptionEventContextInterface,
  ErrorContextInterface,
  ReducerEvent,
};

export type ConnectionEvent = 'connect' | 'disconnect' | 'connectError';

export type DbConnectionConfig<RemoteModule extends UntypedRemoteModule> = {
  uri: URL;
  nameOrAddress: string;
  identity?: Identity;
  token?: string;
  emitter: EventEmitter<ConnectionEvent>;
  createWSFn: typeof WebsocketDecompressAdapter.createWebSocketFn;
  compression: 'gzip' | 'none';
  lightMode: boolean;
  confirmedReads?: boolean;
  remoteModule: RemoteModule;
};

type ProcedureCallback = (result: ProcedureResultMessage['result']) => void;

export class DbConnectionImpl<RemoteModule extends UntypedRemoteModule>
  implements DbContext<RemoteModule>
{
  /**
   * Whether or not the connection is active.
   */
  isActive = false;

  /**
   * This connection's public identity.
   */
  identity?: Identity = undefined;

  /**
   * This connection's private authentication token.
   */
  token?: string = undefined;

  /** @internal */
  [INTERNAL_REMOTE_MODULE](): RemoteModule {
    return this.#remoteModule;
  }

  /**
   * The accessor field to access the tables in the database and associated
   * callback functions.
   */
  db: ClientDbView<RemoteModule>;

  /**
   * The accessor field to access the reducers in the database.
   */
  reducers: ReducersView<RemoteModule>;

  /**
   * The accessor field to access the procedures in the database.
   */
  procedures: ProceduresView<RemoteModule>;

  /**
   * The `ConnectionId` of the connection to to the database.
   */
  connectionId: ConnectionId = ConnectionId.random();

  // These fields are meant to be strictly private.
  #queryId = 0;
  #requestId = 0;
  #eventId = 0;
  #emitter: EventEmitter<ConnectionEvent>;
  #messageQueue = Promise.resolve();
  #outboundQueue: ClientMessage[] = [];
  #subscriptionManager = new SubscriptionManager<RemoteModule>();
  #remoteModule: RemoteModule;
  #reducerCallbacks = new Map<
    number,
    (result: ReducerResultMessage['result']) => void
  >();
  #reducerCallInfo = new Map<number, { name: string; args: object }>();
  #procedureCallbacks = new Map<number, ProcedureCallback>();
  #rowDeserializers: Record<string, Deserializer<any>>;
  #reducerArgsSerializers: Record<
    string,
    { serialize: Serializer<any>; deserialize: Deserializer<any> }
  >;
  #procedureSerializers: Record<
    string,
    { serializeArgs: Serializer<any>; deserializeReturn: Deserializer<any> }
  >;
  #sourceNameToTableDef: Record<string, Values<RemoteModule['tables']>>;

  // These fields are not part of the public API, but in a pinch you
  // could use JavaScript to access them by bypassing TypeScript's
  // private fields.
  // We use them in testing.
  private clientCache: ClientCache<RemoteModule>;
  private ws?: WebsocketDecompressAdapter | WebsocketTestAdapter;
  private wsPromise: Promise<
    WebsocketDecompressAdapter | WebsocketTestAdapter | undefined
  >;

  constructor({
    uri,
    nameOrAddress,
    identity,
    token,
    emitter,
    remoteModule,
    createWSFn,
    compression,
    lightMode,
    confirmedReads,
  }: DbConnectionConfig<RemoteModule>) {
    stdbLogger('info', 'Connecting to SpacetimeDB WS...');

    // We use .toString() here because some versions of React Native contain a bug where the URL constructor
    // incorrectly treats a URL instance as a plain string.
    // This results in an attempt to call .endsWith() on it, leading to an error.
    const url = new URL(uri.toString());
    if (!/^wss?:/.test(uri.protocol)) {
      url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
    }

    this.identity = identity;
    this.token = token;

    this.#remoteModule = remoteModule;
    this.#emitter = emitter;

    this.#rowDeserializers = Object.create(null);
    this.#sourceNameToTableDef = Object.create(null);
    for (const table of Object.values(remoteModule.tables)) {
      this.#rowDeserializers[table.sourceName] = ProductType.makeDeserializer(
        table.rowType
      );
      this.#sourceNameToTableDef[table.sourceName] = table as Values<
        RemoteModule['tables']
      >;
    }

    this.#reducerArgsSerializers = Object.create(null);
    for (const reducer of remoteModule.reducers) {
      this.#reducerArgsSerializers[reducer.name] = {
        serialize: ProductType.makeSerializer(reducer.paramsType),
        deserialize: ProductType.makeDeserializer(reducer.paramsType),
      };
    }

    this.#procedureSerializers = Object.create(null);
    for (const procedure of remoteModule.procedures) {
      this.#procedureSerializers[procedure.name] = {
        serializeArgs: ProductType.makeSerializer(
          new ProductBuilder(procedure.params).algebraicType.value
        ),
        deserializeReturn: AlgebraicType.makeDeserializer(
          procedure.returnType.algebraicType
        ),
      };
    }

    const connectionId = this.connectionId.toHexString();
    url.searchParams.set('connection_id', connectionId);

    this.clientCache = new ClientCache<RemoteModule>();
    this.db = this.#makeDbView();
    this.reducers = this.#makeReducers(remoteModule);
    this.procedures = this.#makeProcedures(remoteModule);

    this.wsPromise = createWSFn({
      url,
      nameOrAddress,
      wsProtocol: 'v2.bsatn.spacetimedb',
      authToken: token,
      compression: compression,
      lightMode: lightMode,
      confirmedReads: confirmedReads,
    })
      .then(v => {
        this.ws = v;

        this.ws.onclose = () => {
          this.#emitter.emit('disconnect', this);
          this.isActive = false;
        };
        this.ws.onerror = (e: ErrorEvent) => {
          this.#emitter.emit('connectError', this, e);
          this.isActive = false;
        };
        this.ws.onopen = this.#handleOnOpen.bind(this);
        this.ws.onmessage = this.#handleOnMessage.bind(this);
        return v;
      })
      .catch(e => {
        stdbLogger('error', 'Error connecting to SpacetimeDB WS');
        this.#emitter.emit('connectError', this, e);

        return undefined;
      });
  }

  #getNextQueryId = () => {
    const queryId = this.#queryId;
    this.#queryId += 1;
    return queryId;
  };

  #getNextRequestId = () => this.#requestId++;

  #makeDbView(): ClientDbView<RemoteModule> {
    const view = Object.create(null) as ClientDbView<RemoteModule>;

    for (const tbl of Object.values(this.#sourceNameToTableDef)) {
      // ClientDbView uses this name verbatim
      const key = tbl.accessorName;
      Object.defineProperty(view, key, {
        enumerable: true,
        configurable: false,
        get: () => this.clientCache.getOrCreateTable(tbl),
      });
    }

    return view;
  }

  #makeReducers(def: RemoteModule): ReducersView<RemoteModule> {
    const out: Record<string, unknown> = {};

    for (const reducer of def.reducers) {
      const reducerName = reducer.name;
      const key = toCamelCase(reducerName);

      const { serialize: serializeArgs } =
        this.#reducerArgsSerializers[reducerName];

      (out as any)[key] = (params: InferTypeOfRow<typeof reducer.params>) => {
        const writer = new BinaryWriter(1024);
        serializeArgs(writer, params);
        const argsBuffer = writer.getBuffer();
        return this.callReducer(reducerName, argsBuffer, params);
      };
    }

    return out as ReducersView<RemoteModule>;
  }

  #makeProcedures(def: RemoteModule): ProceduresView<RemoteModule> {
    const out: Record<string, unknown> = {};

    for (const procedure of def.procedures) {
      const procedureName = procedure.name;
      const key = toCamelCase(procedureName);

      const { serializeArgs, deserializeReturn } =
        this.#procedureSerializers[procedureName];

      (out as any)[key] = (
        params: InferTypeOfRow<typeof procedure.params>
      ): Promise<any> => {
        const writer = new BinaryWriter(1024);
        serializeArgs(writer, params);
        const argsBuffer = writer.getBuffer();
        return this.callProcedure(procedureName, argsBuffer).then(returnBuf => {
          return deserializeReturn(new BinaryReader(returnBuf));
        });
      };
    }

    return out as ProceduresView<RemoteModule>;
  }

  #makeEventContext(
    event: Event<
      ReducerEventInfo<
        RemoteModule['reducers'][number]['name'],
        InferTypeOfRow<RemoteModule['reducers'][number]['params']>
      >
    >
  ): EventContextInterface<RemoteModule> {
    // Bind methods to preserve `this` (#private fields safe)
    return {
      db: this.db,
      reducers: this.reducers,
      isActive: this.isActive,
      subscriptionBuilder: this.subscriptionBuilder.bind(this),
      disconnect: this.disconnect.bind(this),
      event,
    };
  }

  // NOTE: This is very important!!! This is the actual function that
  // gets called when you call `connection.subscriptionBuilder()`.
  // The `subscriptionBuilder` function which is generated, just shadows
  // this function in the type system, but not the actual implementation!
  // Do not remove this function, or shoot yourself in the foot please.
  // It's not clear what would be a better way to do this at this exact
  // moment.
  subscriptionBuilder = (): SubscriptionBuilderImpl<RemoteModule> => {
    return new SubscriptionBuilderImpl(this);
  };

  getTablesMap(): any {
    return makeQueryBuilder({ tables: this.#remoteModule.tables } as any);
  }

  registerSubscription(
    handle: SubscriptionHandleImpl<RemoteModule>,
    handleEmitter: EventEmitter<
      SubscribeEvent,
      SubscriptionEventCallback<RemoteModule>
    >,
    querySql: string[]
  ): number {
    const querySetId = this.#getNextQueryId();
    this.#subscriptionManager.subscriptions.set(querySetId, {
      handle,
      emitter: handleEmitter,
    });
    const requestId = this.#getNextRequestId();
    this.#sendMessage(
      ClientMessage.Subscribe({
        queryStrings: querySql,
        querySetId: { id: querySetId },
        requestId,
      })
    );
    return querySetId;
  }

  unregisterSubscription(querySetId: number): void {
    const requestId = this.#getNextRequestId();
    this.#sendMessage(
      ClientMessage.Unsubscribe({
        querySetId: { id: querySetId },
        requestId,
        flags: UnsubscribeFlags.SendDroppedRows,
      })
    );
  }

  #parseRowList(
    type: 'insert' | 'delete',
    tableName: string,
    rowList: BsatnRowList
  ): Operation[] {
    const buffer = rowList.rowsData;
    const reader = new BinaryReader(buffer);
    const rows: Operation[] = [];

    const deserializeRow = this.#rowDeserializers[tableName];
    const table = this.#sourceNameToTableDef[tableName];
    // TODO: performance
    const columnsArray = Object.entries(table.columns);
    const primaryKeyColumnEntry = columnsArray.find(
      col => col[1].columnMetadata.isPrimaryKey
    );
    let previousOffset = 0;
    while (reader.remaining > 0) {
      const row = deserializeRow(reader);
      let rowId: ComparablePrimitive | undefined = undefined;
      if (primaryKeyColumnEntry !== undefined) {
        const primaryKeyColName = primaryKeyColumnEntry[0];
        const primaryKeyColType =
          primaryKeyColumnEntry[1].typeBuilder.algebraicType;
        rowId = AlgebraicType.intoMapKey(
          primaryKeyColType,
          row[primaryKeyColName]
        );
      } else {
        // Get a view of the bytes for this row.
        const rowBytes = buffer.subarray(previousOffset, reader.offset);
        // Convert it to a base64 string, so we can use it as a map key.
        const asBase64 = fromByteArray(rowBytes);
        rowId = asBase64;
      }
      previousOffset = reader.offset;

      rows.push({
        type,
        rowId,
        row,
      });
    }
    return rows;
  }

  // Take a bunch of table updates and ensure that there is at most one update per table.
  #mergeTableUpdates(
    updates: CacheTableUpdate<UntypedTableDef>[]
  ): CacheTableUpdate<UntypedTableDef>[] {
    const merged = new Map<string, Operation[]>();
    for (const update of updates) {
      const ops = merged.get(update.tableName);
      if (ops) {
        for (const op of update.operations) ops.push(op);
      } else {
        merged.set(update.tableName, update.operations.slice());
      }
    }
    return Array.from(merged, ([tableName, operations]) => ({
      tableName,
      operations,
    }));
  }

  #queryRowsToTableUpdates(
    rows: QueryRows,
    opType: 'insert' | 'delete'
  ): CacheTableUpdate<UntypedTableDef>[] {
    const updates: CacheTableUpdate<UntypedTableDef>[] = [];
    for (const tableRows of rows.tables) {
      updates.push({
        tableName: tableRows.table,
        operations: this.#parseRowList(opType, tableRows.table, tableRows.rows),
      });
    }
    return this.#mergeTableUpdates(updates);
  }

  #tableUpdateRowsToOperations(
    tableName: string,
    rows: TableUpdateRows
  ): Operation[] {
    if (rows.tag === 'PersistentTable') {
      const inserts = this.#parseRowList(
        'insert',
        tableName,
        rows.value.inserts
      );
      const deletes = this.#parseRowList(
        'delete',
        tableName,
        rows.value.deletes
      );
      return inserts.concat(deletes);
    }
    if (rows.tag === 'EventTable') {
      // Event table rows are insert-only. The table cache handles skipping
      // storage for event tables and only firing on_insert callbacks.
      return this.#parseRowList('insert', tableName, rows.value.events);
    }
    return [];
  }

  #querySetUpdateToTableUpdates(
    querySetUpdate: QuerySetUpdate
  ): CacheTableUpdate<UntypedTableDef>[] {
    const updates: CacheTableUpdate<UntypedTableDef>[] = [];
    for (const tableUpdate of querySetUpdate.tables) {
      let operations: Operation[] = [];
      for (const rows of tableUpdate.rows) {
        operations = operations.concat(
          this.#tableUpdateRowsToOperations(tableUpdate.tableName, rows)
        );
      }
      updates.push({
        tableName: tableUpdate.tableName,
        operations,
      });
    }
    return this.#mergeTableUpdates(updates);
  }

  #sendEncoded(
    wsResolved: WebsocketDecompressAdapter | WebsocketTestAdapter,
    message: ClientMessage,
  ): void {
    const writer = new BinaryWriter(1024);
    AlgebraicType.serializeValue(writer, ClientMessage.algebraicType, message);
    const encoded = writer.getBuffer();
    wsResolved.send(encoded);
  }

  #flushOutboundQueue(
    wsResolved: WebsocketDecompressAdapter | WebsocketTestAdapter
  ): void {
    if (!this.isActive || this.#outboundQueue.length === 0) {
      return;
    }
    const pending = this.#outboundQueue.splice(0);
    for (const message of pending) {
      this.#sendEncoded(wsResolved, message);
    }
  }

  #sendMessage(message: ClientMessage): void {
    this.wsPromise.then(wsResolved => {
      if (!wsResolved || !this.isActive) {
        this.#outboundQueue.push(message);
        return;
      }
      this.#flushOutboundQueue(wsResolved);
      this.#sendEncoded(wsResolved, message);
    });
  }

  #nextEventId(): string {
    this.#eventId += 1;
    return `${this.connectionId.toHexString()}:${this.#eventId}`;
  }

  /**
   * Handles WebSocket onOpen event.
   */
  #handleOnOpen(): void {
    this.isActive = true;
    if (this.ws) {
      this.#flushOutboundQueue(this.ws);
    }
  }

  #applyTableUpdates(
    tableUpdates: CacheTableUpdate<UntypedTableDef>[],
    eventContext: EventContextInterface<RemoteModule>
  ): PendingCallback[] {
    const pendingCallbacks: PendingCallback[] = [];
    for (const tableUpdate of tableUpdates) {
      // Get table information for the table being updated
      const tableName = tableUpdate.tableName;
      const tableDef = this.#sourceNameToTableDef[tableName];
      const table = this.clientCache.getOrCreateTable(tableDef);
      const newCallbacks = table.applyOperations(
        tableUpdate.operations as Operation<
          RowType<Values<RemoteModule['tables']>>
        >[],
        eventContext
      );
      for (const callback of newCallbacks) {
        pendingCallbacks.push(callback);
      }
    }
    return pendingCallbacks;
  }

  #applyTransactionUpdates(
    eventContext: EventContextInterface<RemoteModule>,
    tu: TransactionUpdate
  ): PendingCallback[] {
    const allUpdates: CacheTableUpdate<UntypedTableDef>[] = [];
    for (const querySetUpdate of tu.querySets) {
      const tableUpdates = this.#querySetUpdateToTableUpdates(querySetUpdate);
      for (const update of tableUpdates) {
        allUpdates.push(update);
      }
      // TODO: When we have per-query storage, we will want to apply the per-query events here.
    }
    return this.#applyTableUpdates(
      this.#mergeTableUpdates(allUpdates),
      eventContext
    );
  }

  async #processMessage(data: Uint8Array): Promise<void> {
    const serverMessage = ServerMessage.deserialize(new BinaryReader(data));
    switch (serverMessage.tag) {
      case 'InitialConnection': {
        this.identity = serverMessage.value.identity;
        if (!this.token && serverMessage.value.token) {
          this.token = serverMessage.value.token;
        }
        this.connectionId = serverMessage.value.connectionId;
        this.#emitter.emit('connect', this, this.identity, this.token);
        break;
      }
      case 'SubscribeApplied': {
        const querySetId = serverMessage.value.querySetId.id;
        const subscription =
          this.#subscriptionManager.subscriptions.get(querySetId);
        if (!subscription) {
          stdbLogger(
            'error',
            `Received SubscribeApplied for unknown querySetId ${querySetId}.`
          );
          return;
        }
        const event: Event<never> = {
          id: this.#nextEventId(),
          tag: 'SubscribeApplied',
        };
        const eventContext = this.#makeEventContext(event);
        const tableUpdates = this.#queryRowsToTableUpdates(
          serverMessage.value.rows,
          'insert'
        );
        const callbacks = this.#applyTableUpdates(tableUpdates, eventContext);
        const { event: _, ...subscriptionEventContext } = eventContext;
        subscription.emitter.emit('applied', subscriptionEventContext);
        for (const callback of callbacks) {
          callback.cb();
        }
        break;
      }
      case 'UnsubscribeApplied': {
        const querySetId = serverMessage.value.querySetId.id;
        const subscription =
          this.#subscriptionManager.subscriptions.get(querySetId);
        if (!subscription) {
          stdbLogger(
            'error',
            `Received UnsubscribeApplied for unknown querySetId ${querySetId}.`
          );
          return;
        }
        const event: Event<never> = {
          id: this.#nextEventId(),
          tag: 'UnsubscribeApplied',
        };
        const eventContext = this.#makeEventContext(event);
        const tableUpdates = serverMessage.value.rows
          ? this.#queryRowsToTableUpdates(serverMessage.value.rows, 'delete')
          : [];
        const callbacks = this.#applyTableUpdates(tableUpdates, eventContext);
        const { event: _, ...subscriptionEventContext } = eventContext;
        subscription.emitter.emit('end', subscriptionEventContext);
        this.#subscriptionManager.subscriptions.delete(querySetId);
        for (const callback of callbacks) {
          callback.cb();
        }
        break;
      }
      case 'SubscriptionError': {
        const querySetId = serverMessage.value.querySetId.id;
        const error = Error(serverMessage.value.error);
        const event: Event<never> = {
          id: this.#nextEventId(),
          tag: 'Error',
          value: error,
        };
        const eventContext = this.#makeEventContext(event);
        const errorContext = {
          ...eventContext,
          event: error,
        };
        const subscription =
          this.#subscriptionManager.subscriptions.get(querySetId);
        if (subscription) {
          subscription.emitter.emit('error', errorContext, error);
          this.#subscriptionManager.subscriptions.delete(querySetId);
        } else {
          console.error(
            `Received SubscriptionError for unknown querySetId ${querySetId}:`,
            error
          );
        }
        break;
      }
      case 'TransactionUpdate': {
        const event: Event<never> = {
          id: this.#nextEventId(),
          tag: 'UnknownTransaction',
        };
        const eventContext = this.#makeEventContext(event);
        const callbacks = this.#applyTransactionUpdates(
          eventContext,
          serverMessage.value
        );
        for (const callback of callbacks) {
          callback.cb();
        }
        break;
      }
      case 'ReducerResult': {
        const { requestId, result } = serverMessage.value;

        if (result.tag === 'Ok') {
          const reducerInfo = this.#reducerCallInfo.get(requestId);
          const eventId: string = this.#nextEventId();
          const event: Event<any> = reducerInfo
            ? {
                id: eventId,
                tag: 'Reducer',
                value: {
                  timestamp: serverMessage.value.timestamp,
                  outcome: result,
                  reducer: {
                    name: reducerInfo.name,
                    args: reducerInfo.args,
                  },
                },
              }
            : {
                id: eventId,
                tag: 'UnknownTransaction',
              };
          const eventContext = this.#makeEventContext(event as any);

          const callbacks = this.#applyTransactionUpdates(
            eventContext,
            result.value.transactionUpdate
          );
          for (const callback of callbacks) {
            callback.cb();
          }
        }
        this.#reducerCallInfo.delete(requestId);
        const cb = this.#reducerCallbacks.get(requestId);
        this.#reducerCallbacks.delete(requestId);
        cb?.(result);
        break;
      }
      case 'ProcedureResult': {
        const { status, requestId } = serverMessage.value;
        const result: ProcedureResultMessage['result'] =
          status.tag === 'Returned'
            ? { tag: 'Ok', value: status.value }
            : { tag: 'Err', value: status.value };
        const cb = this.#procedureCallbacks.get(requestId);
        this.#procedureCallbacks.delete(requestId);
        cb?.(result);
        break;
      }
      case 'OneOffQueryResult': {
        console.warn(
          'Received OneOffQueryResult but SDK does not expose one-off query APIs yet.'
        );
        break;
      }
    }
  }

  /**
   * Handles WebSocket onMessage event.
   * @param wsMessage MessageEvent object.
   */
  #handleOnMessage(wsMessage: { data: Uint8Array }): void {
    // Utilize promise chaining to ensure that we process messages in order
    // even though we are processing them asyncronously. This will not begin
    // processing the next message until we await the processing of the
    // current message.
    this.#messageQueue = this.#messageQueue.then(() => {
      return this.#processMessage(wsMessage.data);
    });
  }

  /**
   * Call a reducer on your SpacetimeDB module.
   *
   * @param reducerName The name of the reducer to call
   * @param argsSerializer The arguments to pass to the reducer
   */
  callReducer(
    reducerName: string,
    argsBuffer: Uint8Array,
    reducerArgs?: object
  ): Promise<void> {
    const { promise, resolve, reject } = Promise.withResolvers<void>();
    const requestId = this.#getNextRequestId();
    const message = ClientMessage.CallReducer({
      reducer: reducerName,
      args: argsBuffer,
      requestId,
      flags: 0,
    });
    this.#sendMessage(message);
    if (reducerArgs) {
      this.#reducerCallInfo.set(requestId, {
        name: reducerName,
        args: reducerArgs,
      });
    }
    this.#reducerCallbacks.set(requestId, result => {
      if (result.tag === 'Ok' || result.tag === 'OkEmpty') {
        resolve();
      } else {
        if (result.tag === 'Err') {
          /// Interpret the user-returned error as a string.
          const reader = new BinaryReader(result.value);
          const errorString = reader.readString();
          reject(new SenderError(errorString));
        } else if (result.tag === 'InternalError') {
          reject(new InternalError(result.value));
        } else {
          const unreachable: never = result;
          reject(new Error('Unexpected reducer result'));
          void unreachable;
        }
      }
    });
    return promise;
  }

  /**
   * Call a reducer on your SpacetimeDB module with typed arguments.
   * @param reducerSchema The schema of the reducer to call
   * @param callReducerFlags The flags for the reducer call
   * @param params The arguments to pass to the reducer
   */
  callReducerWithParams(
    reducerName: string,
    // TODO: remove
    _paramsType: ProductType,
    params: object
  ): Promise<void> {
    const writer = new BinaryWriter(1024);
    this.#reducerArgsSerializers[reducerName].serialize(writer, params);
    const argsBuffer = writer.getBuffer();
    return this.callReducer(reducerName, argsBuffer, params);
  }

  /**
   * Call a reducer on your SpacetimeDB module.
   *
   * @param procedureName The name of the reducer to call
   * @param argsBuffer The arguments to pass to the reducer
   */
  callProcedure(
    procedureName: string,
    argsBuffer: Uint8Array
  ): Promise<Uint8Array> {
    const { promise, resolve, reject } = Promise.withResolvers<Uint8Array>();
    const requestId = this.#getNextRequestId();
    const message = ClientMessage.CallProcedure({
      procedure: procedureName,
      args: argsBuffer,
      requestId,
      // reserved for future use - 0 is the only valid value
      flags: 0,
    });
    this.#sendMessage(message);
    this.#procedureCallbacks.set(requestId, result => {
      if (result.tag === 'Ok') {
        resolve(result.value);
      } else {
        reject(result.value);
      }
    });
    return promise;
  }

  /**
   * Call a reducer on your SpacetimeDB module with typed arguments.
   * @param reducerSchema The schema of the reducer to call
   * @param callReducerFlags The flags for the reducer call
   * @param params The arguments to pass to the reducer
   */
  callProcedureWithParams(
    procedureName: string,
    // TODO: remove
    _paramsType: ProductType,
    params: object,
    // TODO: remove
    _returnType: AlgebraicType
  ): Promise<any> {
    const writer = new BinaryWriter(1024);
    const { serializeArgs, deserializeReturn } =
      this.#procedureSerializers[procedureName];
    serializeArgs(writer, params);
    const argsBuffer = writer.getBuffer();
    return this.callProcedure(procedureName, argsBuffer).then(returnBuf => {
      return deserializeReturn(new BinaryReader(returnBuf));
    });
  }

  /**
   * Close the current connection.
   *
   * @example
   *
   * ```ts
   * const connection = DbConnection.builder().build();
   * connection.disconnect()
   * ```
   */
  disconnect(): void {
    this.wsPromise.then(wsResolved => {
      if (wsResolved) {
        wsResolved.close();
      }
    });
  }

  private on(
    eventName: ConnectionEvent,
    callback: (ctx: DbConnectionImpl<RemoteModule>, ...args: any[]) => void
  ): void {
    this.#emitter.on(eventName, callback);
  }

  private off(
    eventName: ConnectionEvent,
    callback: (ctx: DbConnectionImpl<RemoteModule>, ...args: any[]) => void
  ): void {
    this.#emitter.off(eventName, callback);
  }

  private onConnect(
    callback: (ctx: DbConnectionImpl<RemoteModule>, ...args: any[]) => void
  ): void {
    this.#emitter.on('connect', callback);
  }

  private onDisconnect(
    callback: (ctx: DbConnectionImpl<RemoteModule>, ...args: any[]) => void
  ): void {
    this.#emitter.on('disconnect', callback);
  }

  private onConnectError(
    callback: (ctx: DbConnectionImpl<RemoteModule>, ...args: any[]) => void
  ): void {
    this.#emitter.on('connectError', callback);
  }

  removeOnConnect(
    callback: (ctx: DbConnectionImpl<RemoteModule>, ...args: any[]) => void
  ): void {
    this.#emitter.off('connect', callback);
  }

  removeOnDisconnect(
    callback: (ctx: DbConnectionImpl<RemoteModule>, ...args: any[]) => void
  ): void {
    this.#emitter.off('disconnect', callback);
  }

  removeOnConnectError(
    callback: (ctx: DbConnectionImpl<RemoteModule>, ...args: any[]) => void
  ): void {
    this.#emitter.off('connectError', callback);
  }
}
