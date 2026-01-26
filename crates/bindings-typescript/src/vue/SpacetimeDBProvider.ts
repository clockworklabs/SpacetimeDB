import {
  defineComponent,
  onMounted,
  onUnmounted,
  provide,
  reactive,
  type PropType,
  type Slot,
} from 'vue';
import {
  DbConnectionBuilder,
  type DbConnectionImpl,
  type ErrorContextInterface,
  type RemoteModuleOf,
} from '../sdk/db_connection_impl';
import { ConnectionId } from '../lib/connection_id';
import {
  SPACETIMEDB_INJECTION_KEY,
  type ConnectionState,
} from './connection_state';

export interface SpacetimeDBProviderProps<
  DbConnection extends DbConnectionImpl<any>,
> {
  connectionBuilder: DbConnectionBuilder<DbConnection>;
}

let connRef: DbConnectionImpl<any> | null = null;
let cleanupTimeoutId: ReturnType<typeof setTimeout> | null = null;

function setupConnection<DbConnection extends DbConnectionImpl<any>>(
  connectionBuilder: DbConnectionBuilder<DbConnection>
): {
  state: ConnectionState;
  cleanup: () => void;
} {
  const getConnection = <T extends DbConnectionImpl<any>>() =>
    connRef as T | null;

  const state = reactive<ConnectionState>({
    isActive: false,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
    getConnection,
  });

  provide(SPACETIMEDB_INJECTION_KEY, state);

  let onConnectCallback: ((conn: DbConnection) => void) | null = null;
  let onDisconnectCallback:
    | ((ctx: ErrorContextInterface<RemoteModuleOf<DbConnection>>) => void)
    | null = null;
  let onConnectErrorCallback:
    | ((
        ctx: ErrorContextInterface<RemoteModuleOf<DbConnection>>,
        err: Error
      ) => void)
    | null = null;

  onMounted(() => {
    if (cleanupTimeoutId) {
      clearTimeout(cleanupTimeoutId);
      cleanupTimeoutId = null;
    }

    if (!connRef) {
      connRef = connectionBuilder.build();
    }

    onConnectCallback = (conn: DbConnection) => {
      state.isActive = conn.isActive;
      state.identity = conn.identity;
      state.token = conn.token;
      state.connectionId = conn.connectionId;
      state.connectionError = undefined;
    };

    onDisconnectCallback = (
      ctx: ErrorContextInterface<RemoteModuleOf<DbConnection>>
    ) => {
      state.isActive = ctx.isActive;
    };

    onConnectErrorCallback = (
      ctx: ErrorContextInterface<RemoteModuleOf<DbConnection>>,
      err: Error
    ) => {
      state.isActive = ctx.isActive;
      state.connectionError = err;
    };

    connectionBuilder.onConnect(onConnectCallback);
    connectionBuilder.onDisconnect(onDisconnectCallback);
    connectionBuilder.onConnectError(onConnectErrorCallback);

    const conn = connRef;
    if (conn) {
      state.isActive = conn.isActive;
      state.identity = conn.identity;
      state.token = conn.token;
      state.connectionId = conn.connectionId;
    }
  });

  const cleanup = () => {
    if (connRef) {
      if (onConnectCallback) {
        connRef.removeOnConnect?.(onConnectCallback as any);
      }
      if (onDisconnectCallback) {
        connRef.removeOnDisconnect?.(onDisconnectCallback as any);
      }
      if (onConnectErrorCallback) {
        connRef.removeOnConnectError?.(onConnectErrorCallback as any);
      }

      cleanupTimeoutId = setTimeout(() => {
        connRef?.disconnect();
        connRef = null;
        cleanupTimeoutId = null;
      }, 0);
    }
  };

  onUnmounted(cleanup);

  return { state, cleanup };
}

export const SpacetimeDBProvider = defineComponent({
  name: 'SpacetimeDBProvider',

  props: {
    connectionBuilder: {
      type: Object as PropType<DbConnectionBuilder<any>>,
      required: true,
    },
  },

  setup(props, { slots }) {
    setupConnection(props.connectionBuilder);

    return () => {
      const defaultSlot = slots.default as Slot | undefined;
      return defaultSlot ? defaultSlot() : null;
    };
  },
});

export function useSpacetimeDBProvider<
  DbConnection extends DbConnectionImpl<any>,
>(connectionBuilder: DbConnectionBuilder<DbConnection>): ConnectionState {
  const { state } = setupConnection(connectionBuilder);
  return state;
}
