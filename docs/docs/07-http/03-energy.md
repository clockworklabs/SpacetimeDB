---
title: `/energy`
---

The HTTP endpoints in `/energy` allow clients to query identities' energy balances. Spacetime databases expend energy from their owners' balances while executing reducers.

## At a glance

| Route                                            | Description                                               |
| ------------------------------------------------ | --------------------------------------------------------- |
| [`/energy/:identity GET`](#energyidentity-get)   | Get the remaining energy balance for the user `identity`. |
| [`/energy/:identity POST`](#energyidentity-post) | Set the energy balance for the user `identity`.           |

## `/energy/:identity GET`

Get the energy balance of an identity.

Accessible through the CLI as `spacetime energy status <identity>`.

#### Parameters

| Name        | Value                   |
| ----------- | ----------------------- |
| `:identity` | The Spacetime identity. |

#### Returns

Returns JSON in the form:

```typescript
{
    "balance": string
}
```

| Field     | Value                                                                                                                                                          |
| --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `balance` | The identity's energy balance, as a decimal integer. Note that energy balances may be negative, and will frequently be too large to store in a 64-bit integer. |

## `/energy/:identity POST`

Set the energy balance for an identity.

Note that in the SpacetimeDB 0.6 Testnet, this endpoint always returns code 401, `UNAUTHORIZED`. Testnet energy balances cannot be refilled.

Accessible through the CLI as `spacetime energy set-balance <balance> <identity>`.

#### Parameters

| Name        | Value                   |
| ----------- | ----------------------- |
| `:identity` | The Spacetime identity. |

#### Query Parameters

| Name      | Value                                      |
| --------- | ------------------------------------------ |
| `balance` | A decimal integer; the new balance to set. |

#### Required Headers

| Name            | Value                                                           |
| --------------- | --------------------------------------------------------------- |
| `Authorization` | A Spacetime token [encoded as Basic authorization](/docs/http). |

#### Returns

Returns JSON in the form:

```typescript
{
    "balance": number
}
```

| Field     | Value                                                                                                                                                              |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `balance` | The identity's new energy balance, as a decimal integer. Note that energy balances may be negative, and will frequently be too large to store in a 64-bit integer. |
