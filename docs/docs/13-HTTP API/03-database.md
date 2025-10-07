---
slug: /http/database
---

# `/v1/database`

The HTTP endpoints in `/v1/database` allow clients to interact with Spacetime databases in a variety of ways, including retrieving information, creating and deleting databases, invoking reducers and evaluating SQL queries.

## At a glance

| Route                                                                                              | Description                                       |
| -------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| [`POST /v1/database`](#post-v1database)                                                            | Publish a new database given its module code.     |
| [`POST /v1/database/:name_or_identity`](#post-v1databasename_or_identity)                          | Publish to a database given its module code.      |
| [`GET /v1/database/:name_or_identity`](#get-v1databasename_or_identity)                            | Get a JSON description of a database.             |
| [`DELETE /v1/database/:name_or_identity`](#post-v1databasename_or_identity)                        | Delete a database.                                |
| [`GET /v1/database/:name_or_identity/names`](#get-v1databasename_or_identitynames)                 | Get the names this database can be identified by. |
| [`POST /v1/database/:name_or_identity/names`](#post-v1databasename_or_identitynames)               | Add a new name for this database.                 |
| [`PUT /v1/database/:name_or_identity/names`](#put-v1databasename_or_identitynames)                 | Set the list of names for this database.          |
| [`GET /v1/database/:name_or_identity/identity`](#get-v1databasename_or_identityidentity)           | Get the identity of a database.                   |
| [`GET /v1/database/:name_or_identity/subscribe`](#get-v1databasename_or_identitysubscribe)         | Begin a WebSocket connection.                     |
| [`POST /v1/database/:name_or_identity/call/:reducer`](#post-v1databasename_or_identitycallreducer) | Invoke a reducer in a database.                   |
| [`GET /v1/database/:name_or_identity/schema`](#get-v1databasename_or_identityschema)               | Get the schema for a database.                    |
| [`GET /v1/database/:name_or_identity/logs`](#get-v1databasename_or_identitylogs)                   | Retrieve logs from a database.                    |
| [`POST /v1/database/:name_or_identity/sql`](#post-v1databasename_or_identitysql)                   | Run a SQL query against a database.               |

## `POST /v1/database`

Publish a new database with no name.

Accessible through the CLI as `spacetime publish`.

#### Required Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](/http/authorization#authorization-headers). |

#### Data

A WebAssembly module in the [binary format](https://webassembly.github.io/spec/core/binary/index.html).

#### Returns

If the database was successfully published, returns JSON in the form:

```typescript
{ "Success": {
    "database_identity": string,
    "op": "created" | "updated"
} }
```

## `POST /v1/database/:name_or_identity`

Publish to a database with the specified name or identity. If the name doesn't exist, creates a new database.

Accessible through the CLI as `spacetime publish`.

#### Query Parameters

| Name    | Value                                                                             |
| ------- | --------------------------------------------------------------------------------- |
| `clear` | A boolean; whether to clear any existing data when updating an existing database. |

#### Required Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](/http/authorization#authorization-headers). |

#### Data

A WebAssembly module in the [binary format](https://webassembly.github.io/spec/core/binary/index.html).

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

#### Required Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](/http/authorization#authorization-headers). |

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

#### Required Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](/http/authorization#authorization-headers). |

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

#### Required Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](/http/authorization#authorization-headers). |

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
| `Connection`             | `Updgrade`                                                            |
| `Upgrade`                | `websocket`                                                           |
| `Sec-WebSocket-Version`  | `13`                                                                  |
| `Sec-WebSocket-Key`      | A 16-byte value, generated randomly by the client, encoded as Base64. |

The SpacetimeDB binary WebSocket protocol, `v1.bsatn.spacetimedb`, encodes messages as well as reducer and row data using [BSATN](/bsatn).
Its messages are defined [here](https://github.com/clockworklabs/SpacetimeDB/blob/master/crates/client-api-messages/src/websocket.rs).

The SpacetimeDB text WebSocket protocol, `v1.json.spacetimedb`, encodes messages according to the [SATS-JSON format](/sats-json).

#### Optional Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](/http/authorization#authorization-headers). |

## `POST /v1/database/:name_or_identity/call/:reducer`

Invoke a reducer in a database.

#### Path parameters

| Name       | Value                    |
| ---------- | ------------------------ |
| `:reducer` | The name of the reducer. |

#### Required Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](/http/authorization#authorization-headers). |

#### Data

A JSON array of arguments to the reducer.

## `GET /v1/database/:name_or_identity/schema`

Get a schema for a database.

Accessible through the CLI as `spacetime describe <name_or_identity>`.

#### Query Parameters

| Name      | Value                                            |
| --------- | ------------------------------------------------ |
| `version` | The version of `RawModuleDef` to return, e.g. 9. |

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

#### Required Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](/http/authorization#authorization-headers). |

#### Returns

Text, or streaming text if `follow` is supplied, containing log lines.

## `POST /v1/database/:name_or_identity/sql`

Run a SQL query against a database.

Accessible through the CLI as `spacetime sql <name_or_identity> <query>`.

#### Required Headers

| Name            | Value                                                                               |
| --------------- | ----------------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [as Bearer auth](/http/authorization#authorization-headers). |

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

The `schema` will be a [JSON-encoded `ProductType`](/sats-json) describing the type of the returned rows.

The `rows` will be an array of [JSON-encoded `ProductValue`s](/sats-json), each of which conforms to the `schema`.
