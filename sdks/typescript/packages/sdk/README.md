## SpacetimeDB SDK

### Overview

This repository contains the TypeScript SDK for SpacetimeDB. The SDK allows to interact with the database server and is prepared to work with code generated from a SpacetimeDB backend code.

### Installation

The SDK is an NPM package, thus you can use your package manager of choice like NPM or Yarn, for example:

```
npm install --save @clockworklabs/spacetimedb-sdk
```

You can use the package in the browser, using a bundler like vite/parcel/rsbuild, in server-side applications like NodeJS, Deno, Bun and in Cloudflare Workers.

> NOTE: For usage in NodeJS 18-21, you need to install the `undici` package as a peer dependency: `npm install @clockworklabs/spacetimedb-sdk undici`. Node 22 and later are supported out of the box.

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

If for some reason you need to disconnect the client:

```ts
connection.disconnect();
```

Typically, you will use the SDK with types generated from a backend DB service. For example, given a table named `Player` you can subscribe to player updates like this:

```ts
connection.db.player.onInsert((ctx, player) => {
  console.log(player);
});
```

Given a reducer called `CreatePlayer` you can call it using a call method:

```ts
connection.reducers.createPlayer();
```

### Developer notes

To run the tests, do:

```sh
pnpm compile && pnpm test
```
