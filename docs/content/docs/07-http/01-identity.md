---
title: `/identity` HTTP API
navTitle: `/identity`
---

The HTTP endpoints in `/identity` allow clients to generate and manage Spacetime public identities and private tokens.

## At a glance

| Route                                                                   | Description                                                        |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------ |
| [`/identity GET`](#identity-get)                                        | Look up an identity by email.                                      |
| [`/identity POST`](#identity-post)                                      | Generate a new identity and token.                                 |
| [`/identity/websocket_token POST`](#identitywebsocket_token-post)       | Generate a short-lived access token for use in untrusted contexts. |
| [`/identity/:identity/set-email POST`](#identityidentityset-email-post) | Set the email for an identity.                                     |
| [`/identity/:identity/databases GET`](#identityidentitydatabases-get)   | List databases owned by an identity.                               |
| [`/identity/:identity/verify GET`](#identityidentityverify-get)         | Verify an identity and token.                                      |

## `/identity GET`

Look up Spacetime identities associated with an email.

Accessible through the CLI as `spacetime identity find <email>`.

### Query Parameters

| Name    | Value                           |
| ------- | ------------------------------- |
| `email` | An email address to search for. |

### Returns

Returns JSON in the form:

```typescript
{
    "identities": [
        {
            "identity": string,
            "email": string
        }
    ]
}
```

The `identities` value is an array of zero or more objects, each of which has an `identity` and an `email`. Each `email` will be the same as the email passed as a query parameter.

## `/identity POST`

Create a new identity.

Accessible through the CLI as `spacetime identity new`.

### Query Parameters

| Name    | Value                                                                                                                   |
| ------- | ----------------------------------------------------------------------------------------------------------------------- |
| `email` | An email address to associate with the new identity. If unsupplied, the new identity will not have an associated email. |

#### Returns

Returns JSON in the form:

```typescript
{
    "identity": string,
    "token": string
}
```

## `/identity/websocket_token POST`

Generate a short-lived access token which can be used in untrusted contexts, e.g. embedded in URLs.

### Required Headers

| Name            | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http). |

### Returns

Returns JSON in the form:

```typescript
{
    "token": string
}
```

The `token` value is a short-lived [JSON Web Token](https://datatracker.ietf.org/doc/html/rfc7519).

## `/identity/:identity/set-email POST`

Associate an email with a Spacetime identity.

Accessible through the CLI as `spacetime identity set-email <identity> <email>`.

### Parameters

| Name        | Value                                     |
| ----------- | ----------------------------------------- |
| `:identity` | The identity to associate with the email. |

### Query Parameters

| Name    | Value             |
| ------- | ----------------- |
| `email` | An email address. |

### Required Headers

| Name            | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http). |

## `/identity/:identity/databases GET`

List all databases owned by an identity.

### Parameters

| Name        | Value                 |
| ----------- | --------------------- |
| `:identity` | A Spacetime identity. |

### Returns

Returns JSON in the form:

```typescript
{
    "addresses": array<string>
}
```

The `addresses` value is an array of zero or more strings, each of which is the address of a database owned by the identity passed as a parameter.

## `/identity/:identity/verify GET`

Verify the validity of an identity/token pair.

### Parameters

| Name        | Value                   |
| ----------- | ----------------------- |
| `:identity` | The identity to verify. |

### Required Headers

| Name            | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http). |

### Returns

Returns no data.

If the token is valid and matches the identity, returns `204 No Content`.

If the token is valid but does not match the identity, returns `400 Bad Request`.

If the token is invalid, or no `Authorization` header is included in the request, returns `401 Unauthorized`.
