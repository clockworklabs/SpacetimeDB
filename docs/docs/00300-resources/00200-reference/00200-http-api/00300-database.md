---
slug: /http/database
---

# `/v1/database`

The HTTP endpoints in `/v1/database` allow clients to interact with Spacetime databases in a variety of ways, including retrieving information, creating and deleting databases, invoking reducers and evaluating SQL queries. These APIs are intended primarily for management, debugging and interactive developer use, and have not been optimized for performance to the same extent as the WebSocket API used by [the SpacetimeDB client SDKs](../../../00200-core-concepts/00600-clients.md).

## At a glance

| Route                                                                                              | Description                                       |
| -------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| [`POST /v1/database`](#post-v1database)                                                            | Publish a new database given its module code.     |
| [`PUT /v1/database/:name_or_identity`](#put-v1databasename_or_identity)                            | Publish to a database given its module code.      |
| [`GET /v1/database/:name_or_identity`](#get-v1databasename_or_identity)                            | Get a JSON description of a database.             |
| [`DELETE /v1/database/:name_or_identity`](#delete-v1databasename_or_identity)                      | Delete a database.                                |
| [`GET /v1/database/:name_or_identity/names`](#get-v1databasename_or_identitynames)                 | Get the names this database can be identified by. |
| [`POST /v1/database/:name_or_identity/names`](#post-v1databasename_or_identitynames)               | Add a new name for this database.                 |
| [`PUT /v1/database/:name_or_identity/names`](#put-v1databasename_or_identitynames)                 | Set the list of names for this database.          |
| [`GET /v1/database/:name_or_identity/identity`](#get-v1databasename_or_identityidentity)           | Get the identity of a database.                   |
| [`GET /v1/database/:name_or_identity/subscribe`](#get-v1databasename_or_identitysubscribe)         | Begin a WebSocket connection.                     |
| [`POST /v1/database/:name_or_identity/call/:reducer`](#post-v1databasename_or_identitycallreducer) | Invoke a reducer OR procedure in a database.      |
| [`GET /v1/database/:name_or_identity/schema`](#get-v1databasename_or_identityschema)               | Get the schema for a database.                    |
| [`GET /v1/database/:name_or_identity/environment`](#get-v1databasename_or_identityenvironment)     | Get environment declarations and stored keys.     |
| [`PUT /v1/database/:name_or_identity/environment`](#put-v1databasename_or_identityenvironment)     | Replace a database's environment.                 |
| [`PATCH /v1/database/:name_or_identity/environment`](#patch-v1databasename_or_identityenvironment) | Set or delete environment values.                 |
| [`GET /v1/database/:name_or_identity/logs`](#get-v1databasename_or_identitylogs)                   | Retrieve logs from a database.                    |
| [`POST /v1/database/:name_or_identity/sql`](#post-v1databasename_or_identitysql)                   | Run a SQL query against a database.               |
| [`ANY /v1/database/:name_or_identity/route/{*path}`](#any-v1databasename_or_identityroutepath)     | Access database-defined HTTP APIs.                |

## `POST /v1/database`

Publish a new database with no name.

Accessible through the CLI as `spacetime publish`.

#### Optional Headers

| Name                           | Value                                                                                                                                               |
| ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Authorization`                | A Spacetime token [as Bearer auth](./00100-authorization.md#authorization-headers).                                                                 |
| `spacetime-environment`        | One `KEY=VALUE` environment value. Repeat the header for each value. See [Publishing with environment values](#publishing-with-environment-values). |
| `spacetime-environment-remove` | Stored environment keys to delete, or `*` to replace the whole environment.                                                                         |

If no `Authorization` header is provided, a new anonymous identity will be created and will own the new database. This is generally not what you want.

#### Data

A WebAssembly module in the [binary format](https://webassembly.github.io/spec/core/binary/index.html). To publish with environment values, see [Publishing with environment values](#publishing-with-environment-values).

#### Returns

If the database was successfully published, returns JSON in the form:

```typescript
{ "Success": {
    "database_identity": string,
    "op": "created" | "updated"
} }
```

## `PUT /v1/database/:name_or_identity`

Publish to a database with the specified name or identity. If the name does not exist, creates a new database.

Accessible through the CLI as `spacetime publish`.

#### Query Parameters

| Name    | Value                                                                             |
| ------- | --------------------------------------------------------------------------------- |
| `clear` | A boolean; whether to clear any existing data when updating an existing database. |

#### Optional Headers

| Name                           | Value                                                                                                                                               |
| ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Authorization`                | A Spacetime token [as Bearer auth](./00100-authorization.md#authorization-headers).                                                                 |
| `spacetime-environment`        | One `KEY=VALUE` environment value. Repeat the header for each value. See [Publishing with environment values](#publishing-with-environment-values). |
| `spacetime-environment-remove` | Stored environment keys to delete, or `*` to replace the whole environment.                                                                         |

If no `Authorization` header is provided, a new anonymous identity will be created. When updating an existing database, the token must correspond to the database's owner, or the request will be rejected.

#### Data

A WebAssembly module in the [binary format](https://webassembly.github.io/spec/core/binary/index.html). To publish with environment values, see [Publishing with environment values](#publishing-with-environment-values).

#### Returns

If the database was successfully published, returns JSON in the form:

```typescript
{ "Success": {
    "domain": null | string,
    "database_identity": string,
    "op": "created" | "updated"
} }
```

If a database with the given name exists, but the identity provided in the `Authorization` header does not have permission to edit it, returns `401 UNAUTHORIZED` along with JSON in the form:

```typescript
{ "PermissionDenied": {
    "name": string
} }
```

### Publishing with environment values

Both publish endpoints accept [environment values](../../../00200-core-concepts/00100-databases/00700-environment-variables.md) in headers, alongside the module in the body. Send one `spacetime-environment` header per value, in the form `KEY=VALUE`:

```
spacetime-environment: API_KEY=development-only-key
spacetime-environment: MODE=development
```

Values are written like shell or `.env` assignments:

- A bare value, such as `API_KEY=abc123`, is taken literally. It cannot contain spaces, tabs, quotes, `,` or `\`.
- A single-quoted value, such as `GREETING='hello, world'`, is taken literally up to the closing quote. It cannot contain `'`.
- A double-quoted value, such as `PEM="line1\nline2"`, supports JSON string escapes, as in `.env` files: `\"`, `\\`, `\n`, `\t`, and `\uXXXX` for other control characters and non-ASCII characters, which cannot appear raw in a header.

Unlike a shell, the server does not expand variables. Values that break these rules, such as `GREETING=hello world`, are rejected with `400 Bad Request`. The server also accepts several values in one header, separated by commas outside quotes.

Formally, in [ABNF](https://www.rfc-editor.org/rfc/rfc5234), using the list syntax (`#`) from [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110#section-5.6.1):

```text
spacetime-environment = #assignment
assignment    = key "=" value
key           = ( ALPHA / "_" ) *( ALPHA / DIGIT / "_" )
value         = bare / single-quoted / double-quoted
bare          = *( %x21 / %x23-26 / %x28-2B / %x2D-5B / %x5D-7E )
                ; visible ASCII except " ' , \
single-quoted = "'" *( HTAB / %x20-26 / %x28-7E ) "'"
                ; taken literally
double-quoted = DQUOTE *( %x20-21 / %x23-5B / %x5D-7E / escape ) DQUOTE
escape        = "\" ( DQUOTE / "\" / "/" / %x62 / %x66 / %x6E / %x72 / %x74 / %x75 4HEXDIG )
                ; \" \\ \/ \b \f \n \r \t \uXXXX, lowercase only, as in JSON
                ; strings (RFC 8259). A \u escape of a UTF-16 high surrogate must be
                ; followed by one of a low surrogate, and a low surrogate cannot
                ; appear on its own.

spacetime-environment-remove = #( "*" / key )
                ; "*" must be the only element
```

Keys are at most 256 bytes and values at most 8 KiB. A key cannot appear twice across all `spacetime-environment` headers.

Supplied values override stored values with the same key, and stored values that are not supplied are kept. To delete stored values, list their keys in a `spacetime-environment-remove` header, separated by commas (`OLD_KEY, _UNUSED`). To replace the whole environment with only the supplied values, send `spacetime-environment-remove: *`. A key cannot be both supplied and removed.

The server validates the resulting environment against the module's declarations and installs the module and the environment in one transaction. All required values must be present, and every declared value must satisfy its declaration. Invalid updates leave the database unchanged.

If a required value is missing, the server returns `400 Bad Request` with JSON that names the missing key, so clients can prompt for it and retry:

```json
{
  "EnvironmentError": { "MissingRequiredEnvironment": { "keys": ["API_KEY"] } }
}
```

A reset (`clear=true`) deletes all stored values, so the request must supply every required value again.

For example, to publish a module to a local server with `curl`, export `SPACETIME_TOKEN` with a token authorized to publish, then run:

```bash
curl --fail-with-body --request PUT \
  'http://127.0.0.1:3000/v1/database/env-example?host_type=wasm' \
  --header "Authorization: Bearer $SPACETIME_TOKEN" \
  --header 'spacetime-environment: API_KEY=development-only-key' \
  --header 'spacetime-environment: MODE=development' \
  --data-binary @module.wasm
```

To change environment values without publishing a module, use [`PUT`](#put-v1databasename_or_identityenvironment) or [`PATCH /v1/database/:name_or_identity/environment`](#patch-v1databasename_or_identityenvironment).

## `GET /v1/database/:name_or_identity/environment`

Get the environment declarations of the database's current module and the keys of its stored values. Values are never returned. Requires a token authorized to update the database.

#### Returns

```typescript
{
    "module_hash": string,
    "declarations": [{
        "name": string,
        "optional": boolean,
        "ty": "String" | { "StringLiteral": string } | { "Union": string[] }
    }],
    "stored_keys": string[]
}
```

`module_hash` identifies the module the declarations came from. Treat it as an opaque string and pass it unchanged as `expected_module_hash` when updating the environment.

## `PUT /v1/database/:name_or_identity/environment`

Replace the database's whole environment without publishing a module. Stored values that are not supplied are deleted.

#### Query Parameters

| Name                   | Value                                                                                    |
| ---------------------- | ---------------------------------------------------------------------------------------- |
| `expected_module_hash` | The `module_hash` from [`GET /environment`](#get-v1databasename_or_identityenvironment). |

#### Data

A JSON object mapping each key to its string value, for example `{"API_KEY": "new-key", "MODE": "production"}`.

#### Returns

On success, returns `200 OK` with `{"Ok": null}`. If the resulting environment does not satisfy the module's declarations, returns `400 Bad Request`, with `{"Err": {"MissingRequiredEnvironment": {"keys": [...]}}}` when a required value is missing. If the database's module no longer has `expected_module_hash`, returns `409 Conflict`; fetch the metadata again before retrying.

## `PATCH /v1/database/:name_or_identity/environment`

Set and delete individual environment values without publishing a module. Stored values that are neither supplied nor removed are kept.

#### Query Parameters

| Name                   | Value                                                                                    |
| ---------------------- | ---------------------------------------------------------------------------------------- |
| `expected_module_hash` | The `module_hash` from [`GET /environment`](#get-v1databasename_or_identityenvironment). |

#### Optional Headers

| Name                           | Value                                                                               |
| ------------------------------ | ----------------------------------------------------------------------------------- |
| `spacetime-environment-remove` | Stored keys to delete, separated by commas. `*` is not accepted; use `PUT` instead. |

#### Data

A JSON object mapping each key to set to its string value. Send `{}` to only delete keys.

#### Returns

The same as [`PUT /environment`](#put-v1databasename_or_identityenvironment).

## `GET /v1/database/:name_or_identity`

Get a database's identity, owner identity, host type, number of replicas and a hash of its WASM module.

#### Returns

Returns JSON in the form:

```typescript
{
    "database_identity": string,
    "owner_identity": string,
    "host_type": "wasm",
    "initial_program": string
}
```

| Field                 | Type   | Meaning                                                          |
| --------------------- | ------ | ---------------------------------------------------------------- |
| `"database_identity"` | String | The Spacetime identity of the database.                          |
| `"owner_identity"`    | String | The Spacetime identity of the database's owner.                  |
| `"host_type"`         | String | The module host type; currently always `"wasm"`.                 |
| `"initial_program"`   | String | Hash of the WASM module with which the database was initialized. |

## `DELETE /v1/database/:name_or_identity`

Delete a database.

Accessible through the CLI as `spacetime delete <identity>`.

#### Optional Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](./00100-authorization.md#authorization-headers). |

Deleting a database requires ownership. If no `Authorization` header is provided, the request will be treated as anonymous and will be rejected.

## `GET /v1/database/:name_or_identity/names`

Get the names this database can be identified by.

#### Returns

Returns JSON in the form:

```typescript
{ "names": array<string> }
```

where `<names>` is a JSON array of strings, each of which is a name which refers to the database.

## `POST /v1/database/:name_or_identity/names`

Add a new name for this database.

#### Optional Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](./00100-authorization.md#authorization-headers). |

If no `Authorization` header is provided, the request will be treated as anonymous.

#### Data

Takes as the request body a string containing the new name of the database.

#### Returns

If the name was successfully set, returns JSON in the form:

```typescript
{ "Success": {
    "domain": string,
    "database_result": string
} }
```

If the new name already exists but the identity provided in the `Authorization` header does not have permission to edit it, returns JSON in the form:

```typescript
{ "PermissionDenied": {
    "domain": string
} }
```

## `PUT /v1/database/:name_or_identity/names`

Set the list of names for this database.

#### Optional Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](./00100-authorization.md#authorization-headers). |

Setting names requires ownership of the database. If no `Authorization` header is provided, the request will be treated as anonymous and will be rejected.

#### Data

Takes as the request body a list of names, as a JSON array of strings.

#### Returns

If the name was successfully set, returns JSON in the form:

```typescript
{ "Success": null }
```

If any of the new names already exist but the identity provided in the `Authorization` header does not have permission to edit it, returns `401 UNAUTHORIZED` along with JSON in the form:

```typescript
{ "PermissionDenied": null }
```

## `GET /v1/database/:name_or_identity/identity`

Get the identity of a database.

#### Returns

Returns a hex string of the specified database's identity.

## `GET /v1/database/:name_or_identity/subscribe`

Begin a WebSocket connection with a database.

#### Required Headers

For more information about WebSocket headers, see [RFC 6455](https://datatracker.ietf.org/doc/html/rfc6455).

| Name                     | Value                                                                 |
| ------------------------ | --------------------------------------------------------------------- |
| `Sec-WebSocket-Protocol` | `v1.bsatn.spacetimedb` or `v1.json.spacetimedb`                       |
| `Connection`             | `Upgrade`                                                             |
| `Upgrade`                | `websocket`                                                           |
| `Sec-WebSocket-Version`  | `13`                                                                  |
| `Sec-WebSocket-Key`      | A 16-byte value, generated randomly by the client, encoded as Base64. |

The SpacetimeDB binary WebSocket protocol, `v1.bsatn.spacetimedb`, encodes messages as well as reducer and row data using [BSATN](../00300-internals/00300-bsatn.md).
Its messages are defined [here](https://github.com/clockworklabs/SpacetimeDB/blob/master/crates/client-api-messages/src/websocket.rs).

The SpacetimeDB text WebSocket protocol, `v1.json.spacetimedb`, encodes messages according to the [SATS-JSON format](../00300-internals/00200-sats-json.md).

#### Optional Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](./00100-authorization.md#authorization-headers). |

## `POST /v1/database/:name_or_identity/call/:reducer`

Invoke a reducer in a database.

#### Path parameters

| Name       | Value                                 |
| ---------- | ------------------------------------- |
| `:reducer` | The name of the reducer OR procedure. |

#### Optional Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](./00100-authorization.md#authorization-headers). |

If no `Authorization` header is provided, the request will be treated as anonymous. The caller's identity is passed to the reducer via its `ReducerContext`, and the module may accept or reject the call based on that identity.

#### Data

A JSON array of arguments to the reducer.

## `GET /v1/database/:name_or_identity/schema`

Get a schema for a database.

Accessible through the CLI as `spacetime describe <name_or_identity>`.

#### Query Parameters

| Name      | Value                                            |
| --------- | ------------------------------------------------ |
| `version` | The version of `RawModuleDef` to return, e.g. 9. |

#### Optional Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](./00100-authorization.md#authorization-headers). |

No authorization is required to fetch a database's schema. If an `Authorization` header is provided, the response will include `spacetime-identity` and `spacetime-identity-token` headers echoing the caller's identity. If omitted, a new anonymous identity will be allocated for this purpose.

#### Returns

Returns a `RawModuleDef` in JSON form.

<details>
<summary>Example response from `/schema?version=9` for the default module generated by `spacetime init`</summary>

```json
{
  "typespace": {
    "types": [
      {
        "Product": {
          "elements": [
            {
              "name": {
                "some": "name"
              },
              "algebraic_type": {
                "String": []
              }
            }
          ]
        }
      }
    ]
  },
  "tables": [
    {
      "name": "person",
      "product_type_ref": 0,
      "primary_key": [],
      "indexes": [],
      "constraints": [],
      "sequences": [],
      "schedule": {
        "none": []
      },
      "table_type": {
        "User": []
      },
      "table_access": {
        "Private": []
      }
    }
  ],
  "reducers": [
    {
      "name": "add",
      "params": {
        "elements": [
          {
            "name": {
              "some": "name"
            },
            "algebraic_type": {
              "String": []
            }
          }
        ]
      },
      "lifecycle": {
        "none": []
      }
    },
    {
      "name": "identity_connected",
      "params": {
        "elements": []
      },
      "lifecycle": {
        "some": {
          "OnConnect": []
        }
      }
    },
    {
      "name": "identity_disconnected",
      "params": {
        "elements": []
      },
      "lifecycle": {
        "some": {
          "OnDisconnect": []
        }
      }
    },
    {
      "name": "init",
      "params": {
        "elements": []
      },
      "lifecycle": {
        "some": {
          "Init": []
        }
      }
    },
    {
      "name": "say_hello",
      "params": {
        "elements": []
      },
      "lifecycle": {
        "none": []
      }
    }
  ],
  "types": [
    {
      "name": {
        "scope": [],
        "name": "Person"
      },
      "ty": 0,
      "custom_ordering": true
    }
  ],
  "misc_exports": [],
  "row_level_security": []
}
```

</details>

## `GET /v1/database/:name_or_identity/logs`

Retrieve logs from a database.

Accessible through the CLI as `spacetime logs <name_or_identity>`.

#### Query Parameters

| Name        | Value                                                           |
| ----------- | --------------------------------------------------------------- |
| `num_lines` | Number of most-recent log lines to retrieve.                    |
| `follow`    | A boolean; whether to continue receiving new logs via a stream. |

#### Optional Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](./00100-authorization.md#authorization-headers). |

Viewing logs requires ownership of the database. If no `Authorization` header is provided, the request will be treated as anonymous and will be rejected.

#### Returns

Text, or streaming text if `follow` is supplied, containing log lines.

## `POST /v1/database/:name_or_identity/sql`

Run a SQL query against a database.

Accessible through the CLI as `spacetime sql <name_or_identity> <query>`.

#### Optional Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](./00100-authorization.md#authorization-headers). |

If no `Authorization` header is provided, the request will be treated as anonymous and will only have access to public tables. The caller's identity is used to enforce row-level security policies.

#### Data

SQL queries, separated by `;`.

#### Returns

Returns a JSON array of statement results, each of which takes the form:

```typescript
{
    "schema": ProductType,
    "rows": array
}
```

The `schema` will be a [JSON-encoded `ProductType`](../00300-internals/00200-sats-json.md) describing the type of the returned rows.

The `rows` will be an array of [JSON-encoded `ProductValue`s](../00300-internals/00200-sats-json.md), each of which conforms to the `schema`.

## `ANY /v1/database/:name_or_identity/route/{*path}`

Access routes defined by a database using [HTTP handlers](../../../00200-core-concepts/00200-functions/00600-HTTP-handlers.md).
