import ReactDOM from 'react-dom/client'
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection } from './module_bindings/index.ts';
import { CONFIG } from './config.ts';
import App from './App.tsx'
import './index.css'

// Global connection and identity storage
declare global {
  interface Window {
    __db_conn: DbConnection | null;
    __my_identity: string | null;
  }
}

const root = ReactDOM.createRoot(document.getElementById('root')!);

const connectionBuilder = DbConnection.builder()
  .withUri(CONFIG.SPACETIMEDB_URI)
  .withModuleName(CONFIG.MODULE_NAME)
  .withToken(localStorage.getItem('auth_token') || undefined)
  .onConnect((conn, identity, token) => {
    localStorage.setItem('auth_token', token);
    window.__db_conn = conn;
    window.__my_identity = identity.toHexString();
    console.log('Connected to SpacetimeDB');
  })
  .onDisconnect(() => {
    console.log('Disconnected from SpacetimeDB');
  })
  .onConnectError((_ctx, err) => {
    console.error('Connection error:', err.message);
    if (err.message?.includes('Unauthorized') || err.message?.includes('401')) {
      localStorage.removeItem('auth_token');
    }
  });

root.render(
  <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
    <App />
  </SpacetimeDBProvider>
);