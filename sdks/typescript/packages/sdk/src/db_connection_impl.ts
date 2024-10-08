import decompress from 'brotli/decompress';
import { Address } from './address.ts';
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
import { SubscriptionBuilder, type DBContext } from './db_context.ts';
import type { Event } from './event.ts';
import { type EventContextInterface } from './event_context.ts';
import { EventEmitter } from './event_emitter.ts';
import type { Identity } from './identity.ts';
import type { IdentityTokenMessage, Message } from './message_types.ts';
import type { ReducerEvent } from './reducer_event.ts';
import type SpacetimeModule from './spacetime_module.ts';
import { TableCache, type Operation, type TableUpdate } from './table_cache.ts';
import { deepEqual, toPascalCase } from './utils.ts';
import { WebsocketDecompressAdapter } from './websocket_decompress_adapter.ts';
import type { WebsocketTestAdapter } from './websocket_test_adapter.ts';
import { Buffer } from 'buffer';

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
  SubscriptionBuilder,
  SumType,
  SumTypeVariant,
  TableCache,
  type Event,
  type ReducerArgsAdapter,
  type ValueAdapter,
};

export type { DBContext, EventContextInterface, ReducerEvent };

export type ConnectionEvent = 'connect' | 'disconnect' | 'connectError';

export class DBConnectionImpl<DBView = any, Reducers = any>
  implements DBContext<DBView, Reducers>
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
  #reducerEmitter: EventEmitter = new EventEmitter();
  #onApplied?: (ctx: EventContextInterface) => void;

  wsPromise!: Promise<WebsocketDecompressAdapter | WebsocketTestAdapter>;
  ws?: WebsocketDecompressAdapter | WebsocketTestAdapter;
  db: DBView;
  reducers: Reducers;

  clientAddress: Address = Address.random();

  constructor(remoteModule: SpacetimeModule, emitter: EventEmitter) {
    this.clientCache = new ClientCache();
    this.#emitter = emitter;
    this.remoteModule = remoteModule;
    this.db = this.remoteModule.dbViewConstructor(this);
    this.reducers = this.remoteModule.reducersConstructor(this);
  }

  subscriptionBuilder = (): SubscriptionBuilder => {
    return new SubscriptionBuilder(this);
  };

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

  #processParsedMessage(
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
        const rowId = Buffer.from(buffer).toString('base64');
        rows.push({
          type,
          rowId,
          row,
        });
        offset = reader.offset;
      }
      return rows;
    };
    const parseTableUpdate = (rawTableUpdate: ws.TableUpdate): TableUpdate => {
      const tableName = rawTableUpdate.tableName;
      let operations: Operation[] = [];
      for (const update of rawTableUpdate.updates) {
        let decompressed: ws.QueryUpdate;
        if (update.tag === 'Brotli') {
          const decompressedBuffer = decompress(Buffer.from(update.value));
          decompressed = ws.QueryUpdate.deserialize(
            new BinaryReader(decompressedBuffer)
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
    const parseDatabaseUpdate = (
      dbUpdate: ws.DatabaseUpdate
    ): TableUpdate[] => {
      const tableUpdates: TableUpdate[] = [];
      for (const rawTableUpdate of dbUpdate.tables) {
        tableUpdates.push(parseTableUpdate(rawTableUpdate));
      }
      return tableUpdates;
    };

    switch (message.tag) {
      case 'InitialSubscription': {
        const dbUpdate = message.value.databaseUpdate;
        const tableUpdates = parseDatabaseUpdate(dbUpdate);
        const subscriptionUpdate: Message = {
          tag: 'InitialSubscription',
          tableUpdates,
        };
        callback(subscriptionUpdate);
        break;
      }

      case 'TransactionUpdate': {
        const txUpdate = message.value;
        const identity = txUpdate.callerIdentity;
        const address = Address.nullIfZero(txUpdate.callerAddress);
        const originalReducerName = txUpdate.reducerCall.reducerName;
        const reducerName: string = toPascalCase(originalReducerName);
        const args = txUpdate.reducerCall.args;
        const energyQuantaUsed = txUpdate.energyQuantaUsed;

        let tableUpdates: TableUpdate[];
        let errMessage = '';
        switch (txUpdate.status.tag) {
          case 'Committed':
            tableUpdates = parseDatabaseUpdate(txUpdate.status.value);
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
          address,
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
          address: message.value.address,
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

  processMessage(data: Uint8Array, callback: (message: Message) => void): void {
    const message = parseValue(ws.ServerMessage, data);
    this.#processParsedMessage(message, callback);
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
    onApplied?: (ctx: EventContextInterface) => void,
    _onError?: (ctx: EventContextInterface) => void
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
  callReducer(reducerName: string, argsBuffer: Uint8Array): void {
    const message = ws.ClientMessage.CallReducer({
      reducer: reducerName,
      args: argsBuffer,
      // The TypeScript SDK doesn't currently track `request_id`s,
      // so always use 0.
      requestId: 0,
    });
    this.#sendMessage(message);
  }

  /**
   * Handles WebSocket onOpen event.
   */
  handleOnOpen(): void {
    this.isActive = true;
  }

  /**
   * Handles WebSocket onMessage event.
   * @param wsMessage MessageEvent object.
   */
  handleOnMessage(wsMessage: { data: Uint8Array }): void {
    this.processMessage(wsMessage.data, message => {
      if (message.tag === 'InitialSubscription') {
        let event: Event = { tag: 'SubscribeApplied' };
        const eventContext = this.remoteModule.eventContextConstructor(
          this,
          event
        );
        for (let tableUpdate of message.tableUpdates) {
          // Get table information for the table being updated
          const tableName = tableUpdate.tableName;
          const tableTypeInfo = this.remoteModule.tables[tableName]!;
          const table = this.clientCache.getOrCreateTable(tableTypeInfo);
          table.applyOperations(tableUpdate.operations, eventContext);
        }

        if (this.#emitter) {
          this.#onApplied?.(eventContext);
        }
      } else if (message.tag === 'TransactionUpdate') {
        const reducerName = message.originalReducerName;
        const reducerTypeInfo = this.remoteModule.reducers[reducerName]!;

        if (reducerName == '<none>') {
          let errorMessage = message.message;
          console.error(`Received an error from the database: ${errorMessage}`);
        } else {
          const reader = new BinaryReader(message.args as Uint8Array);
          const reducerArgs = reducerTypeInfo.argsType.deserialize(reader);
          const reducerEvent = {
            callerIdentity: message.identity,
            status: message.status,
            callerAddress: message.address as Address,
            timestamp: message.timestamp,
            energyConsumed: message.energyConsumed,
            reducer: {
              name: reducerName,
              args: reducerArgs,
            },
          };
          const event: Event = { tag: 'Reducer', value: reducerEvent };
          const eventContext = this.remoteModule.eventContextConstructor(
            this,
            event
          );

          for (let tableUpdate of message.tableUpdates) {
            // Get table information for the table being updated
            const tableName = tableUpdate.tableName;
            const tableTypeInfo = this.remoteModule.tables[tableName]!;
            const table = this.clientCache.getOrCreateTable(tableTypeInfo);
            table.applyOperations(tableUpdate.operations, eventContext);
          }

          const argsArray: any[] = [];
          reducerTypeInfo.argsType.product.elements.forEach(
            (element, index) => {
              argsArray.push(reducerArgs[element.name]);
            }
          );
          this.#reducerEmitter.emit(reducerName, eventContext, ...argsArray);
        }
      } else if (message.tag === 'IdentityToken') {
        this.identity = message.identity;
        if (!this.token && message.token) {
          this.token = message.token;
        }
        this.clientAddress = message.address;
        this.#emitter.emit('connect', this, this.identity, this.token);
      }
    });
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

  onReducer<ReducerArgs extends any[] = any[]>(
    reducerName: string,
    callback: (ctx: any, ...args: ReducerArgs) => void
  ): void {
    this.#reducerEmitter.on(reducerName, callback);
  }

  offReducer<ReducerArgs extends any[] = any[]>(
    reducerName: string,
    callback: (ctx: any, ...args: ReducerArgs) => void
  ): void {
    this.#reducerEmitter.off(reducerName, callback);
  }
}
