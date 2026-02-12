<template>
  <ClientOnly>
    <SpacetimeDBProvider :connection-builder="connectionBuilder">
      <AppContent />
    </SpacetimeDBProvider>
    <template #fallback>
      <AppContent />
    </template>
  </ClientOnly>
</template>

<script setup lang="ts">
import { Identity } from 'spacetimedb';
import { SpacetimeDBProvider } from 'spacetimedb/vue';
import { DbConnection, type ErrorContext } from './module_bindings';

const HOST = import.meta.env.VITE_SPACETIMEDB_HOST ?? 'ws://localhost:3000';
const DB_NAME = import.meta.env.VITE_SPACETIMEDB_DB_NAME ?? 'nuxt-ts';

const onConnect = (_conn: DbConnection, identity: Identity, token: string) => {
  localStorage.setItem('auth_token', token);
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

const connectionBuilder = import.meta.client
  ? DbConnection.builder()
      .withUri(HOST)
      .withModuleName(DB_NAME)
      .withToken(localStorage.getItem('auth_token') || undefined)
      .onConnect(onConnect)
      .onDisconnect(onDisconnect)
      .onConnectError(onConnectError)
  : undefined;
</script>
