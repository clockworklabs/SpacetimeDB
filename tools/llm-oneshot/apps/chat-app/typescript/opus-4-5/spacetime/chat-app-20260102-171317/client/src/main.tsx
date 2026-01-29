import { createRoot } from 'react-dom/client';
import { useMemo } from 'react';
import { SpacetimeDBProvider, Identity } from 'spacetimedb/react';
import { DbConnection } from './module_bindings';
import App from './App';

const SPACETIMEDB_URI = 'ws://localhost:3000';
const MODULE_NAME = 'chat-app';

// Global storage for connection and identity
declare global {
  interface Window {
    __db_conn: DbConnection | null;
    __my_identity: Identity | null;
  }
}
window.__db_conn = null;
window.__my_identity = null;

function Root() {
  const connectionBuilder = useMemo(() => {
    const onConnect = (
      conn: DbConnection,
      identity: Identity,
      token: string
    ) => {
      console.log('Connected to SpacetimeDB');
      window.__db_conn = conn;
      window.__my_identity = identity;

      if (token) {
        localStorage.setItem('auth_token', token);
      }

      conn.subscriptionBuilder().subscribeToAllTables();
    };

    const onConnectError = (_ctx: unknown, err: Error) => {
      console.error('Connection error:', err);
      if (
        err.message?.includes('Unauthorized') ||
        err.message?.includes('401')
      ) {
        localStorage.removeItem('auth_token');
        window.location.reload();
      }
    };

    const onDisconnect = () => {
      console.log('Disconnected from SpacetimeDB');
      window.__db_conn = null;
    };

    return DbConnection.builder()
      .withUri(SPACETIMEDB_URI)
      .withModuleName(MODULE_NAME)
      .withToken(localStorage.getItem('auth_token') || undefined)
      .onConnect(onConnect)
      .onConnectError(onConnectError)
      .onDisconnect(onDisconnect);
  }, []);

  return (
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <App />
    </SpacetimeDBProvider>
  );
}

createRoot(document.getElementById('root')!).render(<Root />);
