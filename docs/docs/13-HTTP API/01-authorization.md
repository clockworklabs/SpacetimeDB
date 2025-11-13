---
title: Authorization
slug: /http/authorization
---

# SpacetimeDB HTTP Authorization

### Generating identities and tokens

SpacetimeDB can derive an identity from the `sub` and `iss` claims of any [OpenID Connect](https://openid.net/developers/how-connect-works/) compliant [JSON Web Token](https://jwt.io/).

Clients can request a new identity and token signed by the SpacetimeDB host via [the `POST /v1/identity` HTTP endpoint](/http/identity#post-v1identity). Such a token will not be portable to other SpacetimeDB clusters.

Alternately, a new identity and token will be generated during an anonymous connection via the WebSocket API, and passed to the client as an `IdentityToken` message.

### `Authorization` headers

Many SpacetimeDB HTTP endpoints either require or optionally accept a token in the `Authorization` header. SpacetimeDB authorization headers are of the form `Authorization: Bearer ${token}`, where `token` is an [OpenID Connect](https://openid.net/developers/how-connect-works/) compliant [JSON Web Token](https://jwt.io/), such as the one returned from [the `POST /v1/identity` HTTP endpoint](/http/identity#post-v1identity).

# Top level routes

| Route                         | Description                                            |
| ----------------------------- | ------------------------------------------------------ |
| [`GET /v1/ping`](#get-v1ping) | No-op. Used to determine whether a client can connect. |

## `GET /v1/ping`

Does nothing and returns no data. Clients can send requests to this endpoint to determine whether they are able to connect to SpacetimeDB.
