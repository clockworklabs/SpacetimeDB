import React from 'react';
import ReactDOM from 'react-dom/client';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection } from './module_bindings';
import { MODULE_NAME, SPACETIMEDB_URI } from './config';
import App from './App';
import './styles.css';

declare global {
  interface Window {
    __db_conn: DbConnection | null;
    __my_identity: any;
  }
}

const connectionBuilder = DbConnection.builder()
  .withUri(SPACETIMEDB_URI)
  .withModuleName(MODULE_NAME)
  .onConnect((conn, identity) => {
    window.__db_conn = conn;
    window.__my_identity = identity;
  })
  .onConnectError((ctx, err) => {
    console.error('Connection error:', err);
    if (err.message?.includes('Unauthorized') || err.message?.includes('401')) {
      localStorage.removeItem('auth_token');
      window.location.reload();
    }
  });

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.Fragment>
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <App />
    </SpacetimeDBProvider>
  </React.Fragment>
);