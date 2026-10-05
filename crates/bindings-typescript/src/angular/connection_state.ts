import { InjectionToken, type WritableSignal } from '@angular/core';
import type { ConnectionId } from '../lib/connection_id';
import type { Identity } from '../lib/identity';
import type { DbConnectionBase } from '../sdk/db_connection_impl';

export interface ConnectionState {
  isActive: boolean;
  identity?: Identity;
  token?: string;
  connectionId: ConnectionId;
  connectionError?: Error;
  getConnection<
    DbConnection extends DbConnectionBase<any>,
  >(): DbConnection | null;
}

export const SPACETIMEDB_CONNECTION = new InjectionToken<
  WritableSignal<ConnectionState>
>('SpacetimeDB Connection State');
