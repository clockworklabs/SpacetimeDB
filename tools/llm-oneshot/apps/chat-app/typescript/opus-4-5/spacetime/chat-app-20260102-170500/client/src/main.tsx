import { createRoot } from 'react-dom/client';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { useMemo } from 'react';
import { DbConnection, Identity } from './module_bindings';
import App from './App';
import './index.css';

const SPACETIMEDB_URI = 'ws://localhost:3000';
const MODULE_NAME = 'chat-app';

declare global {
  interface Window {
    __db_conn: DbConnection | null;
    __my_identity: Identity | null;
  }
}
window.__db_conn = null;
window.__my_identity = null;

function Root() {
  const builder = useMemo(() => {
    const onConnect = (conn: DbConnection, identity: Identity, token: string) => {
      window.__db_conn = conn;
      window.__my_identity = identity;
      if (token) {
        localStorage.setItem('auth_token', token);
      }
      conn.subscriptionBuilder().subscribeToAllTables();
    };

    const onConnectError = (_ctx: unknown, err: Error) => {
      console.error('Connection error:', err);
      if (err.message?.includes('Unauthorized') || err.message?.includes('401')) {
        localStorage.removeItem('auth_token');
        window.location.reload();
      }
    };

    return DbConnection.builder()
      .withUri(SPACETIMEDB_URI)
      .withModuleName(MODULE_NAME)
      .withToken(localStorage.getItem('auth_token') || undefined)
      .onConnect(onConnect)
      .onConnectError(onConnectError);
  }, []);

  return (
    <SpacetimeDBProvider connectionBuilder={builder}>
      <App />
    </SpacetimeDBProvider>
  );
}

createRoot(document.getElementById('root')!).render(<Root />);
