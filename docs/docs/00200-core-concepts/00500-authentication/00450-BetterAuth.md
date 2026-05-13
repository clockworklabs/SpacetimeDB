---
title: Better Auth
slug: /authentication/better-auth
---

[Better Auth](https://www.better-auth.com/) is a TypeScript authentication
framework that can manage application sessions, organizations, API keys, JWTs,
and OAuth 2.1/OIDC provider flows. SpacetimeDB can use Better Auth when Better
Auth issues an OIDC-compatible JWT with a stable `iss`, a stable `sub`, an
`aud` value for your SpacetimeDB module, and a JWKS endpoint that SpacetimeDB
can use to verify the token signature.

This guide focuses on the SpacetimeDB integration choices. It assumes your
Better Auth app already handles sign-in, session cookies, organization
membership, and any application-specific authorization checks.

## Choose an integration

| Pattern | Use when | Better Auth pieces | SpacetimeDB token |
| --- | --- | --- | --- |
| Session broker | You own the web app and want your server to decide when a user, organization member, or API key may connect to SpacetimeDB. | Better Auth sessions, `jwt`, optional `organization`, optional `apiKey`. | A short-lived JWT minted by your app server. |
| OAuth provider | You want OAuth/OIDC clients, native apps, service clients, or MCP-style integrations to request tokens for SpacetimeDB as a protected resource. | `@better-auth/oauth-provider` plus the Better Auth `jwt` plugin. | A JWT access token with `aud` set from the requested `resource`. |
| API-key broker | You need robot or integration credentials. | `@better-auth/api-key` verified by your app server. | A short-lived JWT minted after API-key validation. |

In every pattern, treat the JWT as authentication input, not as the complete
authorization system. Reducers should still verify the issuer, audience, token
type, tenant or organization claim, and any module-local authorization state
before accepting writes.

::::warning
SpacetimeDB verifies JWTs through OIDC/JWKS data. Opaque access tokens cannot be
validated this way. If you use Better Auth OAuth Provider mode, keep the Better
Auth `jwt` plugin enabled and make clients request a valid `resource` value for
your SpacetimeDB audience so the returned access token is JWT-formatted.
::::

## Session broker mode

Session broker mode is often the simplest fit for a first-party web application.
The browser authenticates with Better Auth as usual, your server verifies the
Better Auth session and organization membership, and then your server returns a
short-lived JWT that the SpacetimeDB client uses as its connection token.

Configure Better Auth's `jwt` plugin with an issuer, audience, expiration, JWKS
settings, and compact custom claims for SpacetimeDB:

```ts title="auth.ts"
import { apiKey } from "@better-auth/api-key";
import { betterAuth } from "better-auth";
import { jwt, organization } from "better-auth/plugins";

export const auth = betterAuth({
  plugins: [
    organization(),
    apiKey(),
    jwt({
      jwks: {
        jwksPath: "/.well-known/spacetime-jwks.json",
        keyPairConfig: {
          alg: "ES256",
        },
      },
      jwt: {
        issuer: "https://app.example.com/spacetime-auth",
        audience: "spacetimedb:my-module",
        expirationTime: "5m",
        getSubject: session => session.user.id,
        definePayload: ({ user, session }) => ({
          token_type: "spacetime-access",
          sid: session.session.id,
          actor_ref: `user:${user.id}`,
          tenant_id: session.session.activeOrganizationId,
        }),
      },
    }),
  ],
});
```

Use an asymmetric signing algorithm such as `ES256` or `RS256` for JWKS-backed
tokens. Better Auth can also generate EdDSA keys, but do not assume EdDSA tokens
will be accepted by every SpacetimeDB deployment unless that support is
documented for the version you run.

Your broker endpoint should perform the authorization checks that are too
dynamic to encode globally in `definePayload`. For example, require an active
organization, call Better Auth's organization permission APIs when needed, and
only then return the short-lived JWT.

```ts title="routes/spacetime-token.ts"
import { auth } from "./auth";

export async function GET(request: Request) {
  const headers = request.headers;
  const session = await auth.api.getSession({ headers });

  if (!session) {
    return new Response("Unauthorized", { status: 401 });
  }

  const tenantId = session.session.activeOrganizationId;
  if (!tenantId) {
    return new Response("Select an organization first", { status: 403 });
  }

  const canConnect = await auth.api.hasPermission({
    headers,
    body: {
      permissions: {
        spacetime: ["connect"],
      },
    },
  });

  if (!canConnect.success) {
    return new Response("Forbidden", { status: 403 });
  }

  // Call Better Auth's JWT token endpoint or your own signer here, using the
  // issuer, audience, subject, expiration, and claims shown above.
  const token = await issueSpacetimeToken({ session, tenantId });

  return Response.json({ token, expires_in: 300 });
}
```

Keep these broker tokens short-lived. Better Auth remains the source of truth
for sessions, organizations, and API keys; the JWT is only a connection
credential that lets SpacetimeDB derive identity and inspect claims.

## OAuth provider mode

OAuth provider mode is a good fit when another client needs a standards-based
authorization flow before connecting to SpacetimeDB. Better Auth's OAuth
Provider plugin can issue JWT access tokens for a requested resource when the
Better Auth `jwt` plugin is enabled.

```ts title="auth.ts"
import { betterAuth } from "better-auth";
import { jwt, organization } from "better-auth/plugins";
import { oauthProvider } from "@better-auth/oauth-provider";

export const auth = betterAuth({
  disabledPaths: ["/token"],
  plugins: [
    organization(),
    jwt({
      disableSettingJwtHeader: true,
      jwks: {
        keyPairConfig: {
          alg: "ES256",
        },
      },
    }),
    oauthProvider({
      loginPage: "/sign-in",
      consentPage: "/oauth/consent",
      validAudiences: ["spacetimedb:my-module"],
      scopes: [
        "openid",
        "profile",
        "email",
        "offline_access",
        "spacetime:connect",
        "spacetime:write",
      ],
      customAccessTokenClaims: ({ user, scopes, referenceId, resource }) => ({
        token_type: "spacetime-access",
        actor_ref: user ? `user:${user.id}` : "client",
        tenant_id: referenceId,
        resource,
        scope: scopes.join(" "),
      }),
    }),
  ],
});
```

Clients should request an access token with a `resource` matching one of
`validAudiences`, for example `spacetimedb:my-module`. The resulting JWT should
include that value in `aud`. SpacetimeDB can then verify the token through the
issuer's OIDC discovery document and JWKS URI.

For client-credentials or other non-user flows, `customAccessTokenClaims` may
not receive a user. Use the OAuth client metadata and Better Auth's stored
client configuration to decide which actor, tenant, and scopes should appear in
the SpacetimeDB token.

If your issuer is `https://app.example.com/api/auth`, publish OIDC discovery at
`https://app.example.com/api/auth/.well-known/openid-configuration` and ensure
the discovery document's `jwks_uri` points to Better Auth's JWKS endpoint.
SpacetimeDB follows the issuer metadata to find the verification keys.

The older Better Auth OIDC Provider plugin is being superseded by the OAuth
Provider plugin. Prefer OAuth Provider mode for new integrations.

## API keys and service actors

Better Auth API keys are useful for robots, CLIs, scheduled jobs, and external
integrations. Do not send a long-lived Better Auth API key directly to
SpacetimeDB. Instead, validate the API key on your application server and mint a
short-lived SpacetimeDB JWT after checking the key's owner, organization,
expiration, rate limits, and permissions.

Use claims that make the actor explicit:

```json
{
  "iss": "https://app.example.com/spacetime-auth",
  "sub": "api-key:ak_123",
  "aud": "spacetimedb:my-module",
  "token_type": "spacetime-access",
  "actor_ref": "api-key:ak_123",
  "tenant_id": "org_123",
  "scope": "spacetime:connect spacetime:write"
}
```

If an API key acts on behalf of a user or organization, record that delegation in
custom claims and in your SpacetimeDB tables so reducers can produce useful
audit records.

## Check claims in your module

SpacetimeDB derives the connection identity from `iss` and `sub`. That identity
is stable, but it is not a complete authorization decision. Check the remaining
claims when the client connects, and copy the minimal authorization state you
need into module tables.

```typescript title="server/auth.ts"
import { SenderError } from "spacetimedb/server";

const TRUSTED_ISSUER = "https://app.example.com/spacetime-auth";
const SPACETIME_AUDIENCE = "spacetimedb:my-module";

function stringClaim(
  payload: Record<string, unknown>,
  name: string
): string | undefined {
  const value = payload[name];
  return typeof value === "string" ? value : undefined;
}

export const onConnect = spacetimedb.clientConnected(ctx => {
  const jwt = ctx.senderAuth.jwt;

  if (jwt == null) {
    throw new SenderError("Unauthorized: JWT is required to connect");
  }

  if (jwt.issuer !== TRUSTED_ISSUER) {
    throw new SenderError("Unauthorized: invalid issuer");
  }

  if (!jwt.audience.includes(SPACETIME_AUDIENCE)) {
    throw new SenderError("Unauthorized: invalid audience");
  }

  if (stringClaim(jwt.fullPayload, "token_type") !== "spacetime-access") {
    throw new SenderError("Unauthorized: invalid token type");
  }

  const tenantId = stringClaim(jwt.fullPayload, "tenant_id");
  if (tenantId == null) {
    throw new SenderError("Unauthorized: tenant claim is required");
  }

  // Store or refresh a connection/session row that reducers can use for
  // module-local authorization decisions.
});
```

For multi-tenant apps, prefer compact stable identifiers such as `tenant_id`,
`actor_ref`, `sid`, and coarse `scope` values in the JWT. Resolve mutable
membership, roles, billing status, and feature flags in your app server before
minting the token, or mirror the subset that reducers need into SpacetimeDB
tables.

## Checklist

- Publish an OIDC discovery document whose `issuer` exactly matches the JWT
  `iss` claim.
- Publish a JWKS endpoint with keys for the JWT signing algorithm you choose.
- Use `ES256` or `RS256` unless your SpacetimeDB deployment documents support
  for another JWT algorithm.
- Set `aud` to a SpacetimeDB-specific audience and check it in your module.
- Keep SpacetimeDB access tokens short-lived.
- Do not use opaque Better Auth access tokens as SpacetimeDB connection tokens.
- Keep Better Auth sessions, organization membership, and API keys as the source
  of truth for authorization outside the module.
