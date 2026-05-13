---
title: App-Owned Auth Broker
slug: /authentication/app-owned-auth-broker
---

# App-Owned Auth Broker

Many applications already have an authentication system before they add SpacetimeDB. A full-stack web app may own browser sessions, organization membership, admin flows, customer SSO, API keys, rate limits, and tenant selection. In that architecture, SpacetimeDB does not need to become the web-session provider. Instead, the application server can mint a short-lived SpacetimeDB token after it verifies the app session.

The token broker pattern looks like this:

```text
Browser session or API key
  -> application server verifies the caller and active tenant
  -> application server signs a short-lived OIDC/JWT token for SpacetimeDB
  -> client or server gateway connects to SpacetimeDB with that token
  -> SpacetimeDB validates the token and exposes its claims to module code
```

The SpacetimeDB token is not the web session. It should be short-lived, narrowly scoped, and derived from authorization state the application server has already checked.

## When to Use This Pattern

Use an app-owned auth broker when:

- Your application already owns user sessions through a framework, auth library, or identity provider.
- Tenant or organization membership changes frequently and should be checked before issuing database access.
- Browser clients should not receive long-lived credentials, refresh tokens, or machine credentials.
- A server-side gateway connects to SpacetimeDB on behalf of users, API keys, scheduled jobs, or service accounts.
- You are migrating from one OIDC provider to another and need an explicit identity mapping plan.

You can still use SpacetimeAuth, Auth0, Clerk, Keycloak, or another OIDC provider directly. The broker pattern is for applications that want an app server to be the policy boundary before SpacetimeDB sees a token.

## Claim Contract

SpacetimeDB computes the caller's `Identity` from the token's `iss` and `sub` claims. Keep both stable. Do not use an email address as `sub`, because users can change email addresses.

Recommended claims:

| Claim | Purpose |
| --- | --- |
| `iss` | Stable issuer URL for your application-owned token broker. |
| `sub` | Stable actor ID, such as an application user ID or robot ID. |
| `aud` | Audience for the SpacetimeDB database, app, or resource. |
| `exp` | Short expiration time. Prefer minutes, not days. |
| `iat` | Issued-at timestamp. |
| `token_type` | Distinguishes SpacetimeDB access tokens from web sessions or other tokens. |
| `sid` | Application session ID, useful for audit and revocation checks. |
| `tenant_id` | Active tenant or organization context selected by the app server. |
| `actor_ref` | Application-level actor reference for logs and identity mapping. |
| `scope` or `perms` | Compact permission hints. Do not make these the only source of truth for mutable permissions. |
| `membership_version` | Optional version number that lets module code reject stale membership claims. |

Store mutable authorization in SpacetimeDB tables. Roles, tenant membership, impersonation grants, API-key grants, and revocation-sensitive state should not live only in long-lived JWT claims.

## Token Issuer

SpacetimeDB validates OIDC tokens by reading the issuer's `.well-known/openid-configuration` and then fetching the `jwks_uri` from that metadata. The issuer URL in the discovery document must match the token's `iss` claim.

For local development, the discovery document can be minimal:

```json
{
  "issuer": "https://app.example.com/spacetime-auth",
  "jwks_uri": "https://app.example.com/spacetime-auth/jwks.json"
}
```

Use asymmetric signing keys such as ES256 or RS256 for app-owned issuers so SpacetimeDB can verify tokens through JWKS without sharing a secret. Rotate keys by publishing both the old and new public keys during the overlap period and setting a `kid` header on issued tokens.

## Mint a Short-Lived Token

The exact implementation depends on your web framework and auth library. The broker route should do the same work regardless of provider:

1. Verify the browser session, API key, or service credential.
2. Resolve the active tenant or organization.
3. Check that the actor is allowed to access that tenant.
4. Sign a short-lived token for SpacetimeDB.
5. Return the token to a direct client, or keep it server-side for a gateway connection.

Example using `jose`:

```typescript
import { SignJWT, importPKCS8 } from 'jose';

const issuer = 'https://app.example.com/spacetime-auth';
const audience = 'spacetimedb:my-database';

const privateKey = await importPKCS8(
  process.env.SPACETIME_JWT_PRIVATE_KEY!,
  'ES256'
);

export async function mintSpacetimeToken(input: {
  readonly actorId: string;
  readonly sessionId: string;
  readonly tenantId: string;
  readonly membershipVersion: number;
  readonly scopes: readonly string[];
}) {
  return new SignJWT({
    token_type: 'spacetime-access',
    sid: input.sessionId,
    tenant_id: input.tenantId,
    actor_ref: `user:${input.actorId}`,
    membership_version: input.membershipVersion,
    scope: input.scopes,
  })
    .setProtectedHeader({
      alg: 'ES256',
      kid: process.env.SPACETIME_JWT_KEY_ID!,
    })
    .setIssuer(issuer)
    .setSubject(input.actorId)
    .setAudience(audience)
    .setIssuedAt()
    .setExpirationTime('5m')
    .sign(privateKey);
}
```

If the browser connects directly to SpacetimeDB, return this token only after verifying the browser session. If an application server connects to SpacetimeDB as a gateway, keep the token server-side and use it when constructing the generated SDK connection.

## Validate Claims in Module Code

SpacetimeDB validates token signatures and derives the connection identity. Your module should still enforce the issuer, audience, token type, tenant, and mutable authorization rules that your application expects.

The reducer snippet below assumes your module has already defined `spacetimedb`, `t`, and the `note` table.

```typescript
import { SenderError, type ReducerCtx } from 'spacetimedb/server';

const EXPECTED_ISSUER = 'https://app.example.com/spacetime-auth';
const EXPECTED_AUDIENCE = 'spacetimedb:my-database';

function requireAppToken(ctx: ReducerCtx<any>) {
  const jwt = ctx.senderAuth.jwt;
  if (jwt == null) {
    throw new SenderError('Authentication required');
  }

  if (jwt.issuer !== EXPECTED_ISSUER) {
    throw new SenderError('Invalid issuer');
  }

  if (!jwt.audience.includes(EXPECTED_AUDIENCE)) {
    throw new SenderError('Invalid audience');
  }

  if (jwt.fullPayload['token_type'] !== 'spacetime-access') {
    throw new SenderError('Invalid token type');
  }

  const tenantId = jwt.fullPayload['tenant_id'];
  if (typeof tenantId !== 'string' || tenantId.length === 0) {
    throw new SenderError('Tenant required');
  }

  return {
    actor: ctx.sender,
    subject: jwt.subject,
    tenantId,
    sessionId:
      typeof jwt.fullPayload['sid'] === 'string'
        ? jwt.fullPayload['sid']
        : undefined,
  };
}

export const create_note = spacetimedb.reducer(
  { body: t.string() },
  (ctx, { body }) => {
    const auth = requireAppToken(ctx);

    // Check mutable authorization in private tables here. For example:
    // - actor identity mapping
    // - tenant membership
    // - role permissions
    // - session or API-key revocation state

    ctx.db.note.insert({
      id: 0n,
      tenantId: auth.tenantId,
      owner: auth.actor,
      body,
      createdAt: ctx.timestamp,
    });
  }
);
```

Checking the audience in module code is important. A valid token from the same issuer may have been issued for another application or resource.

## Identity Mapping

Changing `iss` or `sub` changes the SpacetimeDB `Identity`. If you migrate from one auth provider to another, or from direct provider tokens to an app-owned broker, plan for that identity change.

A common approach is to keep a private identity mapping table:

```text
actor_identity_map
  app_actor_id
  issuer
  subject
  spacetimedb_identity
  created_at
  retired_at
```

Use the table to link old issuer/subject pairs, new broker-issued pairs, robot actors, and application users. During migrations, accept both old and new issuers while your application backfills the mapping and audits active traffic.

## Browser Direct vs Server Gateway

There are two common ways to use brokered tokens:

| Topology | Token location | Reducer `ctx.sender` |
| --- | --- | --- |
| Browser connects directly | Browser receives a short-lived SpacetimeDB token after the app session is verified. | User or robot identity from the token. |
| Server-side gateway | Application server keeps the token and connects with the generated SDK. | Identity represented by the gateway connection token. |

If a gateway uses a user-scoped token, reducers see that user as `ctx.sender`. If the gateway uses a service or robot token, reducers see the service identity and should receive any effective actor as trusted, server-derived input.

## Checklist

- Use a stable `iss` and `sub`; do not use emails as subjects.
- Publish OIDC discovery metadata and JWKS for the broker issuer.
- Use short expirations for SpacetimeDB tokens.
- Set and validate `aud`.
- Add a `token_type` claim so module code can distinguish SpacetimeDB access tokens from other tokens.
- Keep mutable roles, tenant membership, impersonation grants, and API-key grants in tables.
- Record audit events with `ctx.sender`, issuer, subject, tenant, session ID, reducer name, and relevant resource IDs.
- Rotate signing keys with overlapping JWKS publication and `kid` headers.
- Plan identity migrations before changing issuer or subject values.

## Related Docs

- [Using Auth Claims](./00500-usage.md)
- [Auth0](./00200-Auth0.md)
- [Clerk](./00300-Clerk.md)
- [Table Access Permissions](../00300-tables/00400-access-permissions.md)
