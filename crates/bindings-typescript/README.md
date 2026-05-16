## SpacetimeDB Module Library and SDK

### Overview

This repository contains both the SpacetimeDB module library and the TypeScript SDK for SpacetimeDB. The SDK allows you to interact with the database server from a client and applies type information from your SpacetimeDB server module.

### Installation

The SDK is an NPM package, thus you can use your package manager of choice like NPM or Yarn, for example:

```
npm add spacetimedb
```

You can use the package in the browser, using a bundler like vite/parcel/rsbuild, in server-side applications like NodeJS, Deno, Bun, NextJS, Remix, and in Cloudflare Workers.

> NOTE: For usage in NodeJS 18-21, you need to install the `undici` package as a peer dependency: `npm add spacetimedb undici`. Node 22 and later are supported out of the box.

### Usage

In order to connect to a database you have to generate module bindings for your database.

```ts
import { DbConnection, tables } from './module_bindings';

const connection = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('MODULE_NAME')
  .onDisconnect(() => {
    console.log('disconnected');
  })
  .onConnectError(() => {
    console.log('client_error');
  })
  .onConnect((connection, identity, _token) => {
    console.log(
      'Connected to SpacetimeDB with identity:',
      identity.toHexString()
    );

    connection.subscriptionBuilder().subscribe(tables.player);
  })
  .withToken('TOKEN')
  .build();
```

If you need to disconnect the client:

```ts
connection.disconnect();
```

Typically, you will use the SDK with types generated from SpacetimeDB module. For example, given a table named `Player` you can subscribe to player updates like this:

```ts
connection.db.player.onInsert((ctx, player) => {
  console.log(player);
});
```

Given a reducer called `CreatePlayer` you can call it using a call method:

```ts
connection.reducers.createPlayer();
```

#### React Usage

This module also includes React hooks to subscribe to tables under the `spacetimedb/react` subpath. The React integration is fully compatible with React StrictMode and handles the double-mount behavior correctly (only one WebSocket connection is created).

In order to use SpacetimeDB React hooks in your project, first add a `SpacetimeDBProvider` at the top of your component hierarchy:

```tsx
const connectionBuilder = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('MODULE_NAME')
  .withLightMode(true)
  .onDisconnect(() => {
    console.log('disconnected');
  })
  .onConnectError(() => {
    console.log('client_error');
  })
  .onConnect((conn, identity, _token) => {
    console.log(
      'Connected to SpacetimeDB with identity:',
      identity.toHexString()
    );

    conn.subscriptionBuilder().subscribe(tables.player);
  })
  .withToken('TOKEN');

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <App />
    </SpacetimeDBProvider>
  </React.StrictMode>
);
```

One you add a `SpacetimeDBProvider` to your hierarchy, you can use SpacetimeDB React hooks in your render function:

```tsx
function App() {
  const conn = useSpacetimeDB<DbConnection>();
  const { rows: messages } = useTable<DbConnection, Message>('message');

  ...
}
```

#### SolidJS Usage

This module also includes SolidJS primitives to subscribe to tables under the `spacetimedb/solid` subpath. The SolidJS integration uses Solid's fine-grained reactivity system (`createSignal`, `createStore`, `createMemo`, `createComputed`) for optimal rendering performance. Reactive updates are scoped to only the data that actually changed.

In order to use SpacetimeDB SolidJS primitives in your project, first add a `SpacetimeDBProvider` at the top of your component hierarchy:

```tsx
import { SpacetimeDBProvider } from 'spacetimedb/solid';
import { DbConnection, tables } from './module_bindings';

const connectionBuilder = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('MODULE_NAME')
  .withLightMode(true)
  .onDisconnect(() => {
    console.log('disconnected');
  })
  .onConnectError(() => {
    console.log('client_error');
  })
  .onConnect((conn, identity, _token) => {
    console.log(
      'Connected to SpacetimeDB with identity:',
      identity.toHexString()
    );

    conn.subscriptionBuilder().subscribe(tables.player);
  })
  .withToken('TOKEN');

render(() => (
  <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
    <App />
  </SpacetimeDBProvider>
), document.getElementById('root')!);
```

Once you add a `SpacetimeDBProvider` to your hierarchy, you can use the SpacetimeDB SolidJS primitives in your components:

```tsx
import { useSpacetimeDB, useTable, useReducer, useProcedure } from 'spacetimedb/solid';

function App() {
  // Access the connection state (identity, token, connection error, etc.)
  const conn = useSpacetimeDB();

  // Subscribe to a table — returns a reactive store of rows and an isReady accessor
  const [rows, isReady] = useTable(() => tables.message);

  // Subscribe to a filtered view
  const [onlineUsers, onlineReady] = useTable(
    () => tables.user.where(r => r.online.eq(true)),
    {
      onInsert: (row) => console.log('User came online:', row),
      onDelete: (row) => console.log('User went offline:', row),
    }
  );

  // Call a reducer — queues calls made before the connection is ready
  const sendMessage = useReducer(reducers.sendMessage);

  // Call a procedure — queues calls made before the connection is ready
  const getResult = useProcedure(procedures.getSomeResult);

  return (
    <div>
      <Show when={isReady()} fallback={<p>Loading...</p>}>
        <p>{rows.length} messages</p>
        <For each={rows}>
          {(row) => <div>{row.text}</div>}
        </For>
      </Show>
      <button onClick={() => sendMessage('hello')}>Send</button>
    </div>
  );
}
```

**Key differences from the React API:**

- `useTable` takes a _getter function_ `() => Query<TableDef>` instead of a plain value, so the query can be reactive and update when signals change.
- `useTable` returns `[rows, isReady]` where `rows` is a Solid reactive store and `isReady` is an accessor function `() => boolean`.
- The `enabled` callback option is a getter `() => boolean` instead of a plain boolean, allowing it to depend on reactive state.
- `useReducer` and `useProcedure` queue calls made before the connection is ready and flush them once connected.

### Developer notes

To run the tests, do:

```sh
pnpm build && pnpm test
```
