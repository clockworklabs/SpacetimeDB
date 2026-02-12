import { createRouter as createTanStackRouter } from '@tanstack/react-router';
import { QueryClient } from '@tanstack/react-query';
import { routerWithQueryClient } from '@tanstack/react-router-with-query';
import { Identity } from 'spacetimedb';
import { routeTree } from './routeTree.gen';
import {
  SpacetimeDBQueryClient,
  SpacetimeDBProvider,
} from 'spacetimedb/tanstack';
import { DbConnection, ErrorContext } from './module_bindings';

const HOST = import.meta.env.VITE_SPACETIMEDB_HOST ?? 'ws://localhost:3000';
const DB_NAME = import.meta.env.VITE_SPACETIMEDB_DB_NAME ?? 'tanstack-ts';

const spacetimeDBQueryClient = new SpacetimeDBQueryClient();

const queryClient: QueryClient = new QueryClient({
  defaultOptions: {
    queries: {
      queryFn: spacetimeDBQueryClient.queryFn,
      staleTime: Infinity,
      refetchOnWindowFocus: false,
      refetchOnMount: false,
      refetchOnReconnect: false,
    },
  },
});
spacetimeDBQueryClient.connect(queryClient);

const onConnect = (conn: DbConnection, identity: Identity, token: string) => {
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem('auth_token', token);
  }
  console.log(
    'Connected to SpacetimeDB with identity:',
    identity.toHexString()
  );
  spacetimeDBQueryClient.setConnection(conn);
};

const onDisconnect = () => {
  console.log('Disconnected from SpacetimeDB');
};

const onConnectError = (_ctx: ErrorContext, err: Error) => {
  console.error('Error connecting to SpacetimeDB:', err);
};

const connectionBuilder = DbConnection.builder()
  .withUri(HOST)
  .withModuleName(DB_NAME)
  .withToken(
    typeof localStorage !== 'undefined'
      ? (localStorage.getItem('auth_token') ?? undefined)
      : undefined
  )
  .onConnect(onConnect)
  .onDisconnect(onDisconnect)
  .onConnectError(onConnectError);

export function createRouter() {
  const router = routerWithQueryClient(
    createTanStackRouter({
      routeTree,
      scrollRestoration: true,
      defaultNotFoundComponent: () => (
        <div style={{ padding: '2rem' }}>
          <h1>404</h1>
          <p>Page Not Found</p>
        </div>
      ),
      context: { queryClient },
      Wrap: ({ children }) => (
        <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
          {children}
        </SpacetimeDBProvider>
      ),
    }),
    queryClient
  );

  return router;
}

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createRouter>;
  }
}
