import { useEffect, useState } from 'react';
import { DbConnection } from '../module_bindings';
import { SpacetimeDBContext } from '../context';

export function ConnectedGuard({ children }: { children: React.ReactNode }) {
  const [conn, setConn] = useState<DbConnection | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (conn) {
      return;
    }

    const urlOverride = new URLSearchParams(window.location.search).get('url');

    const uri = urlOverride || 'https://tpc-c-benchmark.spacetimedb.com';

    DbConnection.builder()
      .withUri(uri)
      .withDatabaseName('tpcc-metrics')
      .onConnect(conn => {
        console.log('Connected to SpacetimeDB');
        setError(null);
        setConn(conn);
      })
      .onDisconnect(() => {
        console.log('Disconnected from SpacetimeDB');
        setConn(null);
        setError('Disconnected from SpacetimeDB');
      })
      .build();
  }, [conn]);

  if (error) {
    return <div className="heading-7 error">{error}</div>;
  }

  if (!conn || !conn.isActive) {
    return <div className="heading-7">Connecting to SpacetimeDB...</div>;
  }

  return (
    <SpacetimeDBContext.Provider value={conn}>
      {children}
    </SpacetimeDBContext.Provider>
  );
}
