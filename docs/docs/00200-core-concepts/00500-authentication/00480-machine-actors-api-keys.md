---
title: Machine Actors, API Keys, and Integrations
---

Human users are not the only callers that need to write to a SpacetimeDB module. Production applications often also have webhooks, scheduled jobs, importers, MCP servers, AI agents, customer-built integrations, and internal worker processes. Treat those callers as machine actors, not as browser users.

The safest pattern is to validate long-lived credentials in your application backend, then exchange them for short-lived OIDC-compatible JWTs that SpacetimeDB can verify:

```text
Integration request
  | x-api-key, OAuth client credential, webhook signature, or job credential
  v
Application backend
  | validate long-lived credential
  | derive tenant, robot actor, permissions, and delegation
  | mint short-lived SpacetimeDB JWT
  v
SpacetimeDB SDK or gateway connection
  | connect or call reducers with the short-lived JWT
  v
SpacetimeDB module
  | verify issuer, audience, subject shape, claims, and grant tables
  | write audit records
```

Do not send customer API keys, integration secrets, webhook secrets, or refresh tokens to SpacetimeDB reducers. Reducers should receive either a verified SpacetimeDB identity through `ctx.sender`, or explicit reducer arguments derived by trusted server code.

## Actor types

Use stable subjects for each actor type. Do not use email addresses, display names, API key prefixes, or provider-specific mutable names as durable subjects.

| Caller | Recommended `sub` | Notes |
| --- | --- | --- |
| Human user | `user:user_123` | A browser user or operator. The user may sign in through SpacetimeAuth, Better Auth, Auth0, Clerk, Keycloak, or another OIDC provider. |
| Robot actor | `robot:integration_123` | A non-human actor owned by an organization, tenant, service, or integration. |
| Scheduled job | `robot:job_123` | A platform-owned or tenant-owned job. Keep it separate from the human who configured it. |
| Webhook receiver | `robot:webhook_123` | A server-side actor created after validating the webhook signature or provider credential. |
| Delegated robot | `robot:integration_123` plus `act` claim | A robot acting with an explicit delegation from a human, admin, or customer-owned app. |

The exact prefix is an application convention. The important rules are that subjects are stable, non-secret, globally unique inside the issuer, and easy for reducers and audit tooling to classify.

## Token exchange flow

API keys and integration credentials are usually long-lived. SpacetimeDB access tokens should be short-lived. The app backend is the broker between those two worlds:

1. Read the integration credential from a header, webhook signature, mTLS identity, job secret, or OAuth client assertion.
2. Validate it in the application auth system.
3. Resolve the owning tenant, organization, robot actor, grant version, and allowed scopes.
4. Mint a short-lived JWT for SpacetimeDB with a stable machine subject.
5. Connect the SDK or call reducers with that short-lived token.
6. In reducers, verify issuer, audience, subject shape, token type, and module-local grant tables.

Example claim shape:

| Claim | Example | Purpose |
| --- | --- | --- |
| `iss` | `https://app.example.com/auth` | Your app-owned issuer, SpacetimeAuth issuer, or another trusted OIDC issuer. |
| `sub` | `robot:integration_123` | Stable machine actor subject. |
| `aud` | `spacetimedb:documents-prod` | The intended SpacetimeDB database or resource. |
| `token_type` | `spacetime-access` | Distinguishes database access tokens from web sessions and raw API keys. |
| `tenant_id` | `tenant_123` | Routing hint for the active tenant. Re-check membership and grants in tables. |
| `actor_ref` | `robot_actor_123` | Stable app actor reference for audit and lookups. |
| `api_key_ref` | `api_key_123` | Non-secret reference to the key record that was verified. |
| `integration_ref` | `integration_123` | Non-secret reference to the integration or client application. |
| `scope` or `perms` | `["document:write"]` | Compact permission hints. Mutable authorization should still live in tables. |
| `grant_version` | `42` | Lets the module reject stale tokens after grants change. |
| `act` | `{ "sub": "user:user_123" }` | Optional delegated actor context when a robot acts on behalf of a human. |
| `jti` | `jwt_123` | Unique token ID for replay diagnostics or revocation checks. |
| `exp` | short expiry | Keep the SpacetimeDB token short-lived. |

JWT claims should be small and stable. Store mutable roles, limits, customer-specific grants, revocation status, impersonation/delegation grants, and integration lifecycle in SpacetimeDB tables or in the application backend.

## Better Auth API key broker

Better Auth's API Key plugin can create, manage, and verify API keys. It supports expiration, remaining-use counters, refill behavior, metadata, prefixes, multiple configurations, secondary storage, built-in rate limiting, permissions, and organization-owned keys.

That makes Better Auth a good application-side API key authority, but the key itself should still stop at the application server. The server verifies the key, derives a robot actor, and mints a short-lived SpacetimeDB token.

```ts title="server/spacetime-token.ts"
import { auth } from "./auth";

type SpacetimeTokenRequest = {
  key: string;
  requestedTenantId: string;
};

export async function mintSpacetimeTokenFromApiKey(request: SpacetimeTokenRequest) {
  const verified = await auth.api.verifyApiKey({
    body: {
      configId: "integrations",
      key: request.key,
      permissions: {
        documents: ["write"],
      },
    },
  });

  if (!verified.valid || verified.key == null) {
    throw new Error("Invalid API key");
  }

  const grant = await resolveIntegrationGrant({
    apiKeyId: verified.key.id,
    referenceId: verified.key.referenceId,
    requestedTenantId: request.requestedTenantId,
  });

  if (grant.status !== "active") {
    throw new Error("Integration grant is not active");
  }

  return signSpacetimeJwt({
    iss: "https://app.example.com/auth",
    sub: `robot:${grant.robotActorId}`,
    aud: "spacetimedb:documents-prod",
    token_type: "spacetime-access",
    tenant_id: grant.tenantId,
    actor_ref: grant.robotActorId,
    api_key_ref: verified.key.id,
    integration_ref: grant.integrationId,
    scope: grant.scopes,
    grant_version: grant.version,
    jti: crypto.randomUUID(),
    exp: Math.floor(Date.now() / 1000) + 60,
  });
}
```

When using organization-owned keys, configure the key system so organization membership and API key permissions are checked before a key can be created, read, updated, deleted, or used. Do not enable API-key-to-session behavior for organization-owned machine keys. Treat machine keys as credentials for robot actors, not as a way to impersonate a human session.

## Suggested module tables

Keep the secret credential store outside SpacetimeDB, but mirror enough non-secret state into your module to authorize reducers and write useful audit records.

```typescript title="module.ts"
import { schema, table, t } from "spacetimedb/server";

const robotActor = table(
  { name: "robot_actor", public: false },
  {
    robotActorId: t.string().primaryKey(),
    tenantId: t.string().index("btree"),
    displayName: t.string(),
    status: t.string(),
    grantVersion: t.u64(),
  }
);

const integrationGrant = table(
  { name: "integration_grant", public: false },
  {
    integrationGrantId: t.string().primaryKey(),
    robotActorId: t.string().index("btree"),
    tenantId: t.string().index("btree"),
    scope: t.string(),
    status: t.string(),
    version: t.u64(),
  }
);

const delegationGrant = table(
  { name: "delegation_grant", public: false },
  {
    delegationGrantId: t.string().primaryKey(),
    robotActorId: t.string().index("btree"),
    humanActorRef: t.string().index("btree"),
    tenantId: t.string().index("btree"),
    scope: t.string(),
    expiresAtMillis: t.u64(),
    status: t.string(),
  }
);

const auditEvent = table(
  { name: "audit_event", public: false },
  {
    auditEventId: t.string().primaryKey(),
    tenantId: t.string().index("btree"),
    actorRef: t.string().index("btree"),
    actorKind: t.string(),
    action: t.string(),
    resourceRef: t.string(),
    occurredAtMillis: t.u64(),
  }
);

export default schema({
  robotActor,
  integrationGrant,
  delegationGrant,
  auditEvent,
});
```

These tables intentionally avoid raw API keys, webhook secrets, refresh tokens, provider assertions, and private key material. Store only references, hashes, lifecycle state, grant versions, and audit-safe metadata.

## Reducer authorization

Reducers should verify both the token and module-local state. A JWT claim can help the reducer find the right records quickly, but it should not be the only authorization source for mutable decisions.

```typescript title="module.ts"
import { ReducerCtx, SenderError } from "spacetimedb/server";

type MachineClaims = {
  token_type?: string;
  tenant_id?: string;
  actor_ref?: string;
  scope?: string[];
  grant_version?: number;
  act?: { sub?: string };
};

function requireMachineActor(ctx: ReducerCtx<any>, scope: string) {
  const jwt = ctx.senderAuth.jwt;
  if (jwt == null) {
    throw new SenderError("Authentication required");
  }
  if (jwt.issuer !== "https://app.example.com/auth") {
    throw new SenderError("Invalid issuer");
  }
  if (!jwt.audience.includes("spacetimedb:documents-prod")) {
    throw new SenderError("Invalid audience");
  }

  const claims = jwt.fullPayload as MachineClaims;
  if (claims.token_type !== "spacetime-access") {
    throw new SenderError("Invalid token type");
  }
  if (typeof jwt.subject !== "string" || !jwt.subject.startsWith("robot:")) {
    throw new SenderError("Machine actor required");
  }
  if (claims.tenant_id == null || claims.actor_ref == null) {
    throw new SenderError("Missing machine actor context");
  }
  if (!Array.isArray(claims.scope) || !claims.scope.includes(scope)) {
    throw new SenderError("Missing required scope");
  }

  const actor = ctx.db.robotActor.robotActorId.find(claims.actor_ref);
  if (actor == null || actor.status !== "active") {
    throw new SenderError("Robot actor is not active");
  }
  if (actor.tenantId !== claims.tenant_id) {
    throw new SenderError("Tenant mismatch");
  }
  if (actor.grantVersion !== BigInt(claims.grant_version ?? -1)) {
    throw new SenderError("Stale machine actor grant");
  }

  return {
    actorRef: actor.robotActorId,
    tenantId: actor.tenantId,
    delegatedSubject: claims.act?.sub,
  };
}

export const importDocument = spacetimedb.reducer(
  (ctx, documentId: string, title: string, body: string) => {
    const actor = requireMachineActor(ctx, "document:write");

    // Write the domain row here, then record an audit event with actor.actorRef.
  }
);
```

The exact table and index names depend on your module. The important point is that reducers fail closed when the token is missing, has the wrong issuer or audience, has a human subject where a robot is required, has stale grants, or no longer matches active module state.

## Delegation

A robot can act in one of three ways:

| Mode | Recommended representation | Use when |
| --- | --- | --- |
| Direct robot | `sub=robot:integration_123` | The integration owns the action, such as a nightly import. |
| Human initiated | `sub=user:user_123` | The user is directly connected or the gateway uses a user-scoped token. |
| Delegated robot | `sub=robot:integration_123` with `act.sub=user:user_123` | A customer app, admin tool, or AI agent performs an action under an explicit user grant. |

For delegated robots, store the delegation grant in a table and verify it in reducers. The `act` claim is useful as a compact hint, but the module should still verify that the delegation is active, scoped to the same tenant, not expired, and allowed for the requested reducer.

Do not let the browser submit `actor_ref`, `tenant_id`, `act`, or impersonation state directly. Those values should be derived by the application server from trusted sessions, API keys, customer app registrations, and delegation grants.

## Gateway topologies

Machine actors work with the same gateway choices as human users:

| Topology | Description | Tradeoff |
| --- | --- | --- |
| Robot-scoped connection | The app opens a SpacetimeDB connection with a robot token. | Reducers see the robot in `ctx.sender`; good for importers and jobs. |
| Service connection plus explicit actor args | The gateway uses one service token and passes effective actor context to reducers. | Fewer WSS connections, but reducers must never trust client-supplied actor args. |
| Hybrid | A service connection handles subscriptions; write paths use robot-scoped or user-scoped tokens when attribution matters. | More moving parts, but clearer audit for important writes. |

If audit trails need native SpacetimeDB identity attribution for each integration, prefer robot-scoped tokens for writes. If you use a shared service connection, make the reducer API explicit about the effective actor and verify that actor from trusted server-side state.

## CLI and CI smoke tests

Keep the broker and gateway callable from command-line scripts. That makes integration behavior testable before a browser, webhook provider, or scheduled job is involved.

```ts title="scripts/smoke-robot-reducer.ts"
import { DbConnection } from "../src/module-bindings";
import { mintSpacetimeTokenFromApiKey } from "../src/server/spacetime-token";

const token = await mintSpacetimeTokenFromApiKey({
  key: mustGetEnv("TEST_INTEGRATION_API_KEY"),
  requestedTenantId: mustGetEnv("TEST_TENANT_ID"),
});

const conn = DbConnection.builder()
  .withUri(mustGetEnv("SPACETIME_URI"))
  .withDatabaseName(mustGetEnv("SPACETIME_DATABASE"))
  .withToken(token)
  .build();

await conn.reducers.importDocument(
  "robot-smoke-test",
  "Robot smoke test",
  `Updated at ${new Date().toISOString()}`
);

conn.disconnect();

function mustGetEnv(name: string) {
  const value = process.env[name];
  if (value == null || value.length === 0) {
    throw new Error(`Missing required environment variable ${name}`);
  }
  return value;
}
```

Useful smoke tests:

- Valid API key mints a short-lived token and can call an allowed reducer.
- Expired, disabled, or over-limit API keys fail before SpacetimeDB is contacted.
- Revoked integration grants fail in reducers even if an unexpired token still exists.
- Stale `grant_version` claims fail after permissions change.
- A robot token cannot call human-only reducers.
- A delegated robot cannot act after its delegation expires.
- Audit events record robot actor, tenant, reducer action, and optional delegated human.

## Security checklist

- Hash long-lived API keys at rest. Never store raw API keys in SpacetimeDB tables.
- Keep API key verification, webhook signature checks, and client-secret validation in the app backend.
- Mint short-lived SpacetimeDB JWTs only after resolving tenant, actor, permissions, and grant version.
- Use stable robot subjects such as `robot:integration_123`.
- Keep mutable authorization in tables. Treat JWT scopes as hints, not as the whole policy.
- Check `iss`, `aud`, `sub`, `token_type`, `tenant_id`, `actor_ref`, and grant freshness in reducers.
- Distinguish direct robot actions from delegated human actions.
- Audit every integration write with actor kind, actor ref, tenant, action, resource, and delegation when present.
- Fail closed on missing grants, suspended actors, retired integrations, stale grant versions, and tenant mismatch.
- Do not enable API-key-to-session behavior unless you have a specific user-owned-key use case and have reviewed the impersonation risk.

## Related docs

- [Authentication](../00500-authentication.md)
- [Using Auth Claims](./00500-usage.md)
- [Better Auth API Key plugin](https://better-auth.com/docs/plugins/api-key)
