import React, { useMemo } from 'react';
import ReactDOM from 'react-dom/client';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import App from './App';
import { DbConnection } from './module_bindings';
import { SPACETIMEDB_URI, MODULE_NAME } from './config';
import './styles.css';

function Root() {
  const connectionBuilder = useMemo(
    () =>
      DbConnection.builder()
        .withUri(SPACETIMEDB_URI)
        .withDatabaseName(MODULE_NAME)
        .withToken(localStorage.getItem('auth_token') || undefined)
        .onConnect((_conn, _identity, token) => {
          localStorage.setItem('auth_token', token);
        }),
    []
  );

  return (
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <App />
    </SpacetimeDBProvider>
  );
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>
);
