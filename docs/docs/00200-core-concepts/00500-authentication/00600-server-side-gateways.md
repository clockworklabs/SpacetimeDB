---
title: Server-Side Gateways
slug: /authentication/server-side-gateways
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

Many applications connect browsers, mobile clients, API integrations, or agents
to an application server first, then let that server connect to SpacetimeDB. This
is useful when the application server owns web sessions, API keys, rate limits,
tenant selection, or other authorization state that should not be exposed to the
browser.

The important design question is actor attribution: which identity should
SpacetimeDB see when a reducer is called?

Every reducer call has a `ctx.sender`. If a browser connects directly with a
user token, `ctx.sender` is that user. If an application server calls reducers
over a service connection, `ctx.sender` is the service identity. Server-side
gateway architectures should choose this intentionally and document it in module
code.

The snippets below focus on the authorization shape and assume your module has
already defined `spacetimedb`, `t`, and the referenced tables.

## Gateway Topologies

### Per-user connections

In this topology, the application server opens a SpacetimeDB connection using a
token scoped to the current user. Reducers see the user as `ctx.sender`.

Use this when:

- Reducers should use native `ctx.sender` as the user identity.
- Existing module authorization is already written around `ctx.sender`.
- Audit records should identify the user without extra delegation fields.

Tradeoffs:

- The gateway may need many WebSocket connections.
- The application server must refresh user-scoped tokens and recreate
  connections when they expire.
- Subscriptions are naturally scoped by user, but cross-user fanout may require
  more server memory and connection management.

Example reducer shape:

```typescript
export const create_note = spacetimedb.reducer(
  { body: t.string() },
  (ctx, { body }) => {
    ctx.db.note.insert({
      id: 0n,
      owner: ctx.sender,
      body,
      createdAt: ctx.timestamp,
    });
  }
);
```

### Service or robot connections

In this topology, the application server connects with a service account,
robot, or gateway token. Reducers see the service identity as `ctx.sender`.
User attribution is passed separately as trusted, server-derived context.

Use this when:

- The application server owns all browser sessions and reducer calls.
- You want fewer long-lived SpacetimeDB connections.
- A gateway subscribes once and relays data to many browser clients.
- Reducers should treat the gateway as a privileged integration boundary.

Tradeoffs:

- `ctx.sender` is the gateway, not the browser user.
- Reducers must not trust actor IDs supplied directly by the browser.
- Module code should verify that only the gateway can call delegated reducers.
- Audit tables should record both the gateway identity and the effective actor.

The application server should derive the effective actor from a verified session,
API key, or service credential, then pass only that trusted value to reducers.

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TS">

```typescript
import { SenderError, type ReducerCtx } from 'spacetimedb/server';

function requireGateway(ctx: ReducerCtx<any>) {
  const jwt = ctx.senderAuth.jwt;
  if (jwt == null) {
    throw new SenderError('Gateway token required');
  }
  if (jwt.fullPayload['token_type'] !== 'spacetime-gateway') {
    throw new SenderError('Gateway token required');
  }
}

export const create_note_for_actor = spacetimedb.reducer(
  {
    actor: t.identity(),
    body: t.string(),
  },
  (ctx, { actor, body }) => {
    requireGateway(ctx);

    ctx.db.note.insert({
      id: 0n,
      owner: actor,
      body,
      createdBy: ctx.sender,
      createdAt: ctx.timestamp,
    });
  }
);
```

</TabItem>
</Tabs>

This pattern makes the trust boundary explicit. The reducer is not saying "the
browser says it is this actor." It is saying "the verified gateway is asserting
this effective actor."

### Hybrid connections

In a hybrid topology, the application server uses one connection shape for
subscriptions and another for writes:

- Service connection for shared subscriptions, projections, background jobs, or
  Server-Sent Events relay.
- User-scoped connection for writes that should preserve native `ctx.sender`.
- Service or robot connection for jobs and integrations that are not acting as a
  human user.

Use this when:

- Browser UI updates are best served through a shared server-side projection.
- Some reducers should still see the human user as `ctx.sender`.
- Other reducers are operational tasks performed by a trusted service.

Tradeoffs:

- There are more moving pieces.
- The module must document which reducers expect user identities and which
  reducers expect service identities.
- The application server must keep subscription state and write paths separate.

## Recommended Checks

Whichever topology you choose, reducers should fail closed. Common checks
include:

- Require a JWT for authenticated reducers.
- Validate the `iss` claim.
- Validate the `aud` claim so tokens issued for another application are not
  accepted.
- Use a custom claim such as `token_type` or `scope` to distinguish browser,
  gateway, and robot tokens.
- Store mutable roles, tenant memberships, impersonation grants, and revocation
  state in tables rather than long-lived JWT claims.
- Keep identity mapping, API key metadata, and delegation grants in private
  tables.
- Record audit events that include `ctx.sender`, the effective actor when
  delegated, the reducer name, and relevant tenant or resource IDs.

## Choosing a Topology

| Requirement | Recommended topology |
| --- | --- |
| Reducers already use `ctx.sender` as the user | Per-user connection |
| Lowest WebSocket count for browser dashboards | Service connection for subscriptions |
| Native user attribution for writes plus shared read projection | Hybrid |
| Scheduled jobs, webhooks, or AI agents | Service or robot connection |
| Customer API keys that act as integrations | Validate API key in the app server, then call SpacetimeDB as a robot actor |

The safest default is to preserve native `ctx.sender` for user-initiated writes
unless you have intentionally modeled delegation. Use service or robot
connections for server-owned workflows, shared subscriptions, and integrations.

## Related Docs

- [Using Auth Claims](./00500-usage.md)
- [Reducer Context](../00200-functions/00300-reducers/00400-reducer-context.md)
- [Table Access Permissions](../00300-tables/00400-access-permissions.md)
- [Connecting to SpacetimeDB](../00600-clients/00300-connection.md)
