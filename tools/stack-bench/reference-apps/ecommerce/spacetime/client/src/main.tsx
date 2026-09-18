import React, { useEffect, useMemo, useState } from 'react';
import ReactDOM from 'react-dom/client';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection } from './module_bindings';
import { MODULE_NAME, SPACETIMEDB_URI } from './config';
import App from './App';
import { auth, initializeAuth } from './auth';
import './index.css';

function Storefront({ token }: { token: string | undefined }) {
  const connectionBuilder = useMemo(
    () =>
      DbConnection.builder()
        .withUri(SPACETIMEDB_URI)
        .withDatabaseName(MODULE_NAME)
        .withToken(token),
    [token]
  );
  return (
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <App />
    </SpacetimeDBProvider>
  );
}

function Root({ initialToken }: { initialToken: string | undefined }) {
  const [token, setToken] = useState(initialToken);
  useEffect(() => {
    const removeLoaded = auth.events.addUserLoaded(user => setToken(user.expired ? undefined : user.id_token));
    const removeUnloaded = auth.events.addUserUnloaded(() => setToken(undefined));
    return () => { removeLoaded(); removeUnloaded(); };
  }, []);
  // A renewed token must also replace the database connection's credential.
  return <Storefront key={token ?? 'guest'} token={token} />;
}

const root = ReactDOM.createRoot(document.getElementById('root')!);
initializeAuth().then(token => {
  root.render(<React.StrictMode><Root initialToken={token} /></React.StrictMode>);
}).catch(error => {
  root.render(<div role="alert" data-role="auth-error">{error instanceof Error ? error.message : 'Login failed.'}</div>);
});
