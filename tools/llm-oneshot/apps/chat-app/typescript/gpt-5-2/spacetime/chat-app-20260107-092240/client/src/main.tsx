import { useCallback, useMemo, useState } from 'react';
import ReactDOM from 'react-dom/client';
import { type Identity } from 'spacetimedb';
import { SpacetimeDBProvider } from 'spacetimedb/react';

import { App } from './App';
import { MODULE_NAME, SPACETIMEDB_URI } from './config';
import { DbConnection } from './module_bindings';

import './styles.css';

declare global {
  interface Window {
    __db_conn: DbConnection | null;
    __my_identity: Identity | null;
  }
}

function Root() {
  const [conn, setConn] = useState<DbConnection | null>(null);
  const [identity, setIdentity] = useState<Identity | null>(null);
  const [connectError, setConnectError] = useState<string | null>(null);

  const onConnect = useCallback((c: DbConnection, id: Identity) => {
    window.__db_conn = c;
    window.__my_identity = id;
    setConn(c);
    setIdentity(id);
    setConnectError(null);

    c.subscriptionBuilder().subscribe([
      'SELECT * FROM user',
      'SELECT * FROM room',
      'SELECT * FROM room_member',
      'SELECT * FROM message',
      'SELECT * FROM message_edit',
      'SELECT * FROM scheduled_message',
      'SELECT * FROM typing_indicator',
      'SELECT * FROM reaction',
      'SELECT * FROM read_receipt',
      'SELECT * FROM room_read_position',
    ]);
  }, []);

  const builder = useMemo(
    () =>
      DbConnection.builder()
        .withUri(SPACETIMEDB_URI)
        .withModuleName(MODULE_NAME)
        .onConnect(onConnect)
        .onConnectError((_ctx, err) => {
          setConnectError(err.message || 'Failed to connect');
        }),
    [onConnect],
  );

  return (
    <SpacetimeDBProvider connectionBuilder={builder}>
      <App conn={conn} identity={identity} connectError={connectError} />
    </SpacetimeDBProvider>
  );
}

ReactDOM.createRoot(document.getElementById('root')!).render(<Root />);

