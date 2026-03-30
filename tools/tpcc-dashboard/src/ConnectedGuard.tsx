import { useEffect, useState } from 'react';
import { DbConnection } from './module_bindings';
import { SpacetimeDBContext } from './context';

export function ConnectedGuard({ children }: { children: React.ReactNode }) {
  const [conn, setConn] = useState<DbConnection | null>(null);
  useEffect(() => {
    if (conn) {
      return;
    }

    DbConnection.builder()
      // .withUri('https://tpc-c-benchmark.spacetimedb.com')
      .withUri('http://localhost:3000')
      .withDatabaseName('tpcc-metrics')
      .onConnect(conn => {
        console.log('Connected to SpacetimeDB');
        setConn(conn);
      })
      .build();
  }, [conn]);

  if (!conn || !conn.isActive) {
    return <div>Connecting to SpacetimeDB...</div>;
  }

  return (
    <SpacetimeDBContext.Provider value={conn}>
      {children}
    </SpacetimeDBContext.Provider>
  );
}
