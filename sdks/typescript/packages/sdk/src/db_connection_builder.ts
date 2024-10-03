import { DBConnectionImpl, type ConnectionEvent } from './db_connection_impl';
import { EventEmitter } from './event_emitter';
import type { Identity } from './identity';
import { stdbLogger } from './logger';
import type SpacetimeModule from './spacetime_module';
import { WebsocketDecompressAdapter } from './websocket_decompress_adapter';

/**
 * The database client connection to a SpacetimeDB server.
 */
export class DBConnectionBuilder<DBConnection> {
  #uri?: URL;
  #nameOrAddress?: string;
  #identity?: Identity;
  #token?: string;
  #emitter: EventEmitter = new EventEmitter();
  #createWSFn: typeof WebsocketDecompressAdapter.createWebSocketFn;

  /**
   * Creates a new `SpacetimeDBClient` database client and set the initial parameters.
   *
   * @param host The host of the SpacetimeDB server.
   * @param name_or_address The name or address of the SpacetimeDB module.
   * @param auth_token The credentials to use to connect to authenticate with SpacetimeDB.
   *
   * @example
   *
   * ```ts
   * const host = "ws://localhost:3000";
   * const name_or_address = "database_name"
   * const auth_token = undefined;
   *
   * var spacetimeDBClient = new SpacetimeDBClient(host, name_or_address, auth_token, protocol);
   * ```
   */
  constructor(
    private spacetimeModule: SpacetimeModule,
    private dbConnectionConstructor: (imp: DBConnectionImpl) => DBConnection
  ) {
    this.#createWSFn = WebsocketDecompressAdapter.createWebSocketFn;
  }

  withUri(uri: string | URL): DBConnectionBuilder<DBConnection> {
    this.#uri = new URL(uri);
    return this;
  }

  withModuleName(nameOrAddress: string): DBConnectionBuilder<DBConnection> {
    this.#nameOrAddress = nameOrAddress;
    return this;
  }

  withCredentials(
    creds: [identity: Identity, token: string]
  ): DBConnectionBuilder<DBConnection> {
    const [identity, token] = creds;
    this.#identity = identity;
    this.#token = token;
    return this;
  }

  withWSFn(
    createWSFn: (args: {
      url: URL;
      wsProtocol: string;
      authToken?: string;
    }) => Promise<WebsocketDecompressAdapter>
  ): DBConnectionBuilder<DBConnection> {
    this.#createWSFn = createWSFn;
    return this;
  }

  /**
   * Connect to The SpacetimeDB Websocket For Your Module. By default, this will use a secure websocket connection. The parameters are optional, and if not provided, will use the values provided on construction of the client.
   *
   * @param host The hostname of the SpacetimeDB server. Defaults to the value passed to the `constructor`.
   * @param nameOrAddress The name or address of the SpacetimeDB module. Defaults to the value passed to the `constructor`.
   * @param authToken The credentials to use to authenticate with SpacetimeDB. Defaults to the value passed to the `constructor`.
   *
   * @example
   *
   * ```ts
   * const host = "ws://localhost:3000";
   * const name_or_address = "database_name"
   * const auth_token = undefined;
   *
   * var spacetimeDBClient = new SpacetimeDBClient(host, name_or_address, auth_token);
   * // Connect with the initial parameters
   * spacetimeDBClient.connect();
   * //Set the `auth_token`
   * spacetimeDBClient.connect(undefined, undefined, NEW_TOKEN);
   * ```
   */
  build(): DBConnection {
    stdbLogger('info', 'Connecting to SpacetimeDB WS...');

    let url = new URL(`database/subscribe/${this.#nameOrAddress}`, this.#uri);

    if (!this.#uri) {
      throw new Error('URI is required to connect to SpacetimeDB');
    }

    if (!/^wss?:/.test(this.#uri.protocol)) {
      url.protocol = 'ws:';
    }

    const connection = new DBConnectionImpl(
      this.spacetimeModule,
      this.#emitter
    );
    connection.identity = this.#identity;
    connection.token = this.#token;

    let clientAddress = connection.clientAddress.toHexString();
    url.searchParams.set('client_address', clientAddress);

    connection.wsPromise = this.#createWSFn({
      url,
      wsProtocol: 'v1.bsatn.spacetimedb',
      authToken: connection.token,
    })
      .then(v => {
        connection.ws = v;

        connection.ws.onclose = () => {
          this.#emitter.emit('disconnect', connection);
        };
        connection.ws.onerror = (e: ErrorEvent) => {
          this.#emitter.emit('connectError', connection, e);
        };
        connection.ws.onopen = connection.handleOnOpen.bind(connection);
        connection.ws.onmessage = connection.handleOnMessage.bind(connection);

        return v;
      })
      .catch(e => {
        stdbLogger('error', 'Error connecting to SpacetimeDB WS');
        connection.on('connectError', e);
        // TODO(cloutiertyler): I don't know but this makes it compile and
        // I don't have time to investigate how to do this properly.
        throw e;
      });

    return this.dbConnectionConstructor(connection);
  }

  /**
   * Register a callback to be invoked upon authentication with the database.
   *
   * @param token The credentials to use to authenticate with SpacetimeDB.
   * @param identity A unique identifier for a client connected to a database.
   *
   * The callback will be invoked with the `Identity` and private authentication `token` provided by the database to identify this connection.
   *
   * If credentials were supplied to connect, those passed to the callback will be equivalent to the ones used to connect.
   *
   * If the initial connection was anonymous, a new set of credentials will be generated by the database to identify this user.
   *
   * The credentials passed to the callback can be saved and used to authenticate the same user in future connections.
   *
   * @example
   *
   * ```ts
   * spacetimeDBClient.onConnect((token, identity) => {
   *  console.log("Connected to SpacetimeDB");
   *  console.log("Token", token);
   *  console.log("Identity", identity);
   * });
   * ```
   */
  onConnect(
    callback: (
      connection: DBConnection,
      identity: Identity,
      token: string
    ) => void
  ): DBConnectionBuilder<DBConnection> {
    this.#emitter.on('connect', callback);
    return this;
  }

  /**
   * Register a callback to be invoked upon an error.
   *
   * @example
   *
   * ```ts
   * spacetimeDBClient.onError((...args: any[]) => {
   *  stdbLogger("warn","ERROR", args);
   * });
   * ```
   */
  onConnectError(
    callback: (connection: DBConnection, error: Error) => void
  ): DBConnectionBuilder<DBConnection> {
    this.#emitter.on('connectError', callback);
    return this;
  }

  /**
   * Registers a callback to run when a {@link DbConnection} whose connection initially succeeded
   * is disconnected, either after a {@link DbConnection.disconnect} call or due to an error.
   *
   * If the connection ended because of an error, the error is passed to the callback.
   *
   * The `callback` will be installed on the `DbConnection` created by `build`
   * before initiating the connection, ensuring there's no opportunity for the disconnect to happen
   * before the callback is installed.
   *
   * Note that this does not trigger if `build` fails
   * or in cases where {@link DbConnectionBuilder.onConnectError} would trigger.
   * This callback only triggers if the connection closes after `build` returns successfully
   * and {@link DbConnectionBuilder.onConnect} is invoked, i.e., after the `IdentityToken` is received.
   *
   * To simplify SDK implementation, at most one such callback can be registered.
   * Calling `onDisconnect` on the same `DbConnectionBuilder` multiple times throws an error.
   *
   * Unlike callbacks registered via {@link DbConnection},
   * no mechanism is provided to unregister the provided callback.
   * This is a concession to ergonomics; there's no clean place to return a `CallbackId` from this method
   * or from `build`.
   *
   * @param {function(error?: Error): void} callback - The callback to invoke upon disconnection.
   * @throws {Error} Throws an error if called multiple times on the same `DbConnectionBuilder`.
   */
  onDisconnect(
    callback: (connection: DBConnection, error?: Error | undefined) => void
  ): DBConnectionBuilder<DBConnection> {
    this.#emitter.on('disconnect', callback);
    return this;
  }
}
