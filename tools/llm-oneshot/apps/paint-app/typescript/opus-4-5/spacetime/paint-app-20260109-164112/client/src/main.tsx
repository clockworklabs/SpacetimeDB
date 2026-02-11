import { createRoot } from 'react-dom/client';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection } from './module_bindings';
import { MODULE_NAME, SPACETIMEDB_URI } from './config';
import App from './App';
import './styles.css';
import { useMemo } from 'react';
import { Identity } from 'spacetimedb';

declare global {
  interface Window {
    __db_conn: DbConnection | null;
    __my_identity: Identity | null;
  }
}

window.__db_conn = null;
window.__my_identity = null;

function Root() {
  const builder = useMemo(
    () =>
      DbConnection.builder()
        .withUri(SPACETIMEDB_URI)
        .withModuleName(MODULE_NAME)
        .withToken(localStorage.getItem('auth_token') || undefined)
        .onConnect((conn, identity, token) => {
          console.log('Connected to SpacetimeDB');
          localStorage.setItem('auth_token', token);
          window.__db_conn = conn;
          window.__my_identity = identity;

          conn
            .subscriptionBuilder()
            .subscribe([
              'SELECT * FROM user',
              'SELECT * FROM canvas',
              'SELECT * FROM canvas_member',
              'SELECT * FROM draw_element',
              'SELECT * FROM cursor_position',
              'SELECT * FROM user_selection',
              'SELECT * FROM undo_entry',
              'SELECT * FROM clipboard',
            ]);
        })
        .onDisconnect(() => {
          console.log('Disconnected from SpacetimeDB');
        })
        .onConnectError((_ctx, err) => {
          console.error('Connection error:', err.message);
          if (
            err.message?.includes('Unauthorized') ||
            err.message?.includes('401')
          ) {
            localStorage.removeItem('auth_token');
            window.location.reload();
          }
        }),
    []
  );

  return (
    <SpacetimeDBProvider connectionBuilder={builder}>
      <App />
    </SpacetimeDBProvider>
  );
}

createRoot(document.getElementById('root')!).render(<Root />);
