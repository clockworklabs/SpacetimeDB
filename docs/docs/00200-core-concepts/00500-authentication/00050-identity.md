---
title: Identity
slug: /identity
---

An `Identity` is a 256-bit identifier for a principal in SpacetimeDB. Users and
databases both have identities. Use an identity to associate rows with their
owner, identify the caller of a reducer, or filter a per-user view.

An identity is **not a credential**. It is safe to send to other clients when
your application needs to identify a user. Knowing an identity, or constructing
an `Identity` value from its hexadecimal representation, does not let a client
act as that principal. The authentication token is the credential; keep it private.

## Where identities come from

For a client authenticated with an OIDC token, SpacetimeDB derives its identity
from the token's issuer (`iss`) and subject (`sub`):

- The same issuer and subject produce the same identity, including across
  different databases.
- Changing either the issuer or the subject produces a different identity.
  Moving to another authentication provider does not automatically transfer
  ownership of rows associated with the previous identity.
- Other claims, such as email, audience, and roles, do not determine the identity.
  A token refresh with the same issuer and subject retains the same identity.

The identity does not contain recoverable JWT claims. You cannot extract an
email, issuer, subject, or role from it. Read claims from the authenticated
context where available; see [Using Auth Claims](./00500-usage.md).

If a client connects without a token, SpacetimeDB creates an identity and returns
a server-issued token. Persist that token and supply it on subsequent connections
to keep using the same identity. Connecting without it creates a new identity;
passing the old identity as a reducer argument does not restore the old session.
See [Server-issued tokens and reconnects](../00500-authentication.md#server-issued-tokens-and-reconnects).

An identity does not by itself grant access to your application. Validate the
expected issuer and audience when accepting clients, then apply your application's
authorization rules.

## Identity versus ConnectionId

An `Identity` identifies a principal across connections. A `ConnectionId`
identifies one client connection: the same user can have several simultaneous
connections, each with its own connection ID. Use `Identity` for persistent
ownership and permissions, and `ConnectionId` for connection-specific state.

Databases also have identities, distinct from their owners' identities. Scheduled
reducers run as the database itself. With
[inter-database communication](../00100-databases/00650-inter-database-communication.md),
the receiving reducer's sender is the sending database, not a user of that database.
See [The Sender of Each Invocation](./00500-usage.md#the-sender-of-each-invocation)
for the sender used in each context.

## Accessing the caller's identity

Use the sender supplied by the context, not an identity submitted by the client:

| Language   | Reducer or per-user view | Identity column type |
| ---------- | ------------------------ | -------------------- |
| TypeScript | `ctx.sender`             | `t.identity()`       |
| C#         | `ctx.Sender`             | `Identity`           |
| Rust       | `ctx.sender()`           | `Identity`           |
| C++        | `ctx.sender()`           | `Identity`           |

Anonymous views have no sender. Neither per-user nor anonymous views expose
`senderAuth`, `SenderAuth`, or `sender_auth()`, so a view cannot read JWT claims.
Use the sender to look up ownership or permissions in your tables. For
claim-derived permissions, validate the claims in a reducer, store the required
authorization data in a private table keyed by identity, and read that table from
the view. Keep that stored data up to date when permissions change.

### Example: a private profile and per-user view

This TypeScript module stores one profile per identity. The reducer uses the
authenticated sender as the primary key, so clients cannot submit someone else's
identity to overwrite their profile. The table is private; clients read their own
profile through the view.

```typescript
import { schema, table, t } from 'spacetimedb/server';

const profile = table(
  { public: false },
  {
    identity: t.identity().primaryKey(),
    displayName: t.string(),
  }
);

const spacetimedb = schema({ profile });
export default spacetimedb;

export const setDisplayName = spacetimedb.reducer(
  { displayName: t.string() },
  (ctx, { displayName }) => {
    const row = { identity: ctx.sender, displayName };
    if (ctx.db.profile.identity.find(ctx.sender)) {
      ctx.db.profile.identity.update(row);
    } else {
      ctx.db.profile.insert(row);
    }
  }
);

export const myProfile = spacetimedb.view(
  { public: true },
  t.option(profile.rowType),
  ctx => ctx.db.profile.identity.find(ctx.sender) ?? undefined
);
```

Clients must subscribe to the view to receive its rows; see [Views](../00200-functions/00500-views.md).

## Comparing and formatting identities

Use value equality. In TypeScript, identities are objects, so `===` compares
object references rather than identity values:

```typescript
import { Identity } from 'spacetimedb';

const first = Identity.fromString(
  'c200000000000000000000000000000000000000000000000000000000000001'
);
const second = Identity.fromString(first.toHexString());

console.log(first.isEqual(second)); // true
console.log(first === second); // false: different objects
```

The following APIs format a full identity as 64 hexadecimal characters and parse
it back. Store the full value, not an abbreviated log representation.

| Language   | Format                   | Parse                                        | Value equality |
| ---------- | ------------------------ | -------------------------------------------- | -------------- |
| TypeScript | `identity.toHexString()` | `Identity.fromString(hex)`                   | `a.isEqual(b)` |
| C#         | `identity.ToString()`    | `Identity.FromHexString(hex)`                | `a == b`       |
| Rust       | `identity.to_hex()`      | `Identity::from_hex(hex)` (returns `Result`) | `a == b`       |

In C++ modules, use `identity.to_hex_string()` to format an identity and `a == b`
to compare values. Prefer SDK conversion methods over manipulating raw bytes:
textual hexadecimal order and the little-endian BSATN wire representation are
different.

Parsing an identity only constructs a value for comparisons, lookups, or
arguments. It does not authenticate a connection or change its sender.
