import { useState } from 'react';
import type { Identity } from 'spacetimedb';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection, type ErrorContext } from '../module_bindings';
import type { PersonData } from '../lib/spacetimedb-server';
import { PersonList } from './PersonList';

interface SpacetimeAppProps {
  initialPeople: PersonData[];
}

const HOST = import.meta.env.PUBLIC_SPACETIMEDB_HOST ?? 'ws://localhost:3000';
const DB_NAME = import.meta.env.PUBLIC_SPACETIMEDB_DB_NAME ?? 'astro-ts';
const TOKEN_KEY = `${HOST}/${DB_NAME}/auth_token`;

function getStoredToken() {
  if (typeof window === 'undefined') {
    return undefined;
  }

  return localStorage.getItem(TOKEN_KEY) ?? undefined;
}

const onConnect = (_conn: DbConnection, identity: Identity, token: string) => {
  if (typeof window !== 'undefined') {
    localStorage.setItem(TOKEN_KEY, token);
  }

  console.info('Connected to SpacetimeDB with identity:', identity.toHexString());
};

const onDisconnect = () => {
  console.info('Disconnected from SpacetimeDB');
};

const onConnectError = (_ctx: ErrorContext, error: Error) => {
  console.error('Error connecting to SpacetimeDB:', error);
};

export function SpacetimeApp({ initialPeople }: SpacetimeAppProps) {
  const [connectionBuilder] = useState(() =>
    DbConnection.builder()
      .withUri(HOST)
      .withDatabaseName(DB_NAME)
      .withToken(getStoredToken())
      .onConnect(onConnect)
      .onDisconnect(onDisconnect)
      .onConnectError(onConnectError)
  );

  return (
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <PersonList initialPeople={initialPeople} />
    </SpacetimeDBProvider>
  );
}
