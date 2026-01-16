import { useMemo } from 'react';
import { createRoot } from 'react-dom/client';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { Identity } from 'spacetimedb';
import { DbConnection, ErrorContext } from './module_bindings';
import App from './App';
import { MODULE_NAME, SPACETIMEDB_URI } from './config';
import './styles.css';

// Global connection and identity storage
declare global {
  interface Window {
    __db_conn: DbConnection | null;
    __my_identity: Identity | null;
  }
}
window.__db_conn = null;
window.__my_identity = null;

const onConnect = (conn: DbConnection, identity: Identity, token: string) => {
  window.__db_conn = conn;
  window.__my_identity = identity;
  if (token) {
    localStorage.setItem('auth_token', token);
  }
  
  // Subscribe to all tables
  conn.subscriptionBuilder().subscribeToAllTables();
};

const onConnectError = (_ctx: ErrorContext, err: Error) => {
  console.error('Connection error:', err);
  if (err.message?.includes('Unauthorized') || err.message?.includes('401')) {
    localStorage.removeItem('auth_token');
    window.location.reload();
  }
};

function Root() {
  const builder = useMemo(
    () =>
      DbConnection.builder()
        .withUri(SPACETIMEDB_URI)
        .withModuleName(MODULE_NAME)
        .withToken(localStorage.getItem('auth_token') || undefined)
        .onConnect(onConnect)
        .onConnectError(onConnectError),
    []
  );

  return (
    <SpacetimeDBProvider connectionBuilder={builder}>
      <App />
    </SpacetimeDBProvider>
  );
}

const container = document.getElementById('root');
if (container) {
  createRoot(container).render(<Root />);
}
