import {
  makeEnvironmentProviders,
  provideAppInitializer,
  signal,
  type EnvironmentProviders,
} from '@angular/core';
import type {
  DbConnectionBuilder,
  DbConnectionBase,
  ErrorContextBase,
  RemoteModuleDeclOf,
} from '../../sdk/db_connection_impl';
import {
  SPACETIMEDB_CONNECTION,
  type ConnectionState,
} from '../connection_state';
import { ConnectionId } from '../../lib/connection_id';

let connRef: DbConnectionBase<any> | null = null;

export function provideSpacetimeDB<DbConnection extends DbConnectionBase<any>>(
  connectionBuilder: DbConnectionBuilder<DbConnection>
): EnvironmentProviders {
  const state = signal<ConnectionState>({
    isActive: false,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
    getConnection: () => null,
  });

  return makeEnvironmentProviders([
    { provide: SPACETIMEDB_CONNECTION, useValue: state },
    provideAppInitializer(() => {
      if (typeof window === 'undefined') {
        return;
      }

      const getConnection = <T extends DbConnectionBase<any>>() =>
        connRef as T | null;

      if (!connRef) {
        connRef = connectionBuilder.build();
      }

      const onConnect = (conn: DbConnection) => {
        state.set({
          ...state(),
          isActive: conn.isActive,
          identity: conn.identity,
          token: conn.token,
          connectionId: conn.connectionId,
          getConnection,
        });
      };

      const onDisconnect = (
        ctx: ErrorContextBase<RemoteModuleDeclOf<DbConnection>>
      ) => {
        state.set({
          ...state(),
          isActive: ctx.isActive,
        });
      };

      const onConnectError = (
        ctx: ErrorContextBase<RemoteModuleDeclOf<DbConnection>>,
        err: Error
      ) => {
        state.set({
          ...state(),
          isActive: ctx.isActive,
          connectionError: err,
        });
      };

      connectionBuilder.onConnect(onConnect);
      connectionBuilder.onDisconnect(onDisconnect);
      connectionBuilder.onConnectError(onConnectError);

      // sync initial state if already connected
      const conn = connRef;
      if (conn) {
        state.set({
          ...state(),
          isActive: conn.isActive,
          identity: conn.identity,
          token: conn.token,
          connectionId: conn.connectionId,
          getConnection,
        });
      }
    }),
  ]);
}
