---
title: SpacetimeDB Typescript Client SDK
navTitle: Typescript
---

The SpacetimeDB client SDK for TypeScript contains all the tools you need to build clients for SpacetimeDB modules using Typescript, either in the browser or with NodeJS.

> You need a database created before use the client, so make sure to follow the Rust or C# Module Quickstart guides if need one.

## Install the SDK

First, create a new client project, and add the following to your `tsconfig.json` file:

```json
{
  "compilerOptions": {
    //You can use any target higher than this one
    //https://www.typescriptlang.org/tsconfig#target
    "target": "es2015"
  }
}
```

Then add the SpacetimeDB SDK to your dependencies:

```bash
cd client
npm install @clockworklabs/spacetimedb-sdk
```

You should have this folder layout starting from the root of your project:

```bash
quickstart-chat
├── client
│   ├── node_modules
│   ├── public
│   └── src
└── server
    └── src
```

### Tip for utilities/scripts

If want to create a quick script to test your module bindings from the command line, you can use https://www.npmjs.com/package/tsx to execute TypeScript files.

Then you create a `script.ts` file and add the imports, code and execute with:

```bash
npx tsx src/script.ts
```

## Generate module bindings

Each SpacetimeDB client depends on some bindings specific to your module. Create a `module_bindings` directory in your project's `src` directory and generate the Typescript interface files using the Spacetime CLI. From your project directory, run:

```bash
mkdir -p client/src/module_bindings
spacetime generate --lang typescript \
    --out-dir client/src/module_bindings \
    --project-path server
```

And now you will get the files for the `reducers` & `tables`:

```bash
quickstart-chat
├── client
│   ├── node_modules
│   ├── public
│   └── src
|       └── module_bindings
|           ├── add_reducer.ts
|           ├── person.ts
|           └── say_hello_reducer.ts
└── server
    └── src
```

Import the `module_bindings` in your client's _main_ file:

```typescript
import { SpacetimeDBClient, Identity } from '@clockworklabs/spacetimedb-sdk';

import Person from './module_bindings/person';
import AddReducer from './module_bindings/add_reducer';
import SayHelloReducer from './module_bindings/say_hello_reducer';
console.log(Person, AddReducer, SayHelloReducer);
```

> There is a known issue where if you do not use every type in your file, it will not pull them into the published build. To fix this, we are using `console.log` to force them to get pulled in.

## API at a glance

### Classes

| Class                                                           | Description                                                                  |
| --------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| [`SpacetimeDBClient`](#api-at-a-glance-class-spacetimedbclient) | The database client connection to a SpacetimeDB server.                      |
| [`Identity`](#events-class-identity)                            | The user's public identity.                                                  |
| [`Address`](#methods-class-address)                             | An opaque identifier for differentiating connections by the same `Identity`. |
| [`{Table}`](#methods-class-table)                               | `{Table}` is a placeholder for each of the generated tables.                 |
| [`{Reducer}`](#methods-class-reducer)                           | `{Reducer}` is a placeholder for each of the generated reducers.             |

### Class `SpacetimeDBClient`

The database client connection to a SpacetimeDB server.

Defined in [spacetimedb-sdk.spacetimedb](https://github.com/clockworklabs/spacetimedb-typescript-sdk/blob/main/src/spacetimedb.ts):

| Constructors                                                                   | Description                                                              |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------------------ |
| [`SpacetimeDBClient.constructor`](#constructors-spacetimedbclient-constructor) | Creates a new `SpacetimeDBClient` database client.                       |
| Properties                                                                     |
| [`SpacetimeDBClient.identity`](#properties-spacetimedbclient-identity)         | The user's public identity.                                              |
| [`SpacetimeDBClient.live`](#properties-spacetimedbclient-live)                 | Whether the client is connected.                                         |
| [`SpacetimeDBClient.token`](#properties-spacetimedbclient-token)               | The user's private authentication token.                                 |
| Methods                                                                        |                                                                          |
| [`SpacetimeDBClient.connect`](#properties-spacetimedbclient-connect)           | Connect to a SpacetimeDB module.                                         |
| [`SpacetimeDBClient.disconnect`](#properties-spacetimedbclient-disconnect)     | Close the current connection.                                            |
| [`SpacetimeDBClient.subscribe`](#properties-spacetimedbclient-subscribe)       | Subscribe to a set of queries.                                           |
| Events                                                                         |                                                                          |
| [`SpacetimeDBClient.onConnect`](#events-spacetimedbclient-onconnect)           | Register a callback to be invoked upon authentication with the database. |
| [`SpacetimeDBClient.onError`](#events-spacetimedbclient-onerror)               | Register a callback to be invoked upon a error.                          |

## Constructors

### `SpacetimeDBClient` constructor

Creates a new `SpacetimeDBClient` database client and set the initial parameters.

```ts
new SpacetimeDBClient(host: string, name_or_address: string, auth_token?: string, protocol?: "binary" | "json")
```

#### Parameters

| Name              | Type                   | Description                                                                                                                                       |
| :---------------- | :--------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------ |
| `host`            | `string`               | The host of the SpacetimeDB server.                                                                                                               |
| `name_or_address` | `string`               | The name or address of the SpacetimeDB module.                                                                                                    |
| `auth_token?`     | `string`               | The credentials to use to connect to authenticate with SpacetimeDB.                                                                               |
| `protocol?`       | `"binary"` \| `"json"` | Define how encode the messages: `"binary"` \| `"json"`. Binary is more efficient and compact, but JSON provides human-readable debug information. |

#### Example

```ts
const host = 'ws://localhost:3000';
const name_or_address = 'database_name';
const auth_token = undefined;
const protocol = 'binary';

var spacetimeDBClient = new SpacetimeDBClient(
  host,
  name_or_address,
  auth_token,
  protocol
);
```

## Class methods

### `SpacetimeDBClient.registerReducers`

Registers reducer classes for use with a SpacetimeDBClient

```ts
registerReducers(...reducerClasses: ReducerClass[])
```

#### Parameters

| Name             | Type           | Description                   |
| :--------------- | :------------- | :---------------------------- |
| `reducerClasses` | `ReducerClass` | A list of classes to register |

#### Example

```ts
import SayHelloReducer from './types/say_hello_reducer';
import AddReducer from './types/add_reducer';

SpacetimeDBClient.registerReducers(SayHelloReducer, AddReducer);
```

---

### `SpacetimeDBClient.registerTables`

Registers table classes for use with a SpacetimeDBClient

```ts
registerTables(...reducerClasses: TableClass[])
```

#### Parameters

| Name           | Type         | Description                   |
| :------------- | :----------- | :---------------------------- |
| `tableClasses` | `TableClass` | A list of classes to register |

#### Example

```ts
import User from './types/user';
import Player from './types/player';

SpacetimeDBClient.registerTables(User, Player);
```

---

## Properties

### `SpacetimeDBClient` identity

The user's public [Identity](#events-class-identity).

```
identity: Identity | undefined
```

---

### `SpacetimeDBClient` live

Whether the client is connected.

```ts
live: boolean;
```

---

### `SpacetimeDBClient` token

The user's private authentication token.

```
token: string | undefined
```

#### Parameters

| Name          | Type         | Description                     |
| :------------ | :----------- | :------------------------------ |
| `reducerName` | `string`     | The name of the reducer to call |
| `serializer`  | `Serializer` | -                               |

---

### `SpacetimeDBClient` connect

Connect to The SpacetimeDB Websocket For Your Module. By default, this will use a secure websocket connection. The parameters are optional, and if not provided, will use the values provided on construction of the client.

```ts
connect(host: string?, name_or_address: string?, auth_token: string?): Promise<void>
```

#### Parameters

| Name               | Type     | Description                                                                                                                                              |
| :----------------- | :------- | :------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `host?`            | `string` | The hostname of the SpacetimeDB server. Defaults to the value passed to the [constructor](#constructors-spacetimedbclient-constructor).                  |
| `name_or_address?` | `string` | The name or address of the SpacetimeDB module. Defaults to the value passed to the [constructor](#constructors-spacetimedbclient-constructor).           |
| `auth_token?`      | `string` | The credentials to use to authenticate with SpacetimeDB. Defaults to the value passed to the [constructor](#constructors-spacetimedbclient-constructor). |

#### Returns

`Promise`<`void`\>

#### Example

```ts
const host = 'ws://localhost:3000';
const name_or_address = 'database_name';
const auth_token = undefined;

var spacetimeDBClient = new SpacetimeDBClient(
  host,
  name_or_address,
  auth_token
);
// Connect with the initial parameters
spacetimeDBClient.connect();
//Set the `auth_token`
spacetimeDBClient.connect(undefined, undefined, NEW_TOKEN);
```

---

### `SpacetimeDBClient` disconnect

Close the current connection.

```ts
disconnect(): void
```

#### Example

```ts
var spacetimeDBClient = new SpacetimeDBClient(
  'ws://localhost:3000',
  'database_name'
);

spacetimeDBClient.disconnect();
```

---

### `SpacetimeDBClient` subscribe

Subscribe to a set of queries, to be notified when rows which match those queries are altered.

> A new call to `subscribe` will remove all previous subscriptions and replace them with the new `queries`.
> If any rows matched the previous subscribed queries but do not match the new queries,
> those rows will be removed from the client cache, and [`{Table}.on_delete`](#methods-table-ondelete) callbacks will be invoked for them.

```ts
subscribe(queryOrQueries: string | string[]): void
```

#### Parameters

| Name             | Type                   | Description                      |
| :--------------- | :--------------------- | :------------------------------- |
| `queryOrQueries` | `string` \| `string`[] | A `SQL` query or list of queries |

#### Example

```ts
spacetimeDBClient.subscribe(['SELECT * FROM User', 'SELECT * FROM Message']);
```

## Events

### `SpacetimeDBClient` onConnect

Register a callback to be invoked upon authentication with the database.

```ts
onConnect(callback: (token: string, identity: Identity) => void): void
```

The callback will be invoked with the public user [Identity](#events-class-identity), private authentication token and connection [`Address`](#methods-class-address) provided by the database. If credentials were supplied to [connect](#properties-spacetimedbclient-connect), those passed to the callback will be equivalent to the ones used to connect. If the initial connection was anonymous, a new set of credentials will be generated by the database to identify this user.

The credentials passed to the callback can be saved and used to authenticate the same user in future connections.

#### Parameters

| Name       | Type                                                                                                                            |
| :--------- | :------------------------------------------------------------------------------------------------------------------------------ |
| `callback` | (`token`: `string`, `identity`: [`Identity`](#events-class-identity), `address`: [`Address`](#methods-class-address)) => `void` |

#### Example

```ts
spacetimeDBClient.onConnect((token, identity, address) => {
  console.log('Connected to SpacetimeDB');
  console.log('Token', token);
  console.log('Identity', identity);
  console.log('Address', address);
});
```

---

### `SpacetimeDBClient` onError

Register a callback to be invoked upon an error.

```ts
onError(callback: (...args: any[]) => void): void
```

#### Parameters

| Name       | Type                           |
| :--------- | :----------------------------- |
| `callback` | (...`args`: `any`[]) => `void` |

#### Example

```ts
spacetimeDBClient.onError((...args: any[]) => {
  console.error('ERROR', args);
});
```

### Class `Identity`

A unique public identifier for a user of a database.

Defined in [spacetimedb-sdk.identity](https://github.com/clockworklabs/spacetimedb-typescript-sdk/blob/main/src/identity.ts):

| Constructors                                                 | Description                                  |
| ------------------------------------------------------------ | -------------------------------------------- |
| [`Identity.constructor`](#constructors-identity-constructor) | Creates a new `Identity`.                    |
| Methods                                                      |                                              |
| [`Identity.isEqual`](#methods-identity-isequal)              | Compare two identities for equality.         |
| [`Identity.toHexString`](#methods-identity-tohexstring)      | Print the identity as a hexadecimal string.  |
| Static methods                                               |                                              |
| [`Identity.fromString`](#methods-identity-fromstring)        | Parse an Identity from a hexadecimal string. |

## Constructors

### `Identity` constructor

```ts
new Identity(data: Uint8Array)
```

#### Parameters

| Name   | Type         |
| :----- | :----------- |
| `data` | `Uint8Array` |

## Methods

### `Identity` isEqual

Compare two identities for equality.

```ts
isEqual(other: Identity): boolean
```

#### Parameters

| Name    | Type                                 |
| :------ | :----------------------------------- |
| `other` | [`Identity`](#events-class-identity) |

#### Returns

`boolean`

---

### `Identity` toHexString

Print an `Identity` as a hexadecimal string.

```ts
toHexString(): string
```

#### Returns

`string`

---

### `Identity` fromString

Static method; parse an Identity from a hexadecimal string.

```ts
Identity.fromString(str: string): Identity
```

#### Parameters

| Name  | Type     |
| :---- | :------- |
| `str` | `string` |

#### Returns

[`Identity`](#events-class-identity)

### Class `Address`

An opaque identifier for a client connection to a database, intended to differentiate between connections from the same [`Identity`](#events-class-identity).

Defined in [spacetimedb-sdk.address](https://github.com/clockworklabs/spacetimedb-typescript-sdk/blob/main/src/address.ts):

| Constructors                                               | Description                                 |
| ---------------------------------------------------------- | ------------------------------------------- |
| [`Address.constructor`](#constructors-address-constructor) | Creates a new `Address`.                    |
| Methods                                                    |                                             |
| [`Address.isEqual`](#methods-address-isequal)              | Compare two identities for equality.        |
| [`Address.toHexString`](#methods-address-tohexstring)      | Print the address as a hexadecimal string.  |
| Static methods                                             |                                             |
| [`Address.fromString`](#methods-address-fromstring)        | Parse an Address from a hexadecimal string. |

## Constructors

### `Address` constructor

```ts
new Address(data: Uint8Array)
```

#### Parameters

| Name   | Type         |
| :----- | :----------- |
| `data` | `Uint8Array` |

## Methods

### `Address` isEqual

Compare two addresses for equality.

```ts
isEqual(other: Address): boolean
```

#### Parameters

| Name    | Type                                |
| :------ | :---------------------------------- |
| `other` | [`Address`](#methods-class-address) |

#### Returns

`boolean`

---

### `Address` toHexString

Print an `Address` as a hexadecimal string.

```ts
toHexString(): string
```

#### Returns

`string`

---

### `Address` fromString

Static method; parse an Address from a hexadecimal string.

```ts
Address.fromString(str: string): Address
```

#### Parameters

| Name  | Type     |
| :---- | :------- |
| `str` | `string` |

#### Returns

[`Address`](#methods-class-address)

### Class `{Table}`

For each table defined by a module, `spacetime generate` generates a `class` in the `module_bindings` folder whose name is that table's name converted to `PascalCase`.

The generated class has a field for each of the table's columns, whose names are the column names converted to `snake_case`.

| Properties                                                 | Description                                                                                                                             |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| [`Table.name`](#properties-table-name)                     | The name of the class.                                                                                                                  |
| [`Table.tableName`](#properties-table-tablename)           | The name of the table in the database.                                                                                                  |
| Methods                                                    |                                                                                                                                         |
| [`Table.all`](#methods-table-all)                          | Return all the subscribed rows in the table.                                                                                            |
| [`Table.filterBy{COLUMN}`](#methods-table-filterby-column) | Autogenerated; return subscribed rows with a given value in a particular column. `{COLUMN}` is a placeholder for a column name.         |
| [`Table.findBy{COLUMN}`](#methods-table-findby-column)     | Autogenerated; return a subscribed row with a given value in a particular unique column. `{COLUMN}` is a placeholder for a column name. |
| Events                                                     |                                                                                                                                         |
| [`Table.onInsert`](#methods-table-oninsert)                | Register an `onInsert` callback for when a subscribed row is newly inserted into the database.                                          |
| [`Table.removeOnInsert`](#methods-table-removeoninsert)    | Unregister a previously-registered [`onInsert`](#methods-table-oninsert) callback.                                                      |
| [`Table.onUpdate`](#methods-table-onupdate)                | Register an `onUpdate` callback for when an existing row is modified.                                                                   |
| [`Table.removeOnUpdate`](#methods-table-removeonupdate)    | Unregister a previously-registered [`onUpdate`](#methods-table-onupdate) callback.                                                      |
| [`Table.onDelete`](#methods-table-ondelete)                | Register an `onDelete` callback for when a subscribed row is removed from the database.                                                 |
| [`Table.removeOnDelete`](#methods-table-removeondelete)    | Unregister a previously-registered [`onDelete`](#methods-table-removeondelete) callback.                                                |

## Properties

### {Table} name

• **name**: `string`

The name of the `Class`.

---

### {Table} tableName

The name of the table in the database.

▪ `Static` **tableName**: `string` = `"Person"`

## Methods

### {Table} all

Return all the subscribed rows in the table.

```ts
{Table}.all(): {Table}[]
```

#### Returns

`{Table}[]`

#### Example

```ts
var spacetimeDBClient = new SpacetimeDBClient(
  'ws://localhost:3000',
  'database_name'
);

spacetimeDBClient.onConnect((token, identity, address) => {
  spacetimeDBClient.subscribe(['SELECT * FROM Person']);

  setTimeout(() => {
    console.log(Person.all()); // Prints all the `Person` rows in the database.
  }, 5000);
});
```

---

### {Table} count

Return the number of subscribed rows in the table, or 0 if there is no active connection.

```ts
{Table}.count(): number
```

#### Returns

`number`

#### Example

```ts
var spacetimeDBClient = new SpacetimeDBClient(
  'ws://localhost:3000',
  'database_name'
);

spacetimeDBClient.onConnect((token, identity, address) => {
  spacetimeDBClient.subscribe(['SELECT * FROM Person']);

  setTimeout(() => {
    console.log(Person.count());
  }, 5000);
});
```

---

### {Table} filterBy{COLUMN}

For each column of a table, `spacetime generate` generates a static method on the `Class` to filter subscribed rows where that column matches a requested value.

These methods are named `filterBy{COLUMN}`, where `{COLUMN}` is the column name converted to `camelCase`.

```ts
{Table}.filterBy{COLUMN}(value): Iterable<{Table}>
```

#### Parameters

| Name    | Type                        |
| :------ | :-------------------------- |
| `value` | The type of the `{COLUMN}`. |

#### Returns

`Iterable<{Table}>`

#### Example

```ts
var spacetimeDBClient = new SpacetimeDBClient(
  'ws://localhost:3000',
  'database_name'
);

spacetimeDBClient.onConnect((token, identity, address) => {
  spacetimeDBClient.subscribe(['SELECT * FROM Person']);

  setTimeout(() => {
    console.log(...Person.filterByName('John')); // prints all the `Person` rows named John.
  }, 5000);
});
```

---

### {Table} findBy{COLUMN}

For each unique column of a table, `spacetime generate` generates a static method on the `Class` to find the subscribed row where that column matches a requested value.

These methods are named `findBy{COLUMN}`, where `{COLUMN}` is the column name converted to `camelCase`.

```ts
{Table}.findBy{COLUMN}(value): {Table} | undefined
```

#### Parameters

| Name    | Type                        |
| :------ | :-------------------------- |
| `value` | The type of the `{COLUMN}`. |

#### Returns

`{Table} | undefined`

#### Example

```ts
var spacetimeDBClient = new SpacetimeDBClient(
  'ws://localhost:3000',
  'database_name'
);

spacetimeDBClient.onConnect((token, identity, address) => {
  spacetimeDBClient.subscribe(['SELECT * FROM Person']);

  setTimeout(() => {
    console.log(Person.findById(0)); // prints a `Person` row with id 0.
  }, 5000);
});
```

---

### {Table} fromValue

Deserialize an `AlgebraicType` into this `{Table}`.

```ts
 {Table}.fromValue(value: AlgebraicValue): {Table}
```

#### Parameters

| Name    | Type             |
| :------ | :--------------- |
| `value` | `AlgebraicValue` |

#### Returns

`{Table}`

---

### {Table} getAlgebraicType

Serialize `this` into an `AlgebraicType`.

#### Example

```ts
{Table}.getAlgebraicType(): AlgebraicType
```

#### Returns

`AlgebraicType`

---

### {Table} onInsert

Register an `onInsert` callback for when a subscribed row is newly inserted into the database.

```ts
{Table}.onInsert(callback: (value: {Table}, reducerEvent: ReducerEvent | undefined) => void): void
```

#### Parameters

| Name       | Type                                                                          | Description                                            |
| :--------- | :---------------------------------------------------------------------------- | :----------------------------------------------------- |
| `callback` | (`value`: `{Table}`, `reducerEvent`: `undefined` \| `ReducerEvent`) => `void` | Callback to run whenever a subscribed row is inserted. |

#### Example

```ts
var spacetimeDBClient = new SpacetimeDBClient(
  'ws://localhost:3000',
  'database_name'
);
spacetimeDBClient.onConnect((token, identity, address) => {
  spacetimeDBClient.subscribe(['SELECT * FROM Person']);
});

Person.onInsert((person, reducerEvent) => {
  if (reducerEvent) {
    console.log('New person inserted by reducer', reducerEvent, person);
  } else {
    console.log('New person received during subscription update', person);
  }
});
```

---

### {Table} removeOnInsert

Unregister a previously-registered [`onInsert`](#methods-table-oninsert) callback.

```ts
{Table}.removeOnInsert(callback: (value: Person, reducerEvent: ReducerEvent | undefined) => void): void
```

#### Parameters

| Name       | Type                                                                          |
| :--------- | :---------------------------------------------------------------------------- |
| `callback` | (`value`: `{Table}`, `reducerEvent`: `undefined` \| `ReducerEvent`) => `void` |

---

### {Table} onUpdate

Register an `onUpdate` callback to run when an existing row is modified by primary key.

```ts
{Table}.onUpdate(callback: (oldValue: {Table}, newValue: {Table}, reducerEvent: ReducerEvent | undefined) => void): void
```

`onUpdate` callbacks are only meaningful for tables with a column declared as a primary key. Tables without primary keys will never fire `onUpdate` callbacks.

#### Parameters

| Name       | Type                                                                                                    | Description                                           |
| :--------- | :------------------------------------------------------------------------------------------------------ | :---------------------------------------------------- |
| `callback` | (`oldValue`: `{Table}`, `newValue`: `{Table}`, `reducerEvent`: `undefined` \| `ReducerEvent`) => `void` | Callback to run whenever a subscribed row is updated. |

#### Example

```ts
var spacetimeDBClient = new SpacetimeDBClient(
  'ws://localhost:3000',
  'database_name'
);
spacetimeDBClient.onConnect((token, identity, address) => {
  spacetimeDBClient.subscribe(['SELECT * FROM Person']);
});

Person.onUpdate((oldPerson, newPerson, reducerEvent) => {
  console.log('Person updated by reducer', reducerEvent, oldPerson, newPerson);
});
```

---

### {Table} removeOnUpdate

Unregister a previously-registered [`onUpdate`](#methods-table-onupdate) callback.

```ts
{Table}.removeOnUpdate(callback: (oldValue: {Table}, newValue: {Table}, reducerEvent: ReducerEvent | undefined) => void): void
```

#### Parameters

| Name       | Type                                                                                                    |
| :--------- | :------------------------------------------------------------------------------------------------------ |
| `callback` | (`oldValue`: `{Table}`, `newValue`: `{Table}`, `reducerEvent`: `undefined` \| `ReducerEvent`) => `void` |

---

### {Table} onDelete

Register an `onDelete` callback for when a subscribed row is removed from the database.

```ts
{Table}.onDelete(callback: (value: {Table}, reducerEvent: ReducerEvent | undefined) => void): void
```

#### Parameters

| Name       | Type                                                                          | Description                                           |
| :--------- | :---------------------------------------------------------------------------- | :---------------------------------------------------- |
| `callback` | (`value`: `{Table}`, `reducerEvent`: `undefined` \| `ReducerEvent`) => `void` | Callback to run whenever a subscribed row is removed. |

#### Example

```ts
var spacetimeDBClient = new SpacetimeDBClient(
  'ws://localhost:3000',
  'database_name'
);
spacetimeDBClient.onConnect((token, identity, address) => {
  spacetimeDBClient.subscribe(['SELECT * FROM Person']);
});

Person.onDelete((person, reducerEvent) => {
  if (reducerEvent) {
    console.log('Person deleted by reducer', reducerEvent, person);
  } else {
    console.log(
      'Person no longer subscribed during subscription update',
      person
    );
  }
});
```

---

### {Table} removeOnDelete

Unregister a previously-registered [`onDelete`](#methods-table-ondelete) callback.

```ts
{Table}.removeOnDelete(callback: (value: {Table}, reducerEvent: ReducerEvent | undefined) => void): void
```

#### Parameters

| Name       | Type                                                                          |
| :--------- | :---------------------------------------------------------------------------- |
| `callback` | (`value`: `{Table}`, `reducerEvent`: `undefined` \| `ReducerEvent`) => `void` |

### Class `{Reducer}`

`spacetime generate` defines an `{Reducer}` class in the `module_bindings` folder for each reducer defined by a module.

The class's name will be the reducer's name converted to `PascalCase`.

| Static methods                                 | Description                                                  |
| ---------------------------------------------- | ------------------------------------------------------------ |
| [`Reducer.call`](#static-methods-reducer-call) | Executes the reducer.                                        |
| Events                                         |                                                              |
| [`Reducer.on`](#events-reducer-on)             | Register a callback to run each time the reducer is invoked. |

## Static methods

### {Reducer} call

Executes the reducer.

```ts
{Reducer}.call(): void
```

#### Example

```ts
SayHelloReducer.call();
```

## Events

### {Reducer} on

Register a callback to run each time the reducer is invoked.

```ts
{Reducer}.on(callback: (reducerEvent: ReducerEvent, ...reducerArgs: any[]) => void): void
```

Clients will only be notified of reducer runs if either of two criteria is met:

- The reducer inserted, deleted or updated at least one row to which the client is subscribed.
- The reducer invocation was requested by this client, and the run failed.

#### Parameters

| Name       | Type                                                           |
| :--------- | :------------------------------------------------------------- |
| `callback` | `(reducerEvent: ReducerEvent, ...reducerArgs: any[]) => void)` |

#### Example

```ts
SayHelloReducer.on((reducerEvent, ...reducerArgs) => {
  console.log('SayHelloReducer called', reducerEvent, reducerArgs);
});
```
