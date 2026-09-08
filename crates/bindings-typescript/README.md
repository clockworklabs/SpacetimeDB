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

#### Module function visibility and invocation authentication

Reducer and procedure options accept `visibility: 'public'`, `'private'`, or
`'internal'`. For example, `spacetime.reducer({ visibility: 'internal' }, ctx => {})`
declares an internal reducer. Omission means public for ordinary functions and
private for scheduled functions. An explicit choice is preserved when the
function is scheduled. Lifecycle reducers permit only omission or `'internal'`
and can only run for their host lifecycle event.

Internal functions require verified internal authority. Private functions also
admit the owner, and public functions admit any client. `ctx.senderAuth.isInternal`
captures the host's invocation authority independently of connection and JWT
presence, so an internal call can have a JWT. `ctx.senderAuth.jwt.identity` is the
verified sender supplied by the host. Procedure transactions preserve this
authentication. Newly compiled modules retain schema V10 and advertise
`hosted_auth_v1`. The extended visibility values and capability section require
a compatible host; older V10 definitions retain their existing defaults.

#### Client SDK

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

### Developer notes

To run the tests, do:

```sh
pnpm build && pnpm test
```
