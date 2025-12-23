---
slug: /http/identity
---

# `/v1/identity`

The HTTP endpoints in `/v1/identity` allow clients to generate and manage Spacetime public identities and private tokens.

## At a glance

| Route                                                                      | Description                                                        |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| [`POST /v1/identity`](#post-v1identity)                                    | Generate a new identity and token.                                 |
| [`POST /v1/identity/websocket-token`](#post-v1identitywebsocket-token)     | Generate a short-lived access token for use in untrusted contexts. |
| [`GET /v1/identity/public-key`](#get-v1identitypublic-key)                 | Get the public key used for verifying tokens.                      |
| [`GET /v1/identity/:identity/databases`](#get-v1identityidentitydatabases) | List databases owned by an identity.                               |
| [`GET /v1/identity/:identity/verify`](#get-v1identityidentityverify)       | Verify an identity and token.                                      |

## `POST /v1/identity`

Create a new identity.

#### Returns

Returns JSON in the form:

```typescript
{
    "identity": string,
    "token": string
}
```

## `POST /v1/identity/websocket-token`

Generate a short-lived access token which can be used in untrusted contexts, e.g. embedded in URLs.

#### Required Headers

| Name            | Value                                                                         |
| --------------- | ----------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/http/authorization). |

#### Returns

Returns JSON in the form:

```typescript
{
    "token": string
}
```

The `token` value is a short-lived [JSON Web Token](https://datatracker.ietf.org/doc/html/rfc7519).

## `GET /v1/identity/public-key`

Fetches the public key used by the database to verify tokens.

#### Returns

Returns a response of content-type `application/pem-certificate-chain`.

## `POST /v1/identity/:identity/set-email`

Associate an email with a Spacetime identity.

#### Parameters

| Name        | Value                                     |
| ----------- | ----------------------------------------- |
| `:identity` | The identity to associate with the email. |

#### Query Parameters

| Name    | Value             |
| ------- | ----------------- |
| `email` | An email address. |

#### Required Headers

| Name            | Value                                                                         |
| --------------- | ----------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/http/authorization). |

## `GET /v1/identity/:identity/databases`

List all databases owned by an identity.

#### Parameters

| Name        | Value                 |
| ----------- | --------------------- |
| `:identity` | A Spacetime identity. |

#### Returns

Returns JSON in the form:

```typescript
{
    "addresses": array<string>
}
```

The `addresses` value is an array of zero or more strings, each of which is the address of a database owned by the identity passed as a parameter.

## `GET /v1/identity/:identity/verify`

Verify the validity of an identity/token pair.

#### Parameters

| Name        | Value                   |
| ----------- | ----------------------- |
| `:identity` | The identity to verify. |

#### Required Headers

| Name            | Value                                                                         |
| --------------- | ----------------------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/http/authorization). |

#### Returns

Returns no data.

If the token is valid and matches the identity, returns `204 No Content`.

If the token is valid but does not match the identity, returns `400 Bad Request`.

If the token is invalid, or no `Authorization` header is included in the request, returns `401 Unauthorized`.
