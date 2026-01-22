'use client';

import { useMemo } from 'react';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection, ErrorContext } from '../src/module_bindings';
import { Identity } from 'spacetimedb';

const URI = process.env.NEXT_PUBLIC_SPACETIMEDB_URI ?? 'ws://localhost:3000';
const MODULE = process.env.NEXT_PUBLIC_SPACETIMEDB_MODULE ?? 'nextjs-ts';

const onConnect = (_conn: DbConnection, identity: Identity, token: string) => {
  if (typeof window !== 'undefined') {
    localStorage.setItem('auth_token', token);
  }
  console.log(
    'Connected to SpacetimeDB with identity:',
    identity.toHexString()
  );
};

const onDisconnect = () => {
  console.log('Disconnected from SpacetimeDB');
};

const onConnectError = (_ctx: ErrorContext, err: Error) => {
  console.log('Error connecting to SpacetimeDB:', err);
};

export function Providers({ children }: { children: React.ReactNode }) {
  // CRITICAL: Memoize connectionBuilder to prevent reconnects on re-render
  const connectionBuilder = useMemo(
    () =>
      DbConnection.builder()
        .withUri(URI)
        .withModuleName(MODULE)
        .withToken(
          typeof window !== 'undefined'
            ? localStorage.getItem('auth_token') || undefined
            : undefined
        )
        .onConnect(onConnect)
        .onDisconnect(onDisconnect)
        .onConnectError(onConnectError),
    []
  );

  return (
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      {children}
    </SpacetimeDBProvider>
  );
}
