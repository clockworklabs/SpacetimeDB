import React, { useMemo } from 'react';
import ReactDOM from 'react-dom/client';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection } from './module_bindings';
import { SPACETIMEDB_URI, MODULE_NAME } from './config';
import App from './App';
import './styles.css';

function Root() {
  const connectionBuilder = useMemo(
    () =>
      DbConnection.builder()
        .withUri(SPACETIMEDB_URI)
        .withDatabaseName(MODULE_NAME)
        .withToken(localStorage.getItem('auth_token') || undefined),
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
