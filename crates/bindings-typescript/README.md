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
import { DbConnection } from './module_bindings';

const connection = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withModuleName('MODULE_NAME')
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

    connection.subscriptionBuilder().subscribe('SELECT * FROM player');
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

This module also include React hooks to subscribe to tables under the `spacetimedb/react` subpath. In order to use SpacetimeDB React hooks in your project, first add a `SpacetimeDBProvider` at the top of your component hierarchy:

```tsx
const connectionBuilder = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withModuleName('MODULE_NAME')
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

    conn.subscriptionBuilder().subscribe('SELECT * FROM player');
  })
  .withToken(
    'TOKEN'
  );

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

### Developer notes

To run the tests, do:

```sh
pnpm build && pnpm test
```
