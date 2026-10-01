import {
  defineComponent,
  onMounted,
  onUnmounted,
  provide,
  shallowReactive,
  shallowRef,
  type PropType,
  type Slot,
} from 'vue';
import {
  DbConnectionBuilder,
  type DbConnectionImpl,
} from '../sdk/db_connection_impl';
import { ConnectionId } from '../lib/connection_id';
import { ConnectionManager } from '../sdk/connection_manager';
import {
  SPACETIMEDB_INJECTION_KEY,
  type ConnectionState,
} from './connection_state';

export interface SpacetimeDBProviderProps<
  DbConnection extends DbConnectionImpl<any>,
> {
  connectionBuilder: DbConnectionBuilder<DbConnection>;
}

function setupConnection<DbConnection extends DbConnectionImpl<any>>(
  connectionBuilder: DbConnectionBuilder<DbConnection>
): ConnectionState {
  const key = ConnectionManager.getKey(
    connectionBuilder.getUri(),
    connectionBuilder.getModuleName()
  );
  // Track replacements without proxying SDK instances with private fields.
  const connection = shallowRef<DbConnectionImpl<any> | null>(null);
  const getConnection = <T extends DbConnectionImpl<any>>() =>
    connection.value && ConnectionManager.getConnection<T>(key);
  const reconnect = (builder: DbConnectionBuilder<any>): void => {
    ConnectionManager.rebuild(key, builder);
  };

  const state = shallowReactive<ConnectionState>({
    isActive: false,
    identity: undefined,
    token: undefined,
    connectionId: ConnectionId.random(),
    connectionError: undefined,
    getConnection,
    reconnect,
  });
  provide(SPACETIMEDB_INJECTION_KEY, state);

  const syncState = () => {
    connection.value = ConnectionManager.getConnection(key);
    Object.assign(state, ConnectionManager.getSnapshot(key));
  };
  let unsubscribe: (() => void) | undefined;

  // Keep connection creation client-only, including when used by Nuxt SSR.
  onMounted(() => {
    ConnectionManager.retain(key, connectionBuilder);
    syncState();
    unsubscribe = ConnectionManager.subscribe(key, syncState);
  });

  onUnmounted(() => {
    unsubscribe?.();
    ConnectionManager.release(key);
    connection.value = null;
  });

  return state;
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

/**
 * Provide a reactive connection backed by the shared ConnectionManager.
 * Connections are pooled by URI and database name and reconnect automatically
 * after an unexpected disconnect. Call `state.reconnect(builder)` with a fresh
 * builder to swap auth tokens without reloading the page. Providers sharing the
 * same key observe the replacement and keep it alive until the last unmount.
 */
export function useSpacetimeDBProvider<
  DbConnection extends DbConnectionImpl<any>,
>(connectionBuilder: DbConnectionBuilder<DbConnection>): ConnectionState {
  return setupConnection(connectionBuilder);
}
