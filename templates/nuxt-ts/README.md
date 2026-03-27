Get a SpacetimeDB Nuxt app running in under 5 minutes.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

Install the [SpacetimeDB CLI](https://spacetimedb.com/install) before continuing.

---

## Create your project

Run the `spacetime dev` command to create a new project with a SpacetimeDB module and Nuxt client.

This will start the local SpacetimeDB server, publish your module, generate TypeScript bindings, and start the Nuxt development server.

```bash
spacetime dev --template nuxt-ts
```



## Open your app

Navigate to [http://localhost:5173](http://localhost:5173) to see your app running.

The template includes a basic Nuxt app connected to SpacetimeDB.



## Explore the project structure

Your project contains both server and client code.

Edit `spacetimedb/src/index.ts` to add tables and reducers. Edit `components/AppContent.vue` to build your UI, and `app.vue` to configure the SpacetimeDB connection.

```
my-spacetime-app/
├── spacetimedb/          # Your SpacetimeDB module
│   └── src/
│       └── index.ts      # SpacetimeDB module logic
├── app.vue               # Root component with provider
├── components/
│   └── AppContent.vue    # Main UI component
├── server/
│   └── api/
│       └── people.get.ts # Server-side data fetching
├── module_bindings/      # Auto-generated types
├── nuxt.config.ts        # Nuxt configuration
└── package.json
```



## Understand tables and reducers

Open `spacetimedb/src/index.ts` to see the module code. The template includes a `person` table and two reducers: `add` to insert a person, and `sayHello` to greet everyone.

Tables store your data. Reducers are functions that modify data — they're the only way to write to the database.

```typescript
import { schema, table, t } from 'spacetimedb/server';

const spacetimedb = schema({
  person: table(
    { public: true },
    {
      name: t.string(),
    }
  ),
});
export default spacetimedb;

export const add = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    ctx.db.person.insert({ name });
  }
);

export const sayHello = spacetimedb.reducer(ctx => {
  for (const person of ctx.db.person.iter()) {
    console.info(`Hello, ${person.name}!`);
  }
  console.info('Hello, World!');
});
```



## Test with the CLI

Open a new terminal and navigate to your project directory. Then use the SpacetimeDB CLI to call reducers and query your data directly.

```bash
cd my-spacetime-app

# Call the add reducer to insert a person
spacetime call add Alice

# Query the person table
spacetime sql "SELECT * FROM person"
 name
---------
 "Alice"

# Call sayHello to greet everyone
spacetime call say_hello

# View the module logs
spacetime logs
2025-01-13T12:00:00.000000Z  INFO: Hello, Alice!
2025-01-13T12:00:00.000000Z  INFO: Hello, World!
```



## Understand server-side rendering

The SpacetimeDB SDK works both server-side and client-side. The template uses a hybrid approach:

- **Server API route** (`server/api/people.get.ts`): Fetches initial data during SSR for fast page loads
- **Client composables**: Maintain a real-time WebSocket connection for live updates

The server API route connects to SpacetimeDB, subscribes, fetches data, and disconnects.

```typescript
// server/api/people.get.ts
import { DbConnection, tables } from '../../module_bindings';

export default defineEventHandler(async () => {
  return new Promise((resolve, reject) => {
    DbConnection.builder()
      .withUri(process.env.SPACETIMEDB_HOST!)
      .withDatabaseName(process.env.SPACETIMEDB_DB_NAME!)
      .onConnect((conn) => {
        conn.subscriptionBuilder()
          .onApplied(() => {
            const people = Array.from(conn.db.person.iter());
            conn.disconnect();
            resolve(people);
          })
          .subscribe(tables.person);
      })
      .build();
  });
});
```



## Set up the SpacetimeDB provider

The root `app.vue` wraps your app in a `SpacetimeDBProvider` that manages the WebSocket connection. The provider is wrapped in `ClientOnly` so it only runs in the browser, while SSR uses the server API route for initial data.

```vue
<!-- app.vue -->
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
import { SpacetimeDBProvider } from 'spacetimedb/vue';
import { DbConnection } from './module_bindings';

const HOST = import.meta.env.VITE_SPACETIMEDB_HOST ?? 'ws://localhost:3000';
const DB_NAME = import.meta.env.VITE_SPACETIMEDB_DB_NAME ?? 'nuxt-ts';
const TOKEN_KEY = `${HOST}/${DB_NAME}/auth_token`;

const connectionBuilder = import.meta.client
  ? DbConnection.builder()
      .withUri(HOST)
      .withDatabaseName(DB_NAME)
      .withToken(localStorage.getItem(TOKEN_KEY) || undefined)
      .onConnect((_conn, identity, token) => {
        localStorage.setItem(TOKEN_KEY, token);
        console.log('Connected:', identity.toHexString());
      })
      .onDisconnect(() => console.log('Disconnected'))
      .onConnectError((_ctx, err) => console.log('Error:', err))
  : undefined;
</script>
```



## Use composables and SSR data together

Use `useFetch` to load initial data server-side, then Vue composables for real-time updates on the client. The component displays server-fetched data immediately while the WebSocket connection establishes.

```vue
<!-- components/AppContent.vue -->
<script setup lang="ts">
import { ref, computed } from 'vue';
import { tables, reducers } from '../module_bindings';

// Fetch initial data server-side for SSR
const { data: initialPeople } = await useFetch('/api/people');

// On the client, use real-time composables
let conn, people, addReducer;
if (import.meta.client) {
  const { useSpacetimeDB, useTable, useReducer } = await import('spacetimedb/vue');
  conn = useSpacetimeDB();
  [people] = useTable(tables.person);
  addReducer = useReducer(reducers.add);
}

// Use real-time data once connected, fall back to SSR data
const displayPeople = computed(() => {
  if (conn?.isActive && people?.value) return people.value;
  return initialPeople.value ?? [];
});
</script>
```

## Next steps

- Read the [TypeScript SDK Reference](https://spacetimedb.com/docs/intro/core-concepts/clients/typescript-reference) for detailed API docs
