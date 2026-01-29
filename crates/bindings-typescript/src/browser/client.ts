import { Identity } from '../lib/identity';
import { ConnectionId } from '../lib/connection_id';
import { resolveWS } from '../sdk/ws';

interface JsonIdentityToken {
  IdentityToken: {
    identity: string;
    token: string;
    address: string;
  };
}

interface JsonSubscriptionUpdate {
  SubscriptionUpdate: {
    table_updates?: JsonTableUpdate[];
    database_update?: {
      tables: any[];
    };
  };
}

interface JsonTableUpdate {
  table_id: number;
  table_name: string;
  table_row_operations: JsonTableRowOperation[];
}

interface JsonTableRowOperation {
  op: 'insert' | 'delete';
  row: any[];
}

interface JsonTransactionUpdate {
  TransactionUpdate: {
    status: {
      Committed?: {
        tables: any[];
      };
    };
    database_update?: {
      tables: any[];
    };
    subscription_update?: {
      table_updates: JsonTableUpdate[];
    };
  };
}

interface JsonInitialSubscription {
  InitialSubscription: {
    database_update: {
      tables: any[];
    };
  };
}

interface JsonTransactionUpdateLight {
  TransactionUpdateLight: {
    database_update: {
      tables: any[];
    };
  };
}

interface JsonSubscriptionError {
  SubscriptionError: {
    total_host_execution_duration_micros: number;
    request_id?: number;
    table_id?: number;
    error: string;
  };
}

type JsonServerMessage =
  | JsonIdentityToken
  | JsonSubscriptionUpdate
  | JsonTransactionUpdate
  | JsonInitialSubscription
  | JsonTransactionUpdateLight
  | JsonSubscriptionError;

interface ModuleSchema {
  typespace: {
    types: any[];
  };
  tables: Array<{
    name: string;
    product_type_ref: number;
  }>;
  reducers: Array<{
    name: string;
    params?: any;
  }>;
}

export interface ClientOptions {
  token?: string | null;
  onConnect?: (identityHex: string, token: string) => void;
  onDisconnect?: () => void;
  onError?: (error: Error) => void;
}

interface Subscription {
  query: string;
  callback: (rows: any[]) => void;
  rows: Map<string, any>;
}

const TABLE_NAME_REGEX = /FROM\s+(\w+)/i;

export class Client {
  private uri: string;
  private moduleName: string;
  private options: ClientOptions;
  private ws: WebSocket | null = null;
  private _identity: Identity | null = null;
  private _identityHex: string | null = null;
  private _token: string | null = null;
  private _connectionId: ConnectionId | null = null;
  private _isConnected = false;
  private subscriptions = new Map<number, Subscription>();
  private nextQueryId = 1;
  private tableNameToColumns = new Map<string, string[]>();

  static builder(): ClientBuilder {
    return new ClientBuilder();
  }

  constructor(uri: string, moduleName: string, options: ClientOptions = {}) {
    this.uri = uri;
    this.moduleName = moduleName;
    this.options = options;
  }

  get identity(): Identity | null {
    return this._identity;
  }

  get identityHex(): string | null {
    return this._identityHex;
  }

  get token(): string | null {
    return this._token;
  }

  get connectionId(): ConnectionId | null {
    return this._connectionId;
  }

  get isConnected(): boolean {
    return this._isConnected;
  }

  private toHttpUrl(url: string): string {
    return url.replace(/^wss:/, 'https:').replace(/^ws:/, 'http:');
  }

  private toWsUrl(url: string): string {
    return url.replace(/^https:/, 'wss:').replace(/^http:/, 'ws:');
  }

  async fetchSchema(): Promise<ModuleSchema> {
    const baseUrl = this.toHttpUrl(this.uri);
    const response = await fetch(
      `${baseUrl}/v1/database/${this.moduleName}/schema?version=9`
    );
    if (!response.ok) {
      throw new Error(`Failed to fetch schema: ${response.status}`);
    }
    const schema: ModuleSchema = await response.json();

    const types = schema.typespace?.types || [];
    for (const table of schema.tables || []) {
      const typeRef = table.product_type_ref;
      const productType = types[typeRef]?.Product;
      if (productType?.elements) {
        const colNames = productType.elements.map(
          (el: any) => el.name?.some || el.name || ''
        );
        this.tableNameToColumns.set(table.name, colNames);
      }
    }

    return schema;
  }

  async connect(): Promise<void> {
    try {
      await this.fetchSchema();
    } catch (e) {
      console.warn('Could not fetch schema:', e);
    }

    const wsBaseUrl = this.toWsUrl(this.uri);
    const wsUrl = new URL(
      `v1/database/${this.moduleName}/subscribe`,
      wsBaseUrl
    );

    const token = this.options.token ?? this._token;
    if (token) {
      wsUrl.searchParams.set('token', token);
    }

    // use JSON WebSocket protocol
    const WS = await resolveWS();
    this.ws = new WS(wsUrl.toString(), 'v1.json.spacetimedb') as WebSocket;

    this.ws.onopen = () => {
      // connection established
    };

    this.ws.onmessage = event => {
      try {
        const message = JSON.parse(event.data) as JsonServerMessage;
        this.handleMessage(message);
      } catch (e) {
        console.error('Failed to parse message:', e);
      }
    };

    this.ws.onerror = event => {
      console.error('WebSocket error:', event);
      this.options.onError?.(new Error('WebSocket error'));
    };

    this.ws.onclose = () => {
      this._isConnected = false;
      this.options.onDisconnect?.();
    };
  }

  private handleMessage(message: JsonServerMessage): void {
    if ('IdentityToken' in message) {
      const { identity, token, address } = message.IdentityToken;

      const identityStr =
        typeof identity === 'string'
          ? identity
          : (identity as any)?.__identity__ || String(identity);

      this._identityHex = identityStr;

      try {
        this._identity = Identity.fromString(identityStr);
      } catch (e) {
        console.warn('Could not parse identity:', e);
        this._identity = null;
      }
      this._token = token;

      if (address) {
        try {
          const addrStr =
            typeof address === 'string'
              ? address
              : (address as any)?.__connection_id__
                ? String((address as any).__connection_id__)
                : String(address);
          this._connectionId = ConnectionId.fromString(addrStr);
        } catch (e) {
          console.warn('Could not parse address:', e);
        }
      }
      this._isConnected = true;
      if (this._identityHex) {
        this.options.onConnect?.(this._identityHex, token);
      }
    } else if ('SubscriptionUpdate' in message) {
      const subUpdate = message.SubscriptionUpdate;
      const tableUpdates =
        subUpdate.table_updates || subUpdate.database_update?.tables || [];
      this.handleTableUpdates(tableUpdates);
    } else if ('InitialSubscription' in message) {
      const tables = message.InitialSubscription.database_update?.tables || [];
      this.handleInitialSubscription(tables);
    } else if ('TransactionUpdate' in message) {
      const txUpdate = message.TransactionUpdate;

      const tables =
        txUpdate.status?.Committed?.tables ||
        txUpdate.database_update?.tables ||
        txUpdate.subscription_update?.table_updates ||
        [];
      if (tables.length > 0) {
        this.handleTableUpdates(tables);
      }
    } else if ('TransactionUpdateLight' in message) {
      const tables =
        message.TransactionUpdateLight.database_update?.tables || [];
      this.handleInitialSubscription(tables);
    } else if ('SubscriptionError' in message) {
      const err = message.SubscriptionError;
      console.error('Subscription error:', err.error);
      this.options.onError?.(new Error(`Subscription error: ${err.error}`));
    }
  }

  private normalizeRow(row: any, tableName: string): any {
    if (Array.isArray(row)) {
      const colNames = this.tableNameToColumns.get(tableName) || [];
      return this.rowArrayToObject(row, colNames);
    }
    return row;
  }

  // apply JSON protocol updates to a subscription
  private applyJsonUpdates(
    sub: Subscription,
    updates: any[],
    tableName: string
  ): boolean {
    let hasChanges = false;
    for (const update of updates) {
      for (const insertJson of update.inserts || []) {
        try {
          const parsed =
            typeof insertJson === 'string'
              ? JSON.parse(insertJson)
              : insertJson;
          const row = this.normalizeRow(parsed, tableName);
          const rowId = JSON.stringify(row);
          sub.rows.set(rowId, row);
          hasChanges = true;
        } catch (e) {
          console.warn('Failed to parse insert:', e);
        }
      }
      for (const deleteJson of update.deletes || []) {
        try {
          const parsed =
            typeof deleteJson === 'string'
              ? JSON.parse(deleteJson)
              : deleteJson;
          const row = this.normalizeRow(parsed, tableName);
          const rowId = JSON.stringify(row);
          sub.rows.delete(rowId);
          hasChanges = true;
        } catch (e) {
          console.warn('Failed to parse delete:', e);
        }
      }
    }
    return hasChanges;
  }

  private handleInitialSubscription(tables: any[]): void {
    for (const table of tables) {
      const tableName = table.table_name;
      const updates = table.updates || [];

      for (const sub of this.subscriptions.values()) {
        const match = sub.query.match(TABLE_NAME_REGEX);
        if (!match || match[1] !== tableName) continue;

        if (this.applyJsonUpdates(sub, updates, tableName)) {
          sub.callback(Array.from(sub.rows.values()));
        }
      }
    }
  }

  private handleTableUpdates(tableUpdates: any[]): void {
    for (const tu of tableUpdates) {
      const tableName = tu.table_name;

      for (const sub of this.subscriptions.values()) {
        const match = sub.query.match(TABLE_NAME_REGEX);
        if (!match || match[1] !== tableName) continue;

        const colNames = this.tableNameToColumns.get(tableName) || [];
        let hasChanges = false;

        if (tu.table_row_operations) {
          for (const op of tu.table_row_operations) {
            const row = this.rowArrayToObject(op.row, colNames);
            const rowId = JSON.stringify(row);
            if (op.op === 'insert') {
              sub.rows.set(rowId, row);
              hasChanges = true;
            } else if (op.op === 'delete') {
              sub.rows.delete(rowId);
              hasChanges = true;
            }
          }
        }

        if (tu.updates) {
          hasChanges =
            this.applyJsonUpdates(sub, tu.updates, tableName) || hasChanges;
        }

        if (hasChanges) {
          sub.callback(Array.from(sub.rows.values()));
        }
      }
    }
  }

  private rowArrayToObject(
    row: any[],
    colNames: string[]
  ): Record<string, any> {
    const obj: Record<string, any> = {};
    for (let i = 0; i < row.length && i < colNames.length; i++) {
      obj[colNames[i]] = row[i];
    }
    return obj;
  }

  subscribe(query: string, callback: (rows: any[]) => void): () => void {
    if (
      !this._isConnected ||
      !this.ws ||
      this.ws.readyState !== 1 // WebSocket.OPEN
    ) {
      throw new Error('Not connected. Call connect() first.');
    }

    const queryId = this.nextQueryId++;

    this.subscriptions.set(queryId, {
      query,
      callback,
      rows: new Map(),
    });

    const subscribeMsg = {
      Subscribe: {
        query_strings: [query],
        request_id: queryId,
      },
    };
    this.ws.send(JSON.stringify(subscribeMsg));

    return () => {
      this.subscriptions.delete(queryId);
      if (this.ws && this.ws.readyState === 1) {
        const unsubscribeMsg = {
          Unsubscribe: {
            request_id: queryId,
          },
        };
        this.ws.send(JSON.stringify(unsubscribeMsg));
      }
    };
  }

  async call(
    reducerName: string,
    args: Record<string, any> = {}
  ): Promise<void> {
    if (!this._isConnected) {
      throw new Error('Not connected. Call connect() first.');
    }

    // use HTTP endpoint for reducer calls
    const baseUrl = this.toHttpUrl(this.uri);
    const url = `${baseUrl}/v1/database/${this.moduleName}/call/${reducerName}`;

    const argsArray = this.argsObjectToArray(args);

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this._token ? { Authorization: `Bearer ${this._token}` } : {}),
      },
      body: JSON.stringify(argsArray),
    });

    if (!response.ok) {
      const text = await response.text();
      throw new Error(`Reducer call failed: ${response.status} ${text}`);
    }
  }

  private argsObjectToArray(args: Record<string, any>): any[] {
    return Object.values(args);
  }

  disconnect(): void {
    this.subscriptions.clear();
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    this._isConnected = false;
    this._identity = null;
    this._identityHex = null;
    this._connectionId = null;
  }
}

export class ClientBuilder {
  #uri?: string;
  #moduleName?: string;
  #token?: string;
  #onConnect?: (identityHex: string, token: string) => void;
  #onDisconnect?: () => void;
  #onError?: (error: Error) => void;

  // set URI of the SpacetimeDB server to connect to
  withUri(uri: string): this {
    this.#uri = uri;
    return this;
  }

  withModuleName(moduleName: string): this {
    this.#moduleName = moduleName;
    return this;
  }

  withToken(token?: string | null): this {
    this.#token = token ?? undefined;
    return this;
  }

  onConnect(callback: (identityHex: string, token: string) => void): this {
    this.#onConnect = callback;
    return this;
  }

  onDisconnect(callback: () => void): this {
    this.#onDisconnect = callback;
    return this;
  }

  onError(callback: (error: Error) => void): this {
    this.#onError = callback;
    return this;
  }

  build(): Client {
    if (!this.#uri) {
      throw new Error('URI is required to connect to SpacetimeDB');
    }
    if (!this.#moduleName) {
      throw new Error(
        'Database name or address is required to connect to SpacetimeDB'
      );
    }

    return new Client(this.#uri, this.#moduleName, {
      token: this.#token,
      onConnect: this.#onConnect,
      onDisconnect: this.#onDisconnect,
      onError: this.#onError,
    });
  }
}
