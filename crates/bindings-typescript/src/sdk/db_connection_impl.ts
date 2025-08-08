import { ConnectionId, ProductBuilder, ProductType } from '../';
import { AlgebraicType, type ComparablePrimitive } from '../';
import { BinaryReader } from '../';
import { BinaryWriter } from '../';
import BsatnRowList from './client_api/bsatn_row_list_type.ts';
import ClientMessage from './client_api/client_message_type.ts';
import DatabaseUpdate from './client_api/database_update_type.ts';
import OneOffTable from './client_api/one_off_table_type.ts';
import QueryUpdate from './client_api/query_update_type.ts';
import ServerMessage from './client_api/server_message_type.ts';
import RawTableUpdate from './client_api/table_update_type.ts';
import {
  ClientCache,
  type TableDefForTableName,
  type TableName,
} from './client_cache.ts';
import { DbConnectionBuilder } from './db_connection_builder.ts';
import { type DbContext } from './db_context.ts';
import type { Event } from './event.ts';
import {
  type ErrorContextInterface,
  type EventContextInterface,
  type QueryEventContextInterface,
  type ReducerEventContextInterface,
  type SubscriptionEventContextInterface,
} from './event_context.ts';
import { EventEmitter } from './event_emitter.ts';
import { decompress } from './decompress.ts';
import type { Identity, Infer, InferTypeOfRow } from '../';
import type {
  IdentityTokenMessage,
  Message,
  ProcedureResultMessage,
  QueryResolvedMessage,
  SubscribeAppliedMessage,
  UnsubscribeAppliedMessage,
} from './message_types.ts';
import type { ReducerEvent } from './reducer_event.ts';
import { type UntypedRemoteModule } from './spacetime_module.ts';
import {
  type TableCache,
  type Operation,
  type PendingCallback,
  type TableUpdate as CacheTableUpdate,
} from './table_cache.ts';
import { WebsocketDecompressAdapter } from './websocket_decompress_adapter.ts';
import type { WebsocketTestAdapter } from './websocket_test_adapter.ts';
import {
  QueryBuilderImpl,
  QueryHandleImpl,
  QueryManager,
  type QueryEvent,
} from './query_builder_impl.ts';
import {
  SubscriptionBuilderImpl,
  SubscriptionHandleImpl,
  SubscriptionManager,
  type SubscribeEvent,
} from './subscription_builder_impl.ts';
import { stdbLogger } from './logger.ts';
import { fromByteArray } from 'base64-js';
import type {
  QueryEventCallback,
  ReducerEventCallback,
  ReducerEventInfo,
  ReducersView,
  SetReducerFlags,
  SubscriptionEventCallback,
} from './reducers.ts';
import type { ClientDbView } from './db_view.ts';
import type { RowType, UntypedTableDef } from '../lib/table.ts';
import { toCamelCase, toPascalCase } from '../lib/util.ts';
import type { ProceduresView } from './procedures.ts';

export {
  DbConnectionBuilder,
  SubscriptionBuilderImpl,
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
export type CallReducerFlags = 'FullUpdate' | 'NoSuccessNotify';

function callReducerFlagsToNumber(flags: CallReducerFlags): number {
  switch (flags) {
    case 'FullUpdate':
      return 0;
    case 'NoSuccessNotify':
      return 1;
  }
}

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

  /**
   * The accessor field to access the tables in the database and associated
   * callback functions.
   */
  db: ClientDbView<RemoteModule>;

  /**
   * The accessor field to access the reducers in the database and associated
   * callback functions.
   */
  reducers: ReducersView<RemoteModule>;

  /**
   * The accessor field to access functions related to setting flags on
   * reducers regarding how the server should handle the reducer call and
   * the events that it sends back to the client.
   */
  setReducerFlags: SetReducerFlags<RemoteModule>;

  /**
   * The accessor field to access the reducers in the database and associated
   * callback functions.
   */
  procedures: ProceduresView<RemoteModule>;

  /**
   * The `ConnectionId` of the connection to to the database.
   */
  connectionId: ConnectionId = ConnectionId.random();

  // These fields are meant to be strictly private.
  #queryId = 0;
  #requestId = 0;
  #emitter: EventEmitter<ConnectionEvent>;
  #reducerEmitter: EventEmitter<string, ReducerEventCallback<RemoteModule>> =
    new EventEmitter();
  #onApplied?: SubscriptionEventCallback<RemoteModule>;
  #messageQueue = Promise.resolve();
  #queryManager = new QueryManager<RemoteModule>();
  #subscriptionManager = new SubscriptionManager<RemoteModule>();
  #remoteModule: RemoteModule;
  #callReducerFlags = new Map<string, CallReducerFlags>();
  #procedureCallbacks = new Map<number, ProcedureCallback>();

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

    const connectionId = this.connectionId.toHexString();
    url.searchParams.set('connection_id', connectionId);

    this.clientCache = new ClientCache<RemoteModule>(this);
    this.db = this.#makeDbView(remoteModule);
    this.reducers = this.#makeReducers(remoteModule);
    this.setReducerFlags = this.#makeSetReducerFlags(remoteModule);
    this.procedures = this.#makeProcedures(remoteModule);

    this.wsPromise = createWSFn({
      url,
      nameOrAddress,
      wsProtocol: 'v1.bsatn.spacetimedb',
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

  #makeDbView(def: RemoteModule): ClientDbView<RemoteModule> {
    const view = Object.create(null) as ClientDbView<RemoteModule>;

    for (const tbl of def.tables) {
      // ClientDbView uses this name verbatim
      const key = tbl.accessorName;
      Object.defineProperty(view, key, {
        enumerable: true,
        configurable: false,
        get: () => {
          return this.clientCache.getOrCreateTable(tbl);
        },
      });
    }

    return view;
  }

  #makeReducers(def: RemoteModule): ReducersView<RemoteModule> {
    const out: Record<string, unknown> = {};

    for (const reducer of def.reducers) {
      const key = toCamelCase(reducer.name);

      (out as any)[key] = (params: InferTypeOfRow<typeof reducer.params>) => {
        const flags = this.#callReducerFlags.get(reducer.name) ?? 'FullUpdate';
        this.callReducerWithParams(
          reducer.name,
          reducer.paramsType,
          params,
          flags
        );
      };

      const onReducerEventKey = `on${toPascalCase(reducer.name)}`;
      (out as any)[onReducerEventKey] = (
        callback: ReducerEventCallback<
          RemoteModule,
          InferTypeOfRow<typeof reducer.params>
        >
      ) => {
        this.onReducer(reducer.name, callback);
      };

      const offReducerEventKey = `removeOn${toPascalCase(reducer.name)}`;
      (out as any)[offReducerEventKey] = (
        callback: ReducerEventCallback<
          RemoteModule,
          InferTypeOfRow<typeof reducer.params>
        >
      ) => {
        this.offReducer(reducer.name, callback);
      };
    }

    return out as ReducersView<RemoteModule>;
  }

  #makeSetReducerFlags(defs: RemoteModule): SetReducerFlags<RemoteModule> {
    const out = Object.create(null) as SetReducerFlags<RemoteModule>;
    for (const r of defs.reducers) {
      const key = toCamelCase(r.name);
      Object.defineProperty(out, key, {
        enumerable: true,
        configurable: false,
        value: (flags: CallReducerFlags) => {
          this.#callReducerFlags.set(r.name, flags);
        },
      });
    }
    return out;
  }

  #makeProcedures(def: RemoteModule): ProceduresView<RemoteModule> {
    const out: Record<string, unknown> = {};

    for (const procedure of def.procedures) {
      const key = toCamelCase(procedure.name);

      const paramsType = new ProductBuilder(procedure.params).algebraicType
        .value;

      const returnType = procedure.returnType.algebraicType;

      (out as any)[key] = (
        params: InferTypeOfRow<typeof procedure.params>
      ): Promise<any> =>
        this.callProcedureWithParams(
          procedure.name,
          paramsType,
          params,
          returnType
        );
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
      setReducerFlags: this.setReducerFlags,
      isActive: this.isActive,
      queryBuilder: this.queryBuilder.bind(this),
      subscriptionBuilder: this.subscriptionBuilder.bind(this),
      disconnect: this.disconnect.bind(this),
      event,
    };
  }

  queryBuilder = (): QueryBuilderImpl<RemoteModule> => {
    return new QueryBuilderImpl(this);
  };

  registerQuery(
    handle: QueryHandleImpl<RemoteModule>,
    handleEmitter: EventEmitter<
      QueryEvent,
      QueryEventCallback<RemoteModule>
    >,
    querySql: string,
  ): number {
    const queryId = this.#getNextQueryId();
    this.#queryManager.queries.set(queryId, {
      handle,
      emitter: handleEmitter,
    });
    this.#sendMessage(
      ClientMessage.OneOffQuery({
        queryString: querySql,
        messageId: new Uint8Array(new Uint32Array([queryId]).buffer),
      })
    );
    return queryId;
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

  registerSubscription(
    handle: SubscriptionHandleImpl<RemoteModule>,
    handleEmitter: EventEmitter<
      SubscribeEvent,
      SubscriptionEventCallback<RemoteModule>
    >,
    querySql: string[]
  ): number {
    const queryId = this.#getNextQueryId();
    this.#subscriptionManager.subscriptions.set(queryId, {
      handle,
      emitter: handleEmitter,
    });
    this.#sendMessage(
      ClientMessage.SubscribeMulti({
        queryStrings: querySql,
        queryId: { id: queryId },
        // The TypeScript SDK doesn't currently track `request_id`s,
        // so always use 0.
        requestId: 0,
      })
    );
    return queryId;
  }

  unregisterSubscription(queryId: number): void {
    this.#sendMessage(
      ClientMessage.UnsubscribeMulti({
        queryId: { id: queryId },
        // The TypeScript SDK doesn't currently track `request_id`s,
        // so always use 0.
        requestId: 0,
      })
    );
  }

  #parseRowList(
    type: 'insert' | 'delete',
    tableName: string,
    rowList: Infer<typeof BsatnRowList>
  ): Operation[] {
    const buffer = rowList.rowsData;
    const reader = new BinaryReader(buffer);
    const rows: Operation[] = [];

    // TODO: performance
    const table = this.#remoteModule.tables.find(t => t.name === tableName);
    const rowType = table!.rowType;
    const columnsArray = Object.entries(table!.columns);
    const primaryKeyColumnEntry = columnsArray.find(
      col => col[1].columnMetadata.isPrimaryKey
    );
    let previousOffset = 0;
    while (reader.remaining > 0) {
      const row = ProductType.deserializeValue(reader, rowType);
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

  // This function is async because we decompress the message async
  async #processParsedMessage(
    message: Infer<typeof ServerMessage>
  ): Promise<Message | undefined> {
    const parseTableUpdate = async (
      rawTableUpdate: Infer<typeof RawTableUpdate>
    ): Promise<CacheTableUpdate<UntypedTableDef>> => {
      const tableName = rawTableUpdate.tableName;
      let operations: Operation[] = [];
      for (const update of rawTableUpdate.updates) {
        let decompressed: Infer<typeof QueryUpdate>;
        if (update.tag === 'Gzip') {
          const decompressedBuffer = await decompress(update.value, 'gzip');
          decompressed = AlgebraicType.deserializeValue(
            new BinaryReader(decompressedBuffer),
            QueryUpdate.algebraicType
          );
        } else if (update.tag === 'Brotli') {
          throw new Error(
            'Brotli compression not supported. Please use gzip or none compression in withCompression method on DbConnection.'
          );
        } else {
          decompressed = update.value;
        }
        operations = operations.concat(
          this.#parseRowList('insert', tableName, decompressed.inserts)
        );
        operations = operations.concat(
          this.#parseRowList('delete', tableName, decompressed.deletes)
        );
      }
      return {
        tableName,
        operations,
      };
    };

    const parseDatabaseUpdate = async (
      dbUpdate: Infer<typeof DatabaseUpdate>
    ): Promise<CacheTableUpdate<UntypedTableDef>[]> => {
      const tableUpdates: CacheTableUpdate<UntypedTableDef>[] = [];
      for (const rawTableUpdate of dbUpdate.tables) {
        tableUpdates.push(await parseTableUpdate(rawTableUpdate));
      }
      return tableUpdates;
    };

    switch (message.tag) {
      case 'InitialSubscription': {
        const dbUpdate = message.value.databaseUpdate;
        const tableUpdates = await parseDatabaseUpdate(dbUpdate);
        const subscriptionUpdate: Message = {
          tag: 'InitialSubscription',
          tableUpdates,
        };
        return subscriptionUpdate;
      }

      case 'TransactionUpdateLight': {
        const dbUpdate = message.value.update;
        const tableUpdates = await parseDatabaseUpdate(dbUpdate);
        const subscriptionUpdate: Message = {
          tag: 'TransactionUpdateLight',
          tableUpdates,
        };
        return subscriptionUpdate;
      }

      case 'TransactionUpdate': {
        const txUpdate = message.value;
        const identity = txUpdate.callerIdentity;
        const connectionId = ConnectionId.nullIfZero(
          txUpdate.callerConnectionId
        );
        const reducerName: string = txUpdate.reducerCall.reducerName;
        const args = txUpdate.reducerCall.args;
        const energyQuantaUsed = txUpdate.energyQuantaUsed;

        let tableUpdates: CacheTableUpdate<UntypedTableDef>[] = [];
        let errMessage = '';
        switch (txUpdate.status.tag) {
          case 'Committed':
            tableUpdates = await parseDatabaseUpdate(txUpdate.status.value);
            break;
          case 'Failed':
            tableUpdates = [];
            errMessage = txUpdate.status.value;
            break;
          case 'OutOfEnergy':
            tableUpdates = [];
            break;
        }

        // TODO: Can `reducerName` be '<none>'?
        // See: https://github.com/clockworklabs/SpacetimeDB/blob/a2a1b5d9b2e0ebaaf753d074db056d319952d442/crates/core/src/client/message_handlers.rs#L155
        if (reducerName === '<none>') {
          const errorMessage = errMessage;
          console.error(`Received an error from the database: ${errorMessage}`);
          return;
        }

        let reducerInfo:
          | {
              reducerName: string;
              args: Uint8Array;
            }
          | undefined;
        if (reducerName !== '') {
          reducerInfo = {
            reducerName,
            args,
          };
        }

        const transactionUpdate: Message = {
          tag: 'TransactionUpdate',
          tableUpdates,
          identity,
          connectionId,
          reducerInfo,
          status: txUpdate.status,
          energyConsumed: energyQuantaUsed.quanta,
          message: errMessage,
          timestamp: txUpdate.timestamp,
        };
        return transactionUpdate;
      }

      case 'IdentityToken': {
        const identityTokenMessage: IdentityTokenMessage = {
          tag: 'IdentityToken',
          identity: message.value.identity,
          token: message.value.token,
          connectionId: message.value.connectionId,
        };
        return identityTokenMessage;
      }

      case 'OneOffQueryResponse': {
        const queryResolvedMessage: QueryResolvedMessage = {
          tag: 'QueryResolved',
          messageId: message.value.messageId,
          error: message.value.error,
          tables: message.value.tables,
          totalHostExecutionDuration: message.value.totalHostExecutionDuration,
        };
        return queryResolvedMessage;
      }

      case 'SubscribeMultiApplied': {
        const parsedTableUpdates = await parseDatabaseUpdate(
          message.value.update
        );
        const subscribeAppliedMessage: SubscribeAppliedMessage = {
          tag: 'SubscribeApplied',
          queryId: message.value.queryId.id,
          tableUpdates: parsedTableUpdates,
        };
        return subscribeAppliedMessage;
      }

      case 'UnsubscribeMultiApplied': {
        const parsedTableUpdates = await parseDatabaseUpdate(
          message.value.update
        );
        const unsubscribeAppliedMessage: UnsubscribeAppliedMessage = {
          tag: 'UnsubscribeApplied',
          queryId: message.value.queryId.id,
          tableUpdates: parsedTableUpdates,
        };
        return unsubscribeAppliedMessage;
      }

      case 'SubscriptionError': {
        return {
          tag: 'SubscriptionError',
          queryId: message.value.queryId,
          error: message.value.error,
        };
      }

      case 'ProcedureResult': {
        const { status, requestId } = message.value;
        return {
          tag: 'ProcedureResult',
          requestId,
          result:
            status.tag === 'Returned'
              ? { tag: 'Ok', value: status.value }
              : status.tag === 'OutOfEnergy'
                ? {
                    tag: 'Err',
                    value:
                      'Procedure execution aborted due to insufficient energy',
                  }
                : { tag: 'Err', value: status.value },
        };
      }
    }
  }

  #sendMessage(message: Infer<typeof ClientMessage>): void {
    this.wsPromise.then(wsResolved => {
      if (wsResolved) {
        const writer = new BinaryWriter(1024);
        AlgebraicType.serializeValue(
          writer,
          ClientMessage.algebraicType,
          message
        );
        const encoded = writer.getBuffer();
        wsResolved.send(encoded);
      }
    });
  }

  /**
   * Handles WebSocket onOpen event.
   */
  #handleOnOpen(): void {
    this.isActive = true;
  }

  #applyTablesState<N extends TableName<RemoteModule>>(
    tableStates: Infer<typeof OneOffTable>[],
    eventContext: EventContextInterface<RemoteModule>
  ): ClientCache<RemoteModule> {
    const state = new ClientCache<RemoteModule>(this);
    for (const tableState of tableStates) {
      // Get table information for the table being updated
      const tableName = tableState.tableName;
      // TODO: performance
      const tableDef = this.#remoteModule.tables.find(
        t => t.name === tableName
      )!;
      const table = state.getOrCreateTable(tableDef);
      const operations = this.#parseRowList('insert', tableState.tableName, tableState.rows);
      table.applyOperations(
        operations as Operation<RowType<TableDefForTableName<RemoteModule, string>>>[],
        eventContext,
      );
    }
    return state;
  }

  #applyTableUpdates(
    tableUpdates: CacheTableUpdate<UntypedTableDef>[],
    eventContext: EventContextInterface<RemoteModule>
  ): PendingCallback[] {
    const pendingCallbacks: PendingCallback[] = [];
    for (const tableUpdate of tableUpdates) {
      // Get table information for the table being updated
      const tableName = tableUpdate.tableName;
      // TODO: performance
      const tableDef = this.#remoteModule.tables.find(
        t => t.name === tableName
      )!;
      const table = this.clientCache.getOrCreateTable(tableDef);
      const newCallbacks = table.applyOperations(
        tableUpdate.operations,
        eventContext
      );
      for (const callback of newCallbacks) {
        pendingCallbacks.push(callback);
      }
    }
    return pendingCallbacks;
  }

  async #processMessage(data: Uint8Array): Promise<void> {
    const serverMessage = AlgebraicType.deserializeValue(
      new BinaryReader(data),
      ServerMessage.algebraicType
    );
    const message = await this.#processParsedMessage(serverMessage);
    if (!message) {
      return;
    }
    switch (message.tag) {
      case 'InitialSubscription': {
        const event: Event<never> = { tag: 'SubscribeApplied' };
        const eventContext = this.#makeEventContext(event);
        // Remove the event from the subscription event context
        // It is not a field in the type narrowed SubscriptionEventContext
        const { event: _, ...subscriptionEventContext } = eventContext;
        const callbacks = this.#applyTableUpdates(
          message.tableUpdates,
          eventContext
        );

        if (this.#emitter) {
          this.#onApplied?.(subscriptionEventContext);
        }
        for (const callback of callbacks) {
          callback.cb();
        }
        break;
      }
      case 'TransactionUpdateLight': {
        const event: Event<never> = { tag: 'UnknownTransaction' };
        const eventContext = this.#makeEventContext(event);
        const callbacks = this.#applyTableUpdates(
          message.tableUpdates,
          eventContext
        );
        for (const callback of callbacks) {
          callback.cb();
        }
        break;
      }
      case 'TransactionUpdate': {
        let reducerInfo = message.reducerInfo;
        let unknownTransaction = false;
        let reducerArgs: InferTypeOfRow<typeof reducer.params> | undefined;
        const reducer = this.#remoteModule.reducers.find(
          t => t.name === reducerInfo!.reducerName
        )!;
        if (!reducerInfo) {
          unknownTransaction = true;
        } else {
          // TODO: performance
          try {
            const reader = new BinaryReader(reducerInfo.args as Uint8Array);
            reducerArgs = ProductType.deserializeValue(
              reader,
              reducer?.paramsType
            );
          } catch {
            // This should only be printed in development, since it's
            // possible for clients to receive new reducers that they don't
            // know about.
            console.debug('Failed to deserialize reducer arguments');
            unknownTransaction = true;
          }
        }

        if (unknownTransaction) {
          const event: Event<never> = { tag: 'UnknownTransaction' };
          const eventContext = this.#makeEventContext(event);
          const callbacks = this.#applyTableUpdates(
            message.tableUpdates,
            eventContext
          );

          for (const callback of callbacks) {
            callback.cb();
          }
          return;
        }

        // At this point, we know that `reducerInfo` is not null because
        // we return if `unknownTransaction` is true.
        reducerInfo = reducerInfo!;
        reducerArgs = reducerArgs!;

        // Thus this must be a reducer event create it and emit it.
        const reducerEvent = {
          callerIdentity: message.identity,
          status: message.status,
          callerConnectionId: message.connectionId as ConnectionId,
          timestamp: message.timestamp,
          energyConsumed: message.energyConsumed,
          reducer: {
            name: reducerInfo.reducerName,
            args: reducerArgs,
          },
        };
        const event: Event<typeof reducerEvent.reducer> = {
          tag: 'Reducer',
          value: reducerEvent,
        };
        const eventContext = this.#makeEventContext(event);
        const reducerEventContext = {
          ...eventContext,
          event: reducerEvent,
        };

        const callbacks = this.#applyTableUpdates(
          message.tableUpdates,
          eventContext
        );

        const argsArray: any[] = [];
        reducer.paramsType.elements.forEach(element => {
          argsArray.push(reducerArgs[element.name!]);
        });
        this.#reducerEmitter.emit(
          reducerInfo.reducerName,
          reducerEventContext,
          ...argsArray
        );
        for (const callback of callbacks) {
          callback.cb();
        }
        break;
      }
      case 'IdentityToken': {
        this.identity = message.identity;
        if (!this.token && message.token) {
          this.token = message.token;
        }
        this.connectionId = message.connectionId;
        this.#emitter.emit('connect', this, this.identity, this.token);
        break;
      }
      case 'QueryResolved': {
        if (message.messageId?.length != 4) {
          stdbLogger(
            'error',
            `Received QueryResolved with invalid messageId ${message.messageId}.`
          );
          break;
        }
        const queryId = new DataView(message.messageId.buffer, message.messageId.byteOffset, 4).getUint32(0, true);
        const query = this.#queryManager.queries.get(queryId);
        if (query === undefined) {
          stdbLogger(
            'error',
            `Received QueryResolved for unknown queryId ${queryId}.`
          );
          break;
        }
        if (message.error !== undefined) {
          const error = Error(message.error);
          const event: Event<never> = { tag: 'Error', value: error };
          const eventContext = this.#makeEventContext(event);
          const errorContext = {
            ...eventContext,
            event: error,
          };
          this.#queryManager.queries
            .get(queryId)
            ?.emitter.emit(
              'error',
              errorContext,
              error,
              message.totalHostExecutionDuration
            );
        } else {
          const event: Event<never> = { tag: 'QueryResolved' };
          const eventContext = this.#makeEventContext(event);
          const { event: _, ...queryEventContext } = eventContext;
          const state = this.#applyTablesState(
            message.tables,
            eventContext
          );
          query?.emitter.emit(
            'resolved',
            queryEventContext,
            state.tables,
            message.totalHostExecutionDuration
          );
        }
        this.#queryManager.queries.delete(queryId);
        break;
      }
      case 'SubscribeApplied': {
        const subscription = this.#subscriptionManager.subscriptions.get(
          message.queryId
        );
        if (subscription === undefined) {
          stdbLogger(
            'error',
            `Received SubscribeApplied for unknown queryId ${message.queryId}.`
          );
          // If we don't know about the subscription, we won't apply the table updates.
          break;
        }
        const event: Event<never> = { tag: 'SubscribeApplied' };
        const eventContext = this.#makeEventContext(event);
        const { event: _, ...subscriptionEventContext } = eventContext;
        const callbacks = this.#applyTableUpdates(
          message.tableUpdates,
          eventContext
        );
        subscription?.emitter.emit('applied', subscriptionEventContext);
        for (const callback of callbacks) {
          callback.cb();
        }
        break;
      }
      case 'UnsubscribeApplied': {
        const subscription = this.#subscriptionManager.subscriptions.get(
          message.queryId
        );
        if (subscription === undefined) {
          stdbLogger(
            'error',
            `Received UnsubscribeApplied for unknown queryId ${message.queryId}.`
          );
          // If we don't know about the subscription, we won't apply the table updates.
          break;
        }
        const event: Event<never> = { tag: 'UnsubscribeApplied' };
        const eventContext = this.#makeEventContext(event);
        const { event: _, ...subscriptionEventContext } = eventContext;
        const callbacks = this.#applyTableUpdates(
          message.tableUpdates,
          eventContext
        );
        subscription?.emitter.emit('end', subscriptionEventContext);
        this.#subscriptionManager.subscriptions.delete(message.queryId);
        for (const callback of callbacks) {
          callback.cb();
        }
        break;
      }
      case 'SubscriptionError': {
        const error = Error(message.error);
        const event: Event<never> = { tag: 'Error', value: error };
        const eventContext = this.#makeEventContext(event);
        const errorContext = {
          ...eventContext,
          event: error,
        };
        if (message.queryId !== undefined) {
          this.#subscriptionManager.subscriptions
            .get(message.queryId)
            ?.emitter.emit('error', errorContext, error);
          this.#subscriptionManager.subscriptions.delete(message.queryId);
        } else {
          console.error('Received an error message without a queryId: ', error);
          // TODO: This should actually kill the connection.
          // A subscription error without a specific subscription means we aren't receiving
          // updates for all of our subscriptions, so our cache is out of sync.

          // Send it to all of them:
          this.#subscriptionManager.subscriptions.forEach(({ emitter }) => {
            emitter.emit('error', errorContext, error);
          });
        }
        break;
      }
      case 'ProcedureResult': {
        const { requestId, result } = message;
        const cb = this.#procedureCallbacks.get(requestId);
        this.#procedureCallbacks.delete(requestId);
        cb?.(result);
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
    flags: CallReducerFlags
  ): void {
    const message = ClientMessage.CallReducer({
      reducer: reducerName,
      args: argsBuffer,
      // The TypeScript SDK doesn't currently track `request_id`s,
      // so always use 0.
      requestId: 0,
      flags: callReducerFlagsToNumber(flags),
    });
    this.#sendMessage(message);
  }

  /**
   * Call a reducer on your SpacetimeDB module with typed arguments.
   * @param reducerSchema The schema of the reducer to call
   * @param callReducerFlags The flags for the reducer call
   * @param params The arguments to pass to the reducer
   */
  callReducerWithParams(
    reducerName: string,
    paramsType: ProductType,
    params: object,
    flags: CallReducerFlags
  ) {
    const writer = new BinaryWriter(1024);
    ProductType.serializeValue(writer, paramsType, params);
    const argsBuffer = writer.getBuffer();
    this.callReducer(reducerName, argsBuffer, flags);
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
    paramsType: ProductType,
    params: object,
    returnType: AlgebraicType
  ): Promise<any> {
    const writer = new BinaryWriter(1024);
    ProductType.serializeValue(writer, paramsType, params);
    const argsBuffer = writer.getBuffer();
    return this.callProcedure(procedureName, argsBuffer).then(returnBuf => {
      return AlgebraicType.deserializeValue(
        new BinaryReader(returnBuf),
        returnType
      );
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

  // Note: This is required to be public because it needs to be
  // called from the `RemoteReducers` class.
  onReducer(
    reducerName: string,
    callback: ReducerEventCallback<RemoteModule>
  ): void {
    this.#reducerEmitter.on(reducerName, callback);
  }

  // Note: This is required to be public because it needs to be
  // called from the `RemoteReducers` class.
  offReducer(
    reducerName: string,
    callback: ReducerEventCallback<RemoteModule>
  ): void {
    this.#reducerEmitter.off(reducerName, callback);
  }
}
