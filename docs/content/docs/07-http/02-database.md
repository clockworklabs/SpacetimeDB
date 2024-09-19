---
title: `/database` HTTP API
navTitle: `/database`
---

The HTTP endpoints in `/database` allow clients to interact with Spacetime databases in a variety of ways, including retrieving information, creating and deleting databases, invoking reducers and evaluating SQL queries.

## At a glance

| Route                                                                                                               | Description                                                       |
| ------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| [`/database/dns/:name GET`](#databasednsname-get)                                                                   | Look up a database's address by its name.                         |
| [`/database/reverse_dns/:address GET`](#databasereverse_dnsaddress-get)                                             | Look up a database's name by its address.                         |
| [`/database/set_name GET`](#databaseset_name-get)                                                                   | Set a database's name, given its address.                         |
| [`/database/ping GET`](#databaseping-get)                                                                           | No-op. Used to determine whether a client can connect.            |
| [`/database/register_tld GET`](#databaseregister_tld-get)                                                           | Register a top-level domain.                                      |
| [`/database/request_recovery_code GET`](#databaserequest_recovery_code-get)                                         | Request a recovery code to the email associated with an identity. |
| [`/database/confirm_recovery_code GET`](#databaseconfirm_recovery_code-get)                                         | Recover a login token from a recovery code.                       |
| [`/database/publish POST`](#databasepublish-post)                                                                   | Publish a database given its module code.                         |
| [`/database/delete/:address POST`](#databasedeleteaddress-post)                                                     | Delete a database.                                                |
| [`/database/subscribe/:name_or_address GET`](#databasesubscribename_or_address-get)                                 | Begin a [WebSocket connection](/docs/ws).                         |
| [`/database/call/:name_or_address/:reducer POST`](#databasecallname_or_addressreducer-post)                         | Invoke a reducer in a database.                                   |
| [`/database/schema/:name_or_address GET`](#databaseschemaname_or_address-get)                                       | Get the schema for a database.                                    |
| [`/database/schema/:name_or_address/:entity_type/:entity GET`](#databaseschemaname_or_addressentity_typeentity-get) | Get a schema for a particular table or reducer.                   |
| [`/database/info/:name_or_address GET`](#databaseinfoname_or_address-get)                                           | Get a JSON description of a database.                             |
| [`/database/logs/:name_or_address GET`](#databaselogsname_or_address-get)                                           | Retrieve logs from a database.                                    |
| [`/database/sql/:name_or_address POST`](#databasesqlname_or_address-post)                                           | Run a SQL query against a database.                               |

## `/database/dns/:name GET`

Look up a database's address by its name.

Accessible through the CLI as `spacetime dns lookup <name>`.

### Parameters

| Name    | Value                     |
| ------- | ------------------------- |
| `:name` | The name of the database. |

### Returns

If a database with that name exists, returns JSON in the form:

```typescript
{ "Success": {
    "domain": string,
    "address": string
} }
```

If no database with that name exists, returns JSON in the form:

```typescript
{ "Failure": {
    "domain": string
} }
```

## `/database/reverse_dns/:address GET`

Look up a database's name by its address.

Accessible through the CLI as `spacetime dns reverse-lookup <address>`.

### Parameters

| Name       | Value                        |
| ---------- | ---------------------------- |
| `:address` | The address of the database. |

### Returns

Returns JSON in the form:

```typescript
{ "names": array<string> }
```

where `<names>` is a JSON array of strings, each of which is a name which refers to the database.

## `/database/set_name GET`

Set the name associated with a database.

Accessible through the CLI as `spacetime dns set-name <domain> <address>`.

### Query Parameters

| Name           | Value                                                                     |
| -------------- | ------------------------------------------------------------------------- |
| `address`      | The address of the database to be named.                                  |
| `domain`       | The name to register.                                                     |
| `register_tld` | A boolean; whether to register the name as a TLD. Should usually be true. |

### Required Headers

| Name            | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http). |

### Returns

If the name was successfully set, returns JSON in the form:

```typescript
{ "Success": {
    "domain": string,
    "address": string
} }
```

If the top-level domain is not registered, and `register_tld` was not specified, returns JSON in the form:

```typescript
{ "TldNotRegistered": {
    "domain": string
} }
```

If the top-level domain is registered, but the identity provided in the `Authorization` header does not have permission to insert into it, returns JSON in the form:

```typescript
{ "PermissionDenied": {
    "domain": string
} }
```

> Spacetime top-level domains are an upcoming feature, and are not fully implemented in SpacetimeDB 0.6. For now, database names should not contain slashes.

## `/database/ping GET`

Does nothing and returns no data. Clients can send requests to this endpoint to determine whether they are able to connect to SpacetimeDB.

## `/database/register_tld GET`

Register a new Spacetime top-level domain. A TLD is the part of a database name before the first `/`. For example, in the name `tyler/bitcraft`, the TLD is `tyler`. Each top-level domain is owned by at most one identity, and only the owner can publish databases with that TLD.

> Spacetime top-level domains are an upcoming feature, and are not fully implemented in SpacetimeDB 0.6. For now, database names should not contain slashes.

Accessible through the CLI as `spacetime dns register-tld <tld>`.

### Query Parameters

| Name  | Value                                  |
| ----- | -------------------------------------- |
| `tld` | New top-level domain name to register. |

### Required Headers

| Name            | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http). |

### Returns

If the domain is successfully registered, returns JSON in the form:

```typescript
{ "Success": {
    "domain": string
} }
```

If the domain is already registered to the caller, returns JSON in the form:

```typescript
{ "AlreadyRegistered": {
    "domain": string
} }
```

If the domain is already registered to another identity, returns JSON in the form:

```typescript
{ "Unauthorized": {
    "domain": string
} }
```

## `/database/request_recovery_code GET`

Request a recovery code or link via email, in order to recover the token associated with an identity.

Accessible through the CLI as `spacetime identity recover <email> <identity>`.

### Query Parameters

| Name       | Value                                                                                                                                                                                                                                                                                 |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `identity` | The identity whose token should be recovered.                                                                                                                                                                                                                                         |
| `email`    | The email to send the recovery code or link to. This email must be associated with the identity, either during creation via [`/identity`](/docs/http/identity#identity-post) or afterwards via [`/identity/:identity/set-email`](/docs/http/identity#identityidentityset_email-post). |
| `link`     | A boolean; whether to send a clickable link rather than a recovery code.                                                                                                                                                                                                              |

## `/database/confirm_recovery_code GET`

Confirm a recovery code received via email following a [`/database/request_recovery_code GET`](#-database-request_recovery_code-get) request, and retrieve the identity's token.

Accessible through the CLI as `spacetime identity recover <email> <identity>`.

### Query Parameters

| Name       | Value                                         |
| ---------- | --------------------------------------------- |
| `identity` | The identity whose token should be recovered. |
| `email`    | The email which received the recovery code.   |
| `code`     | The recovery code received via email.         |

On success, returns JSON in the form:

```typescript
{
    "identity": string,
    "token": string
}
```

## `/database/publish POST`

Publish a database.

Accessible through the CLI as `spacetime publish`.

### Query Parameters

| Name              | Value                                                                                            |
| ----------------- | ------------------------------------------------------------------------------------------------ |
| `host_type`       | Optional; a SpacetimeDB module host type. Currently, only `"wasmer"` is supported.               |
| `clear`           | A boolean; whether to clear any existing data when updating an existing database.                |
| `name_or_address` | The name of the database to publish or update, or the address of an existing database to update. |
| `register_tld`    | A boolean; whether to register the database's top-level domain.                                  |

### Required Headers

| Name            | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http). |

### Data

A WebAssembly module in the [binary format](https://webassembly.github.io/spec/core/binary/index.html).

### Returns

If the database was successfully published, returns JSON in the form:

```typescript
{ "Success": {
    "domain": null | string,
    "address": string,
    "op": "created" | "updated"
} }
```

If the top-level domain for the requested name is not registered, returns JSON in the form:

```typescript
{ "TldNotRegistered": {
    "domain": string
} }
```

If the top-level domain for the requested name is registered, but the identity provided in the `Authorization` header does not have permission to insert into it, returns JSON in the form:

```typescript
{ "PermissionDenied": {
    "domain": string
} }
```

> Spacetime top-level domains are an upcoming feature, and are not fully implemented in SpacetimeDB 0.6. For now, database names should not contain slashes.

## `/database/delete/:address POST`

Delete a database.

Accessible through the CLI as `spacetime delete <address>`.

### Parameters

| Name       | Address                      |
| ---------- | ---------------------------- |
| `:address` | The address of the database. |

### Required Headers

| Name            | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http). |

## `/database/subscribe/:name_or_address GET`

Begin a [WebSocket connection](/docs/ws) with a database.

### Parameters

| Name               | Value                        |
| ------------------ | ---------------------------- |
| `:name_or_address` | The address of the database. |

### Required Headers

For more information about WebSocket headers, see [RFC 6455](https://datatracker.ietf.org/doc/html/rfc6455).

| Name                     | Value                                                                                                |
| ------------------------ | ---------------------------------------------------------------------------------------------------- |
| `Sec-WebSocket-Protocol` | [`v1.bin.spacetimedb`](/docs/ws#binary-protocol) or [`v1.text.spacetimedb`](/docs/ws#text-protocol). |
| `Connection`             | `Updgrade`                                                                                           |
| `Upgrade`                | `websocket`                                                                                          |
| `Sec-WebSocket-Version`  | `13`                                                                                                 |
| `Sec-WebSocket-Key`      | A 16-byte value, generated randomly by the client, encoded as Base64.                                |

### Optional Headers

| Name            | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http). |

## `/database/call/:name_or_address/:reducer POST`

Invoke a reducer in a database.

### Parameters

| Name               | Value                                |
| ------------------ | ------------------------------------ |
| `:name_or_address` | The name or address of the database. |
| `:reducer`         | The name of the reducer.             |

### Required Headers

| Name            | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http). |

### Data

A JSON array of arguments to the reducer.

## `/database/schema/:name_or_address GET`

Get a schema for a database.

Accessible through the CLI as `spacetime describe <name_or_address>`.

### Parameters

| Name               | Value                                |
| ------------------ | ------------------------------------ |
| `:name_or_address` | The name or address of the database. |

### Query Parameters

| Name     | Value                                                       |
| -------- | ----------------------------------------------------------- |
| `expand` | A boolean; whether to include full schemas for each entity. |

### Returns

Returns a JSON object with two properties, `"entities"` and `"typespace"`. For example, on the default module generated by `spacetime init` with `expand=true`, returns:

```typescript
{
  "entities": {
    "Person": {
      "arity": 1,
      "schema": {
        "elements": [
          {
            "algebraic_type": {
              "Builtin": {
                "String": []
              }
            },
            "name": {
              "some": "name"
            }
          }
        ]
      },
      "type": "table"
    },
    "__init__": {
      "arity": 0,
      "schema": {
        "elements": [],
        "name": "__init__"
      },
      "type": "reducer"
    },
    "add": {
      "arity": 1,
      "schema": {
        "elements": [
          {
            "algebraic_type": {
              "Builtin": {
                "String": []
              }
            },
            "name": {
              "some": "name"
            }
          }
        ],
        "name": "add"
      },
      "type": "reducer"
    },
    "say_hello": {
      "arity": 0,
      "schema": {
        "elements": [],
        "name": "say_hello"
      },
      "type": "reducer"
    }
  },
  "typespace": [
    {
      "Product": {
        "elements": [
          {
            "algebraic_type": {
              "Builtin": {
                "String": []
              }
            },
            "name": {
              "some": "name"
            }
          }
        ]
      }
    }
  ]
}
```

The `"entities"` will be an object whose keys are table and reducer names, and whose values are objects of the form:

```typescript
{
    "arity": number,
    "type": "table" | "reducer",
    "schema"?: ProductType
}
```

| Entity field | Value                                                                                                                                                       |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `arity`      | For tables, the number of colums; for reducers, the number of arguments.                                                                                    |
| `type`       | For tables, `"table"`; for reducers, `"reducer"`.                                                                                                           |
| `schema`     | A [JSON-encoded `ProductType`](/docs/satn); for tables, the table schema; for reducers, the argument schema. Only present if `expand` is supplied and true. |

The `"typespace"` will be a JSON array of [`AlgebraicType`s](/docs/satn) referenced by the module. This can be used to resolve `Ref` types within the schema; the type `{ "Ref": n }` refers to `response["typespace"][n]`.

## `/database/schema/:name_or_address/:entity_type/:entity GET`

Get a schema for a particular table or reducer in a database.

Accessible through the CLI as `spacetime describe <name_or_address> <entity_type> <entity_name>`.

### Parameters

| Name               | Value                                                            |
| ------------------ | ---------------------------------------------------------------- |
| `:name_or_address` | The name or address of the database.                             |
| `:entity_type`     | `reducer` to describe a reducer, or `table` to describe a table. |
| `:entity`          | The name of the reducer or table.                                |

### Query Parameters

| Name     | Value                                                         |
| -------- | ------------------------------------------------------------- |
| `expand` | A boolean; whether to include the full schema for the entity. |

### Returns

Returns a single entity in the same format as in the `"entities"` returned by [the `/database/schema/:name_or_address GET` endpoint](#databaseschemaname_or_address-get):

```typescript
{
    "arity": number,
    "type": "table" | "reducer",
    "schema"?: ProductType,
}
```

| Field    | Value                                                                                                                                                       |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `arity`  | For tables, the number of colums; for reducers, the number of arguments.                                                                                    |
| `type`   | For tables, `"table"`; for reducers, `"reducer"`.                                                                                                           |
| `schema` | A [JSON-encoded `ProductType`](/docs/satn); for tables, the table schema; for reducers, the argument schema. Only present if `expand` is supplied and true. |

## `/database/info/:name_or_address GET`

Get a database's address, owner identity, host type, number of replicas and a hash of its WASM module.

### Parameters

| Name               | Value                                |
| ------------------ | ------------------------------------ |
| `:name_or_address` | The name or address of the database. |

### Returns

Returns JSON in the form:

```typescript
{
    "address": string,
    "owner_identity": string,
    "host_type": "wasm",
    "initial_program": string
}
```

| Field               | Type   | Meaning                                                          |
| ------------------- | ------ | ---------------------------------------------------------------- |
| `"address"`         | String | The address of the database.                                     |
| `"owner_identity"`  | String | The Spacetime identity of the database's owner.                  |
| `"host_type"`       | String | The module host type; currently always `"wasm"`.                 |
| `"initial_program"` | String | Hash of the WASM module with which the database was initialized. |

## `/database/logs/:name_or_address GET`

Retrieve logs from a database.

Accessible through the CLI as `spacetime logs <name_or_address>`.

### Parameters

| Name               | Value                                |
| ------------------ | ------------------------------------ |
| `:name_or_address` | The name or address of the database. |

### Query Parameters

| Name        | Value                                                           |
| ----------- | --------------------------------------------------------------- |
| `num_lines` | Number of most-recent log lines to retrieve.                    |
| `follow`    | A boolean; whether to continue receiving new logs via a stream. |

### Required Headers

| Name            | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http). |

### Returns

Text, or streaming text if `follow` is supplied, containing log lines.

## `/database/sql/:name_or_address POST`

Run a SQL query against a database.

Accessible through the CLI as `spacetime sql <name_or_address> <query>`.

### Parameters

| Name               | Value                                         |
| ------------------ | --------------------------------------------- |
| `:name_or_address` | The name or address of the database to query. |

### Required Headers

| Name            | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http). |

### Data

SQL queries, separated by `;`.

### Returns

Returns a JSON array of statement results, each of which takes the form:

```typescript
{
    "schema": ProductType,
    "rows": array
}
```

The `schema` will be a [JSON-encoded `ProductType`](/docs/satn) describing the type of the returned rows.

The `rows` will be an array of [JSON-encoded `ProductValue`s](/docs/satn), each of which conforms to the `schema`.
