---
title: WebSocket API
---

As an extension of the [HTTP API](/doc/http-api-reference), SpacetimeDB offers a WebSocket API. Clients can subscribe to a database via a WebSocket connection to receive streaming updates as the database changes, and send requests to invoke reducers. Messages received from the server over a WebSocket will follow the same total ordering of transactions as are committed to the database.

The SpacetimeDB SDKs comminicate with their corresponding database using the WebSocket API.

## Connecting

To initiate a WebSocket connection, send a `GET` request to the [`/database/subscribe/:name_or_address` endpoint](/docs/http/database#databasesubscribename_or_address-get) with headers appropriate to upgrade to a WebSocket connection as per [RFC 6455](https://datatracker.ietf.org/doc/html/rfc6455).

To re-connect with an existing identity, include its token in a [SpacetimeDB Authorization header](/docs/http). Otherwise, a new identity and token will be generated for the client.

## Protocols

Clients connecting via WebSocket can choose between two protocols, [`v1.bin.spacetimedb`](#binary-protocol) and [`v1.text.spacetimedb`](#text-protocol). Clients should include one of these protocols in the `Sec-WebSocket-Protocol` header of their request.

| `Sec-WebSocket-Protocol` header value | Selected protocol          |
| ------------------------------------- | -------------------------- |
| `v1.bin.spacetimedb`                  | [Binary](#binary-protocol) |
| `v1.text.spacetimedb`                 | [Text](#text-protocol)     |

### Binary Protocol

The SpacetimeDB binary WebSocket protocol, `v1.bin.spacetimedb`, encodes messages using [ProtoBuf 3](https://protobuf.dev), and reducer and row data using [BSATN](/docs/bsatn).

The binary protocol's messages are defined in [`client_api.proto`](https://github.com/clockworklabs/SpacetimeDB/blob/master/crates/client-api-messages/protobuf/client_api.proto).

### Text Protocol

The SpacetimeDB text WebSocket protocol, `v1.text.spacetimedb`, encodes messages, reducer and row data as JSON. Reducer arguments and table rows are JSON-encoded according to the [SATN JSON format](/docs/satn).

## Messages

### Client to server

| Message                         | Description                                                                 |
| ------------------------------- | --------------------------------------------------------------------------- |
| [`FunctionCall`](#functioncall) | Invoke a reducer.                                                           |
| [`Subscribe`](#subscribe)       | Register queries to receive streaming updates for a subset of the database. |

#### `FunctionCall`

Clients send a `FunctionCall` message to request that the database run a reducer. The message includes the reducer's name and a SATS `ProductValue` of arguments.

##### Binary: ProtoBuf definition

```protobuf
message FunctionCall {
    string reducer = 1;
    bytes argBytes = 2;
}
```

| Field      | Value                                                    |
| ---------- | -------------------------------------------------------- |
| `reducer`  | The name of the reducer to invoke.                       |
| `argBytes` | The reducer arguments encoded as a BSATN `ProductValue`. |

##### Text: JSON encoding

```typescript
{
    "call": {
        "fn": string,
        "args": array,
    }
}
```

| Field  | Value                                          |
| ------ | ---------------------------------------------- |
| `fn`   | The name of the reducer to invoke.             |
| `args` | The reducer arguments encoded as a JSON array. |

#### `Subscribe`

Clients send a `Subscribe` message to register SQL queries in order to receive streaming updates.

The client will only receive [`TransactionUpdate`s](#transactionupdate) for rows to which it is subscribed, and for reducer runs which alter at least one subscribed row. As a special exception, the client is always notified when a reducer run it requests via a [`FunctionCall` message](#functioncall) fails.

SpacetimeDB responds to each `Subscribe` message with a [`SubscriptionUpdate` message](#subscriptionupdate) containing all matching rows at the time the subscription is applied.

Each `Subscribe` message establishes a new set of subscriptions, replacing all previous subscriptions. Clients which want to add a query to an existing subscription must send a `Subscribe` message containing all the previous queries in addition to the new query. In this case, the returned [`SubscriptionUpdate`](#subscriptionupdate) will contain all previously-subscribed rows in addition to the newly-subscribed rows.

Each query must be a SQL `SELECT * FROM` statement on a single table with an optional `WHERE` clause. See the [SQL Reference](/docs/sql) for the subset of SQL supported by SpacetimeDB.

##### Binary: ProtoBuf definition

```protobuf
message Subscribe {
    repeated string query_strings = 1;
}
```

| Field           | Value                                                             |
| --------------- | ----------------------------------------------------------------- |
| `query_strings` | A sequence of strings, each of which contains a single SQL query. |

##### Text: JSON encoding

```typescript
{
    "subscribe": {
        "query_strings": array<string>
    }
}
```

| Field           | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `query_strings` | An array of strings, each of which contains a single SQL query. |

### Server to client

| Message                                     | Description                                                                |
| ------------------------------------------- | -------------------------------------------------------------------------- |
| [`IdentityToken`](#identitytoken)           | Sent once upon successful connection with the client's identity and token. |
| [`SubscriptionUpdate`](#subscriptionupdate) | Initial message in response to a [`Subscribe` message](#subscribe).        |
| [`TransactionUpdate`](#transactionupdate)   | Streaming update after a reducer runs containing altered rows.             |

#### `IdentityToken`

Upon establishing a WebSocket connection, the server will send an `IdentityToken` message containing the client's identity and token. If the client included a [SpacetimeDB Authorization header](/docs/http) in their connection request, the `IdentityToken` message will contain the same token used to connect, and its corresponding identity. If the client connected anonymously, SpacetimeDB will generate a new identity and token for the client.

##### Binary: ProtoBuf definition

```protobuf
message IdentityToken {
    bytes identity = 1;
    string token = 2;
}
```

| Field      | Value                                   |
| ---------- | --------------------------------------- |
| `identity` | The client's public Spacetime identity. |
| `token`    | The client's private access token.      |

##### Text: JSON encoding

```typescript
{
    "IdentityToken": {
        "identity": array<number>,
        "token": string
    }
}
```

| Field      | Value                                   |
| ---------- | --------------------------------------- |
| `identity` | The client's public Spacetime identity. |
| `token`    | The client's private access token.      |

#### `SubscriptionUpdate`

In response to a [`Subscribe` message](#subscribe), the database sends a `SubscriptionUpdate` containing all of the matching rows which are resident in the database at the time the `Subscribe` was received.

##### Binary: ProtoBuf definition

```protobuf
message SubscriptionUpdate {
    repeated TableUpdate tableUpdates = 1;
}

message TableUpdate {
    uint32 tableId = 1;
    string tableName = 2;
    repeated TableRowOperation tableRowOperations = 3;
}

message TableRowOperation {
    enum OperationType {
        DELETE = 0;
        INSERT = 1;
    }
    OperationType op = 1;
    bytes row = 3;
}
```

Each `SubscriptionUpdate` contains a `TableUpdate` for each table with subscribed rows. Each `TableUpdate` contains a `TableRowOperation` for each subscribed row. `SubscriptionUpdate`, `TableUpdate` and `TableRowOperation` are also used by the [`TransactionUpdate` message](#transactionupdate) to encode rows altered by a reducer, so `TableRowOperation` includes an `OperationType` which identifies the row alteration as either an insert or a delete. When a client receives a `SubscriptionUpdate` message in response to a [`Subscribe` message](#subscribe), all of the `TableRowOperation`s will have `op` of `INSERT`.

| `TableUpdate` field  | Value                                                                                                         |
| -------------------- | ------------------------------------------------------------------------------------------------------------- |
| `tableId`            | An integer identifier for the table. A table's `tableId` is not stable, so clients should not depend on it.   |
| `tableName`          | The string name of the table. Clients should use this field to identify the table, rather than the `tableId`. |
| `tableRowOperations` | A `TableRowOperation` for each inserted or deleted row.                                                       |

| `TableRowOperation` field | Value                                                                                                                                                                                                      |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `op`                      | `INSERT` for inserted rows during a [`TransactionUpdate`](#transactionupdate) or rows resident upon applying a subscription; `DELETE` for deleted rows during a [`TransactionUpdate`](#transactionupdate). |
| `row`                     | The altered row, encoded as a BSATN `ProductValue`.                                                                                                                                                        |

##### Text: JSON encoding

```typescript
// SubscriptionUpdate:
{
    "SubscriptionUpdate": {
        "table_updates": array<TableUpdate>
    }
}

// TableUpdate:
{
    "table_id": number,
    "table_name": string,
    "table_row_operations": array<TableRowOperation>
}

// TableRowOperation:
{
    "op": "insert" | "delete",
    "row": array
}
```

Each `SubscriptionUpdate` contains a `TableUpdate` for each table with subscribed rows. Each `TableUpdate` contains a `TableRowOperation` for each subscribed row. `SubscriptionUpdate`, `TableUpdate` and `TableRowOperation` are also used by the [`TransactionUpdate` message](#transactionupdate) to encode rows altered by a reducer, so `TableRowOperation` includes an `"op"` field which identifies the row alteration as either an insert or a delete. When a client receives a `SubscriptionUpdate` message in response to a [`Subscribe` message](#subscribe), all of the `TableRowOperation`s will have `"op"` of `"insert"`.

| `TableUpdate` field    | Value                                                                                                          |
| ---------------------- | -------------------------------------------------------------------------------------------------------------- |
| `table_id`             | An integer identifier for the table. A table's `table_id` is not stable, so clients should not depend on it.   |
| `table_name`           | The string name of the table. Clients should use this field to identify the table, rather than the `table_id`. |
| `table_row_operations` | A `TableRowOperation` for each inserted or deleted row.                                                        |

| `TableRowOperation` field | Value                                                                                                                                                                                                          |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `op`                      | `"insert"` for inserted rows during a [`TransactionUpdate`](#transactionupdate) or rows resident upon applying a subscription; `"delete"` for deleted rows during a [`TransactionUpdate`](#transactionupdate). |
| `row`                     | The altered row, encoded as a JSON array.                                                                                                                                                                      |

#### `TransactionUpdate`

Upon a reducer run, a client will receive a `TransactionUpdate` containing information about the reducer which ran and the subscribed rows which it altered. Clients will only receive a `TransactionUpdate` for a reducer invocation if either of two criteria is met:

1. The reducer ran successfully and altered at least one row to which the client subscribes.
2. The reducer was invoked by the client, and either failed or was terminated due to insufficient energy.

Each `TransactionUpdate` contains a [`SubscriptionUpdate`](#subscriptionupdate) with all rows altered by the reducer, including inserts and deletes; and an `Event` with information about the reducer itself, including a [`FunctionCall`](#functioncall) containing the reducer's name and arguments.

##### Binary: ProtoBuf definition

```protobuf
message TransactionUpdate {
    Event event = 1;
    SubscriptionUpdate subscriptionUpdate = 2;
}

message Event {
    enum Status {
        committed = 0;
        failed = 1;
        out_of_energy = 2;
    }
    uint64 timestamp = 1;
    bytes callerIdentity = 2;
    FunctionCall functionCall = 3;
    Status status = 4;
    string message = 5;
    int64 energy_quanta_used = 6;
    uint64 host_execution_duration_micros = 7;
}
```

| Field                | Value                                                                                                                       |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `event`              | An `Event` containing information about the reducer run.                                                                    |
| `subscriptionUpdate` | A [`SubscriptionUpdate`](#subscriptionupdate) containing all the row insertions and deletions committed by the transaction. |

| `Event` field                    | Value                                                                                                                                                                                                          |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `timestamp`                      | The time when the reducer started, as microseconds since the Unix epoch.                                                                                                                                       |
| `callerIdentity`                 | The identity of the client which requested the reducer invocation. For event-driven and scheduled reducers, this is the identity of the database owner.                                                        |
| `functionCall`                   | A [`FunctionCall`](#functioncall) containing the name of the reducer and the arguments passed to it.                                                                                                           |
| `status`                         | `committed` if the reducer ran successfully and its changes were committed to the database; `failed` if the reducer signaled an error; `out_of_energy` if the reducer was canceled due to insufficient energy. |
| `message`                        | The error message with which the reducer failed if `status` is `failed`, or the empty string otherwise.                                                                                                        |
| `energy_quanta_used`             | The amount of energy consumed by running the reducer.                                                                                                                                                          |
| `host_execution_duration_micros` | The duration of the reducer's execution, in microseconds.                                                                                                                                                      |

##### Text: JSON encoding

```typescript
// TransactionUpdate:
{
    "TransactionUpdate": {
        "event": Event,
        "subscription_update": SubscriptionUpdate
    }
}

// Event:
{
    "timestamp": number,
    "status": "committed" | "failed" | "out_of_energy",
    "caller_identity": string,
    "function_call": {
        "reducer": string,
        "args": array,
    },
    "energy_quanta_used": number,
    "message": string
}
```

| Field                 | Value                                                                                                                       |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `event`               | An `Event` containing information about the reducer run.                                                                    |
| `subscription_update` | A [`SubscriptionUpdate`](#subscriptionupdate) containing all the row insertions and deletions committed by the transaction. |

| `Event` field           | Value                                                                                                                                                                                                          |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `timestamp`             | The time when the reducer started, as microseconds since the Unix epoch.                                                                                                                                       |
| `status`                | `committed` if the reducer ran successfully and its changes were committed to the database; `failed` if the reducer signaled an error; `out_of_energy` if the reducer was canceled due to insufficient energy. |
| `caller_identity`       | The identity of the client which requested the reducer invocation. For event-driven and scheduled reducers, this is the identity of the database owner.                                                        |
| `function_call.reducer` | The name of the reducer.                                                                                                                                                                                       |
| `function_call.args`    | The reducer arguments encoded as a JSON array.                                                                                                                                                                 |
| `energy_quanta_used`    | The amount of energy consumed by running the reducer.                                                                                                                                                          |
| `message`               | The error message with which the reducer failed if `status` is `failed`, or the empty string otherwise.                                                                                                        |
