import { ConnectionId } from './connection_id';
import {
  AlgebraicType,
  ProductType,
  ProductTypeElement,
  SumType,
  SumTypeVariant,
} from './algebraic_type.ts';
import {
  AlgebraicValue,
  parseValue,
  ProductValue,
  type ReducerArgsAdapter,
  type ValueAdapter,
} from './algebraic_value.ts';
import BinaryReader from './binary_reader.ts';
import BinaryWriter from './binary_writer.ts';
import * as ws from './client_api/index.ts';
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
import type { Identity } from './identity.ts';
import type {
  IdentityTokenMessage,
  Message,
  SubscribeAppliedMessage,
  UnsubscribeAppliedMessage,
} from './message_types.ts';
import type { ReducerEvent } from './reducer_event.ts';
import type RemoteModule from './spacetime_module.ts';
import { TableCache, type Operation, type TableUpdate } from './table_cache.ts';
import { deepEqual, toPascalCase } from './utils.ts';
import { WebsocketDecompressAdapter } from './websocket_decompress_adapter.ts';
import type { WebsocketTestAdapter } from './websocket_test_adapter.ts';
import {
  SubscriptionBuilderImpl,
  SubscriptionHandleImpl,
  SubscriptionManager,
  type SubscribeEvent,
} from './subscription_builder_impl.ts';
import { stdbLogger } from './logger.ts';
import type { ReducerRuntimeTypeInfo } from './spacetime_module.ts';

export {
  AlgebraicType,
  AlgebraicValue,
  BinaryReader,
  BinaryWriter,
  DbConnectionBuilder,
  deepEqual,
  ProductType,
  ProductTypeElement,
  ProductValue,
  SubscriptionBuilderImpl,
  SumType,
  SumTypeVariant,
  TableCache,
  type Event,
  type ReducerArgsAdapter,
  type ValueAdapter,
};

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
type ErrorCallback = (ctx: ErrorContextInterface) => void;

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
  private wsPromise: Promise<WebsocketDecompressAdapter | WebsocketTestAdapter>;

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
  }: DbConnectionConfig) {
    stdbLogger('info', 'Connecting to SpacetimeDB WS...');

    let url = new URL(`database/subscribe/${nameOrAddress}`, uri);

    if (!/^wss?:/.test(uri.protocol)) {
      url.protocol = 'ws:';
    }

    this.identity = identity;
    this.token = token;

    this.#remoteModule = remoteModule;
    this.#emitter = emitter;

    let connectionId = this.connectionId.toHexString();
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
      wsProtocol: 'v1.bsatn.spacetimedb',
      authToken: token,
      compression: compression,
      lightMode: lightMode,
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
        this.#on('connectError', e);
        // TODO(cloutiertyler): I don't know but this makes it compile and
        // I don't have time to investigate how to do this properly.
        // Otherwise `.catch` returns void.
        throw e;
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
    querySql: string
  ): number {
    const queryId = this.#getNextQueryId();
    this.#subscriptionManager.subscriptions.set(queryId, {
      handle,
      emitter: handleEmitter,
    });
    this.#sendMessage(
      ws.ClientMessage.SubscribeSingle({
        query: querySql,
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
      ws.ClientMessage.Unsubscribe({
        queryId: { id: queryId },
        // The TypeScript SDK doesn't currently track `request_id`s,
        // so always use 0.
        requestId: 0,
      })
    );
  }

  // This function is async because we decompress the message async
  async #processParsedMessage(
    message: ws.ServerMessage
  ): Promise<Message | undefined> {
    const parseRowList = (
      type: 'insert' | 'delete',
      tableName: string,
      rowList: ws.BsatnRowList
    ): Operation[] => {
      const buffer = rowList.rowsData;
      const length = buffer.length;
      let offset = buffer.byteOffset;
      const endingOffset = offset + length;
      const reader = new BinaryReader(buffer);
      const rows: any[] = [];
      const rowType = this.#remoteModule.tables[tableName]!.rowType;
      while (offset < endingOffset) {
        const row = rowType.deserialize(reader);
        const rowId = new TextDecoder('utf-8').decode(buffer);
        rows.push({
          type,
          rowId,
          row,
        });
        offset = reader.offset;
      }
      return rows;
    };

    const parseTableUpdate = async (
      rawTableUpdate: ws.TableUpdate
    ): Promise<TableUpdate> => {
      const tableName = rawTableUpdate.tableName;
      let operations: Operation[] = [];
      for (const update of rawTableUpdate.updates) {
        let decompressed: ws.QueryUpdate;
        if (update.tag === 'Gzip') {
          const decompressedBuffer = await decompress(update.value, 'gzip');
          decompressed = ws.QueryUpdate.deserialize(
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
      dbUpdate: ws.DatabaseUpdate
    ): Promise<TableUpdate[]> => {
      const tableUpdates: TableUpdate[] = [];
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
        const originalReducerName = txUpdate.reducerCall.reducerName;
        const reducerName: string = toPascalCase(originalReducerName);
        const args = txUpdate.reducerCall.args;
        const energyQuantaUsed = txUpdate.energyQuantaUsed;

        let tableUpdates: TableUpdate[];
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
        if (originalReducerName === '<none>') {
          let errorMessage = errMessage;
          console.error(`Received an error from the database: ${errorMessage}`);
          return;
        }

        let reducerInfo:
          | {
              originalReducerName: string;
              reducerName: string;
              args: Uint8Array;
            }
          | undefined;
        if (originalReducerName !== '') {
          reducerInfo = {
            originalReducerName,
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

      case 'SubscribeApplied': {
        const parsedTableUpdate = await parseTableUpdate(
          message.value.rows.tableRows
        );
        const subscribeAppliedMessage: SubscribeAppliedMessage = {
          tag: 'SubscribeApplied',
          queryId: message.value.queryId.id,
          tableUpdate: parsedTableUpdate,
        };
        return subscribeAppliedMessage;
      }

      case 'UnsubscribeApplied': {
        const parsedTableUpdate = await parseTableUpdate(
          message.value.rows.tableRows
        );
        const unsubscribeAppliedMessage: UnsubscribeAppliedMessage = {
          tag: 'UnsubscribeApplied',
          queryId: message.value.queryId.id,
          tableUpdate: parsedTableUpdate,
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

  #sendMessage(message: ws.ClientMessage): void {
    this.wsPromise.then(wsResolved => {
      const writer = new BinaryWriter(1024);
      ws.ClientMessage.serialize(writer, message);
      const encoded = writer.getBuffer();
      wsResolved.send(encoded);
    });
  }

  /**
   * Handles WebSocket onOpen event.
   */
  #handleOnOpen(): void {
    this.isActive = true;
  }

  #applyTableUpdates(
    tableUpdates: TableUpdate[],
    eventContext: EventContextInterface
  ): void {
    for (let tableUpdate of tableUpdates) {
      // Get table information for the table being updated
      const tableName = tableUpdate.tableName;
      const tableTypeInfo = this.#remoteModule.tables[tableName]!;
      const table = this.clientCache.getOrCreateTable(tableTypeInfo);
      table.applyOperations(tableUpdate.operations, eventContext);
    }
  }

  async #processMessage(data: Uint8Array): Promise<void> {
    const serverMessage = parseValue(ws.ServerMessage, data);
    const message = await this.#processParsedMessage(serverMessage);
    if (!message) {
      return;
    }
    switch (message.tag) {
      case 'InitialSubscription': {
        let event: Event<never> = { tag: 'SubscribeApplied' };

        const eventContext = this.#remoteModule.eventContextConstructor(
          this,
          event
        );
        // Remove the event from the subscription event context
        // It is not a field in the type narrowed SubscriptionEventContext
        const { event: _, ...subscriptionEventContext } = eventContext;
        this.#applyTableUpdates(message.tableUpdates, eventContext);

        if (this.#emitter) {
          this.#onApplied?.(subscriptionEventContext);
        }
        break;
      }
      case 'TransactionUpdateLight': {
        let event: Event<never> = { tag: 'UnknownTransaction' };
        const eventContext = this.#remoteModule.eventContextConstructor(
          this,
          event
        );
        this.#applyTableUpdates(message.tableUpdates, eventContext);
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
          const reducerTypeInfo =
            this.#remoteModule.reducers[reducerInfo.originalReducerName];
          try {
            const reader = new BinaryReader(reducerInfo.args as Uint8Array);
            reducerArgs = reducerTypeInfo.argsType.deserialize(reader);
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
          this.#applyTableUpdates(message.tableUpdates, eventContext);
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

        this.#applyTableUpdates(message.tableUpdates, eventContext);

        const argsArray: any[] = [];
        reducerTypeInfo.argsType.product.elements.forEach((element, index) => {
          argsArray.push(reducerArgs[element.name]);
        });
        this.#reducerEmitter.emit(
          reducerInfo.reducerName,
          reducerEventContext,
          ...argsArray
        );
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
        const event: Event<never> = { tag: 'SubscribeApplied' };
        const eventContext = this.#remoteModule.eventContextConstructor(
          this,
          event
        );
        const { event: _, ...subscriptionEventContext } = eventContext;
        this.#applyTableUpdates([message.tableUpdate], eventContext);
        this.#subscriptionManager.subscriptions
          .get(message.queryId)
          ?.emitter.emit('applied', subscriptionEventContext);
        break;
      }
      case 'UnsubscribeApplied': {
        const event: Event<never> = { tag: 'UnsubscribeApplied' };
        const eventContext = this.#remoteModule.eventContextConstructor(
          this,
          event
        );
        const { event: _, ...subscriptionEventContext } = eventContext;
        this.#applyTableUpdates([message.tableUpdate], eventContext);
        this.#subscriptionManager.subscriptions
          .get(message.queryId)
          ?.emitter.emit('end', subscriptionEventContext);
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
        if (message.queryId) {
          this.#subscriptionManager.subscriptions
            .get(message.queryId)
            ?.emitter.emit('error', errorContext, error);
        } else {
          console.error('Received an error message without a queryId: ', error);
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
    const message = ws.ClientMessage.CallReducer({
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
      wsResolved.close();
    });
  }

  #on(
    eventName: ConnectionEvent,
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.on(eventName, callback);
  }

  #off(
    eventName: ConnectionEvent,
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.off(eventName, callback);
  }

  #onConnect(callback: (ctx: DbConnectionImpl, ...args: any[]) => void): void {
    this.#emitter.on('connect', callback);
  }

  #onDisconnect(
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.on('disconnect', callback);
  }

  #onConnectError(
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.on('connectError', callback);
  }

  #removeOnConnect(
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.off('connect', callback);
  }

  #removeOnDisconnect(
    callback: (ctx: DbConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.off('disconnect', callback);
  }

  #removeOnConnectError(
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
