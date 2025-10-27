import { ConnectionId } from '../';
import {
  AlgebraicType,
  type AlgebraicTypeVariants,
  type ComparablePrimitive,
} from '../';
import { parseValue } from '../';
import { BinaryReader } from '../';
import { BinaryWriter } from '../';
import { BsatnRowList } from './client_api/bsatn_row_list_type.ts';
import { ClientMessage } from './client_api/client_message_type.ts';
import { DatabaseUpdate } from './client_api/database_update_type.ts';
import { QueryUpdate } from './client_api/query_update_type.ts';
import { ServerMessage } from './client_api/server_message_type.ts';
import { TableUpdate as RawTableUpdate } from './client_api/table_update_type.ts';
import { ClientCache } from './client_cache.ts';
import { DbConnectionBuilder } from './db_connection_builder.ts';
import { type DbContext } from './db_context.ts';
import type { Event } from './event.ts';
import {
  type ErrorContextInterface,
  type EventContextInterface,
  type ReducerEventContextInterface,
  type SubscriptionEventContextInterface,
} from './event_context.ts';
import { EventEmitter } from './event_emitter.ts';
import { decompress } from './decompress.ts';
import type { Identity } from '../';
import type {
  IdentityTokenMessage,
  Message,
  SubscribeAppliedMessage,
  UnsubscribeAppliedMessage,
} from './message_types.ts';
import type { ReducerEvent } from './reducer_event.ts';
import type RemoteModule from './spacetime_module.ts';
import {
  TableCache,
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
import { type ReducerRuntimeTypeInfo } from './spacetime_module.ts';
import { fromByteArray } from 'base64-js';

export { DbConnectionBuilder, SubscriptionBuilderImpl, TableCache, type Event };

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

type ReducerEventCallback<ReducerArgs extends any[] = any[]> = (
  ctx: ReducerEventContextInterface,
  ...args: ReducerArgs
) => void;
type SubscriptionEventCallback = (
  ctx: SubscriptionEventContextInterface
) => void;

function callReducerFlagsToNumber(flags: CallReducerFlags): number {
  switch (flags) {
    case 'FullUpdate':
      return 0;
    case 'NoSuccessNotify':
      return 1;
  }
}

type DbConnectionConfig = {
  uri: URL;
  nameOrAddress: string;
  identity?: Identity;
  token?: string;
  emitter: EventEmitter<ConnectionEvent>;
  remoteModule: RemoteModule;
  createWSFn: typeof WebsocketDecompressAdapter.createWebSocketFn;
  compression: 'gzip' | 'none';
  lightMode: boolean;
  confirmedReads?: boolean;
};

export class DbConnectionImpl<
  DBView = any,
  Reducers = any,
  SetReducerFlags = any,
> implements DbContext<DBView, Reducers>
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
  db: DBView;

  /**
   * The accessor field to access the reducers in the database and associated
   * callback functions.
   */
  reducers: Reducers;

  /**
   * The accessor field to access functions related to setting flags on
   * reducers regarding how the server should handle the reducer call and
   * the events that it sends back to the client.
   */
  setReducerFlags: SetReducerFlags;

  /**
   * The `ConnectionId` of the connection to to the database.
   */
  connectionId: ConnectionId = ConnectionId.random();

  // These fields are meant to be strictly private.
  #queryId = 0;
  #emitter: EventEmitter<ConnectionEvent>;
  #reducerEmitter: EventEmitter<string, ReducerEventCallback> =
    new EventEmitter();
  #onApplied?: SubscriptionEventCallback;
  #remoteModule: RemoteModule;
  #messageQueue = Promise.resolve();
  #subscriptionManager = new SubscriptionManager();

  // These fields are not part of the public API, but in a pinch you
  // could use JavaScript to access them by bypassing TypeScript's
  // private fields.
  // We use them in testing.
  private clientCache: ClientCache;
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
  }: DbConnectionConfig) {
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

    this.clientCache = new ClientCache();
    this.db = this.#remoteModule.dbViewConstructor(this);
    this.setReducerFlags = this.#remoteModule.setReducerFlagsConstructor();
    this.reducers = this.#remoteModule.reducersConstructor(
      this,
      this.setReducerFlags
    );

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
        };
        this.ws.onerror = (e: ErrorEvent) => {
          this.#emitter.emit('connectError', this, e);
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

  // NOTE: This is very important!!! This is the actual function that
  // gets called when you call `connection.subscriptionBuilder()`.
  // The `subscriptionBuilder` function which is generated, just shadows
  // this function in the type system, but not the actual implementation!
  // Do not remove this function, or shoot yourself in the foot please.
  // It's not clear what would be a better way to do this at this exact
  // moment.
  subscriptionBuilder = (): SubscriptionBuilderImpl => {
    return new SubscriptionBuilderImpl(this);
  };

  registerSubscription(
    handle: SubscriptionHandleImpl<DBView, Reducers, SetReducerFlags>,
    handleEmitter: EventEmitter<SubscribeEvent, SubscriptionEventCallback>,
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

  // This function is async because we decompress the message async
  async #processParsedMessage(
    message: ServerMessage
  ): Promise<Message | undefined> {
    const parseRowList = (
      type: 'insert' | 'delete',
      tableName: string,
      rowList: BsatnRowList
    ): Operation[] => {
      const buffer = rowList.rowsData;
      const reader = new BinaryReader(buffer);
      const rows: Operation[] = [];
      const rowType = this.#remoteModule.tables[tableName]!.rowType;
      const primaryKeyInfo =
        this.#remoteModule.tables[tableName]!.primaryKeyInfo;
      while (reader.remaining > 0) {
        const row = AlgebraicType.deserializeValue(reader, rowType);
        let rowId: ComparablePrimitive | undefined = undefined;
        if (primaryKeyInfo !== undefined) {
          rowId = AlgebraicType.intoMapKey(
            primaryKeyInfo.colType,
            row[primaryKeyInfo.colName]
          );
        } else {
          // Get a view of the bytes for this row.
          const rowBytes = buffer.subarray(0, reader.offset);
          // Convert it to a base64 string, so we can use it as a map key.
          const asBase64 = fromByteArray(rowBytes);
          rowId = asBase64;
        }

        rows.push({
          type,
          rowId,
          row,
        });
      }
      return rows;
    };

    const parseTableUpdate = async (
      rawTableUpdate: RawTableUpdate
    ): Promise<CacheTableUpdate> => {
      const tableName = rawTableUpdate.tableName;
      let operations: Operation[] = [];
      for (const update of rawTableUpdate.updates) {
        let decompressed: QueryUpdate;
        if (update.tag === 'Gzip') {
          const decompressedBuffer = await decompress(update.value, 'gzip');
          decompressed = QueryUpdate.deserialize(
            new BinaryReader(decompressedBuffer)
          );
        } else if (update.tag === 'Brotli') {
          throw new Error(
            'Brotli compression not supported. Please use gzip or none compression in withCompression method on DbConnection.'
          );
        } else {
          decompressed = update.value;
        }
        operations = operations.concat(
          parseRowList('insert', tableName, decompressed.inserts)
        );
        operations = operations.concat(
          parseRowList('delete', tableName, decompressed.deletes)
        );
      }
      return {
        tableName,
        operations,
      };
    };

    const parseDatabaseUpdate = async (
      dbUpdate: DatabaseUpdate
    ): Promise<CacheTableUpdate[]> => {
      const tableUpdates: CacheTableUpdate[] = [];
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

        let tableUpdates: CacheTableUpdate[] = [];
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
        throw new Error(
          `TypeScript SDK never sends one-off queries, but got OneOffQueryResponse ${message}`
        );
      }

      case 'SubscribeMultiApplied': {
        const parsedTableUpdates = await parseDatabaseUpdate(
          message.value.update
        );
        const subscribeAppliedMessage: SubscribeAppliedMessage<
          Record<string, any>
        > = {
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
        const unsubscribeAppliedMessage: UnsubscribeAppliedMessage<
          Record<string, any>
        > = {
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
    }
  }

  #sendMessage(message: ClientMessage): void {
    this.wsPromise.then(wsResolved => {
      if (wsResolved) {
        const writer = new BinaryWriter(1024);
        ClientMessage.serialize(writer, message);
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

  #applyTableUpdates(
    tableUpdates: CacheTableUpdate[],
    eventContext: EventContextInterface
  ): PendingCallback[] {
    const pendingCallbacks: PendingCallback[] = [];
    for (const tableUpdate of tableUpdates) {
      // Get table information for the table being updated
      const tableName = tableUpdate.tableName;
      const tableTypeInfo = this.#remoteModule.tables[tableName]!;
      const table = this.clientCache.getOrCreateTable(tableTypeInfo);
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
    const serverMessage = parseValue(ServerMessage, data);
    const message = await this.#processParsedMessage(serverMessage);
    if (!message) {
      return;
    }
    switch (message.tag) {
      case 'InitialSubscription': {
        const event: Event<never> = { tag: 'SubscribeApplied' };

        const eventContext = this.#remoteModule.eventContextConstructor(
          this,
          event
        );
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
        const eventContext = this.#remoteModule.eventContextConstructor(
          this,
          event
        );
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
        let reducerArgs: any | undefined;
        let reducerTypeInfo: ReducerRuntimeTypeInfo | undefined;
        if (!reducerInfo) {
          unknownTransaction = true;
        } else {
          reducerTypeInfo =
            this.#remoteModule.reducers[reducerInfo.reducerName];
          try {
            const reader = new BinaryReader(reducerInfo.args as Uint8Array);
            reducerArgs = AlgebraicType.deserializeValue(
              reader,
              reducerTypeInfo.argsType
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
          const eventContext = this.#remoteModule.eventContextConstructor(
            this,
            event
          );
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
        reducerTypeInfo = reducerTypeInfo!;

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
        const eventContext = this.#remoteModule.eventContextConstructor(
          this,
          event
        );
        const reducerEventContext = {
          ...eventContext,
          event: reducerEvent,
        };

        const callbacks = this.#applyTableUpdates(
          message.tableUpdates,
          eventContext
        );

        const argsArray: any[] = [];
        (
          reducerTypeInfo.argsType as AlgebraicTypeVariants.Product
        ).value.elements.forEach(element => {
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
        const eventContext = this.#remoteModule.eventContextConstructor(
          this,
          event
        );
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
        const eventContext = this.#remoteModule.eventContextConstructor(
          this,
          event
        );
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
        const eventContext = this.#remoteModule.eventContextConstructor(
          this,
          event
        );
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
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.on(eventName, callback);
  }

  private off(
    eventName: ConnectionEvent,
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.off(eventName, callback);
  }

  private onConnect(
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.on('connect', callback);
  }

  private onDisconnect(
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.on('disconnect', callback);
  }

  private onConnectError(
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.on('connectError', callback);
  }

  private removeOnConnect(
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.off('connect', callback);
  }

  private removeOnDisconnect(
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.off('disconnect', callback);
  }

  private removeOnConnectError(
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.off('connectError', callback);
  }

  // Note: This is required to be public because it needs to be
  // called from the `RemoteReducers` class.
  onReducer(reducerName: string, callback: ReducerEventCallback): void {
    this.#reducerEmitter.on(reducerName, callback);
  }

  // Note: This is required to be public because it needs to be
  // called from the `RemoteReducers` class.
  offReducer(reducerName: string, callback: ReducerEventCallback): void {
    this.#reducerEmitter.off(reducerName, callback);
  }
}
