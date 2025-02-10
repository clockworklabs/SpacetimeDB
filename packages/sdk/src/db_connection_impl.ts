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
import { DBConnectionBuilder } from './db_connection_builder.ts';
import { type DBContext } from './db_context.ts';
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
import type { IdentityTokenMessage, Message } from './message_types.ts';
import type { ReducerEvent } from './reducer_event.ts';
import type SpacetimeModule from './spacetime_module.ts';
import { TableCache, type Operation, type TableUpdate } from './table_cache.ts';
import { deepEqual, toPascalCase } from './utils.ts';
import { WebsocketDecompressAdapter } from './websocket_decompress_adapter.ts';
import type { WebsocketTestAdapter } from './websocket_test_adapter.ts';
import { SubscriptionBuilderImpl } from './subscription_builder_impl.ts';

export {
  AlgebraicType,
  AlgebraicValue,
  BinaryReader,
  BinaryWriter,
  DBConnectionBuilder,
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
  DBContext,
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

export class DBConnectionImpl<
  DBView = any,
  Reducers = any,
  SetReducerFlags = any,
> implements DBContext<DBView, Reducers>
{
  isActive = false;
  /**
   * The user's public identity.
   */
  identity?: Identity = undefined;
  /**
   * The user's private authentication token.
   */
  token?: string = undefined;

  /**
   * Reference to the database of the client.
   */
  clientCache: ClientCache;
  remoteModule: SpacetimeModule;
  #emitter: EventEmitter;
  #reducerEmitter: EventEmitter<ReducerEventCallback> = new EventEmitter();
  #onApplied?: SubscriptionEventCallback;

  wsPromise!: Promise<WebsocketDecompressAdapter | WebsocketTestAdapter>;
  ws?: WebsocketDecompressAdapter | WebsocketTestAdapter;
  db: DBView;
  reducers: Reducers;
  setReducerFlags: SetReducerFlags;

  connectionId: ConnectionId = ConnectionId.random();

  #messageQueue = Promise.resolve();

  constructor(remoteModule: SpacetimeModule, emitter: EventEmitter) {
    this.clientCache = new ClientCache();
    this.#emitter = emitter;
    this.remoteModule = remoteModule;
    this.db = this.remoteModule.dbViewConstructor(this);
    this.setReducerFlags = this.remoteModule.setReducerFlagsConstructor();
    this.reducers = this.remoteModule.reducersConstructor(
      this,
      this.setReducerFlags
    );
  }

  /**
   * Close the current connection.
   *
   * @example
   *
   * ```ts
   * const connection = DBConnection.builder().build();
   * connection.disconnect()
   * ```
   */
  disconnect(): void {
    this.wsPromise.then(wsResolved => {
      wsResolved.close();
    });
  }

  async #processParsedMessage(
    message: ws.ServerMessage,
    callback: (message: Message) => void
  ) {
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
      const rowType = this.remoteModule.tables[tableName]!.rowType;
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
        callback(subscriptionUpdate);
        break;
      }

      case 'TransactionUpdateLight': {
        const dbUpdate = message.value.update;
        const tableUpdates = await parseDatabaseUpdate(dbUpdate);
        const subscriptionUpdate: Message = {
          tag: 'TransactionUpdateLight',
          tableUpdates,
        };
        callback(subscriptionUpdate);
        break;
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
        const transactionUpdate: Message = {
          tag: 'TransactionUpdate',
          tableUpdates,
          identity,
          connectionId,
          originalReducerName,
          reducerName,
          args,
          status: txUpdate.status,
          energyConsumed: energyQuantaUsed.quanta,
          message: errMessage,
          timestamp: txUpdate.timestamp,
        };
        callback(transactionUpdate);
        break;
      }

      case 'IdentityToken': {
        const identityTokenMessage: IdentityTokenMessage = {
          tag: 'IdentityToken',
          identity: message.value.identity,
          token: message.value.token,
          connectionId: message.value.connectionId,
        };
        callback(identityTokenMessage);
        break;
      }

      case 'OneOffQueryResponse': {
        throw new Error(
          `TypeScript SDK never sends one-off queries, but got OneOffQueryResponse ${message}`
        );
      }
    }
  }

  async processMessage(
    data: Uint8Array,
    callback: (message: Message) => void
  ): Promise<void> {
    const message = parseValue(ws.ServerMessage, data);
    await this.#processParsedMessage(message, callback);
  }

  /**
   * Subscribe to a set of queries, to be notified when rows which match those queries are altered.
   *
   * NOTE: A new call to `subscribe` will remove all previous subscriptions and replace them with the new `queries`.
   *
   * If any rows matched the previous subscribed queries but do not match the new queries,
   * those rows will be removed from the client cache, and `{Table}.on_delete` callbacks will be invoked for them.
   *
   * @param queries A `SQL` query or list of queries.
   *
   * @example
   *
   * ```ts
   * spacetimeDBClient.subscribe(["SELECT * FROM User","SELECT * FROM Message"]);
   * ```
   */
  // This is marked private but not # because we need to use it from the builder
  private subscribe(
    queryOrQueries: string | string[],
    onApplied?: SubscriptionEventCallback,
    _onError?: ErrorCallback
  ): void {
    this.#onApplied = onApplied;
    const queries =
      typeof queryOrQueries === 'string' ? [queryOrQueries] : queryOrQueries;
    const message = ws.ClientMessage.Subscribe({
      queryStrings: queries,
      // The TypeScript SDK doesn't currently track `request_id`s,
      // so always use 0.
      requestId: 0,
    });
    this.#sendMessage(message);
  }

  #sendMessage(message: ws.ClientMessage) {
    this.wsPromise.then(wsResolved => {
      const writer = new BinaryWriter(1024);
      ws.ClientMessage.serialize(writer, message);
      const encoded = writer.getBuffer();
      wsResolved.send(encoded);
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

  /**s
   * Handles WebSocket onOpen event.
   */
  handleOnOpen(): void {
    this.isActive = true;
  }

  #applyTableUpdates(
    tableUpdates: TableUpdate[],
    eventContext: EventContextInterface
  ): void {
    for (let tableUpdate of tableUpdates) {
      // Get table information for the table being updated
      const tableName = tableUpdate.tableName;
      const tableTypeInfo = this.remoteModule.tables[tableName]!;
      const table = this.clientCache.getOrCreateTable(tableTypeInfo);
      table.applyOperations(tableUpdate.operations, eventContext);
    }
  }

  /**
   * Handles WebSocket onMessage event.
   * @param wsMessage MessageEvent object.
   */
  handleOnMessage(wsMessage: { data: Uint8Array }): void {
    this.#messageQueue = this.#messageQueue.then(() =>
      this.processMessage(wsMessage.data, message => {
        if (message.tag === 'InitialSubscription') {
          let event: Event<never> = { tag: 'SubscribeApplied' };

          const eventContext = this.remoteModule.eventContextConstructor(
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
        } else if (message.tag === 'TransactionUpdateLight') {
          const event: Event<never> = { tag: 'UnknownTransaction' };
          const eventContext = this.remoteModule.eventContextConstructor(
            this,
            event
          );
          this.#applyTableUpdates(message.tableUpdates, eventContext);
        } else if (message.tag === 'TransactionUpdate') {
          const reducerName = message.originalReducerName;
          const reducerTypeInfo = this.remoteModule.reducers[reducerName]!;

          // TODO: Can `reducerName` be '<none>'?
          // See: https://github.com/clockworklabs/SpacetimeDB/blob/a2a1b5d9b2e0ebaaf753d074db056d319952d442/crates/core/src/client/message_handlers.rs#L155
          if (reducerName === '<none>') {
            let errorMessage = message.message;
            console.error(
              `Received an error from the database: ${errorMessage}`
            );
          } else {
            const reader = new BinaryReader(message.args as Uint8Array);
            const reducerArgs = reducerTypeInfo.argsType.deserialize(reader);
            const reducerEvent = {
              callerIdentity: message.identity,
              status: message.status,
              callerConnectionId: message.connectionId as ConnectionId,
              timestamp: message.timestamp,
              energyConsumed: message.energyConsumed,
              reducer: {
                name: reducerName,
                args: reducerArgs,
              },
            };
            const event: Event<typeof reducerEvent.reducer> = {
              tag: 'Reducer',
              value: reducerEvent,
            };
            const eventContext = this.remoteModule.eventContextConstructor(
              this,
              event
            );
            const reducerEventContext = {
              ...eventContext,
              event: reducerEvent,
            };

            this.#applyTableUpdates(message.tableUpdates, eventContext);

            const argsArray: any[] = [];
            reducerTypeInfo.argsType.product.elements.forEach(
              (element, index) => {
                argsArray.push(reducerArgs[element.name]);
              }
            );
            this.#reducerEmitter.emit(
              reducerName,
              reducerEventContext,
              ...argsArray
            );
          }
        } else if (message.tag === 'IdentityToken') {
          this.identity = message.identity;
          if (!this.token && message.token) {
            this.token = message.token;
          }
          this.connectionId = message.connectionId;
          this.#emitter.emit('connect', this, this.identity, this.token);
        }
      })
    );
  }

  on(
    eventName: ConnectionEvent,
    callback: (connection: DBConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.on(eventName, callback);
  }

  off(
    eventName: ConnectionEvent,
    callback: (connection: DBConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.off(eventName, callback);
  }

  onConnect(
    callback: (connection: DBConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.on('connect', callback);
  }

  onDisconnect(
    callback: (connection: DBConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.on('disconnect', callback);
  }

  onConnectError(
    callback: (connection: DBConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.on('connectError', callback);
  }

  removeOnConnect(
    callback: (connection: DBConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.off('connect', callback);
  }

  removeOnDisconnect(
    callback: (connection: DBConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.off('disconnect', callback);
  }

  removeOnConnectError(
    callback: (connection: DBConnectionImpl, ...args: any[]) => void
  ): void {
    this.#emitter.off('connectError', callback);
  }

  onReducer(reducerName: string, callback: ReducerEventCallback): void {
    this.#reducerEmitter.on(reducerName, callback);
  }

  offReducer(reducerName: string, callback: ReducerEventCallback): void {
    this.#reducerEmitter.off(reducerName, callback);
  }
}
