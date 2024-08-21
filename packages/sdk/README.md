## SpacetimeDB SDK

### Overview

This repository contains the TypeScript SDK for SpacetimeDB. The SDK allows to interact with the database server and is prepared to work with code generated from a SpacetimeDB backend code.

### Installation

The SDK is an NPM package, thus you can use your package manager of choice like NPM or Yarn, for example:

```
npm install @clockworklabs/spacetimedb-sdk
```

You can use the package in the browser, using a bundler like webpack of vite, and in terminal applications

> NOTE: For usage in NodeJS 18-21, you need to install the `undici` package as a peer dependency as well: `npm install @clockworklabs/spacetimedb-sdk undici`. Node 22 and later are supported out of the box.

### Usage

In order to connect to a database you have to create a new client:

```ts
import { SpacetimeDBClient } from '@clockworklabs/spacetimedb-sdk';

let client = new SpacetimeDBClient('spacetimedb.com/spacetimedb', '<db-name>');
```

If you would like to connect to the client you can call the below method. This also takes optional parameters to override the host or credentials:

```ts
client.connect();
```

If for some reason you need to disconnect the client:

```ts
client.disconnect();
```

This will connect to a database instance without a specified identity. If you want to persist an identity fetched on connection you can register an `onConnect` callback, which will receive a new assigned identity as an argument:

```ts
client.onConnect((identity: string) => {
  console.log(identity);
  console.log(client.token);
});
```

You may also pass credentials as an optional third argument:

```ts
let credentials = { identity: '<identity>', token: '<token>' };
let client = new SpacetimeDBClient(
  'spacetimedb.com/spacetimedb',
  '<db-name>',
  credentials
);
```

Typically, you will use the SDK with types generated from a backend DB service. For example, given a component named `Player` you can subscribe to player updates by registering the component:

```ts
client.registerComponent(Player, 'Player');
```

Then you will be able to register callbacks on insert and delete events, for example:

```ts
Player.onInsert((newPlayer: Player) => {
  console.log(newPlayer);
});
```

Given a reducer called `CreatePlayer` you can call it using a call method:

```ts
CreatePlayer.call('Nickname');
```
