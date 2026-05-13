---
title: Multi-Tenant Authorization Cookbook
---

Authentication answers who connected. Authorization answers what that actor can do right now. For multi-tenant SaaS applications, keep those concerns separate: use JWT claims to identify the caller and route the request, then use SpacetimeDB tables, reducers, and views to enforce mutable tenant policy.

The pattern in this cookbook works with SpacetimeAuth, Better Auth, Auth0, Clerk, Keycloak, custom OIDC, service accounts, and server-side gateways. The identity provider can change; the module authorization model should remain explicit and auditable.

## What belongs where

Keep JWT claims small and stable. Put mutable authorization state in tables.

| Concern | JWT claim | SpacetimeDB table |
| --- | --- | --- |
| Stable actor identity | `iss`, `sub`, `actor_ref` | `actor`, `actor_identity_link` |
| Active tenant routing | `tenant_id` | `tenant`, `membership` |
| Token purpose | `token_type` | Optional audit record |
| Compact permission hints | `scope` or `perms` | `role`, `role_permission`, `membership`, `api_key_grant` |
| Session freshness | `sid`, `membership_version`, `grant_version` | `session_context`, `membership`, `api_key_grant`, `impersonation_grant` |
| Impersonation or delegation | `act` | `impersonation_grant`, `delegation_grant`, `audit_event` |
| Revocation-sensitive state | Avoid long-lived claims | Tables with status, version, expiry, and audit history |

JWTs are useful for fast lookup and routing. Tables are where you should keep tenant membership, role assignments, permission grants, API-key grants, impersonation grants, customer-specific policy, and revocation state.

## Table model

Start with private authorization tables. Expose only safe projections through views.

```typescript title="module.ts"
import { schema, table, t } from "spacetimedb/server";

const actor = table(
  { name: "actor" },
  {
    actorId: t.string().primaryKey(),
    actorKind: t.string().index("btree"), // user, robot, service
    displayName: t.string(),
    status: t.string(), // active, suspended, retired
    createdAtMillis: t.u64(),
  }
);

const actorIdentityLink = table(
  { name: "actor_identity_link" },
  {
    identityLinkId: t.string().primaryKey(),
    spacetimeIdentity: t.identity().index("btree"),
    issuer: t.string().index("btree"),
    subject: t.string().index("btree"),
    actorId: t.string().index("btree"),
    status: t.string(),
  }
);

const tenant = table(
  { name: "tenant" },
  {
    tenantId: t.string().primaryKey(),
    displayName: t.string(),
    status: t.string(),
  }
);

const membership = table(
  { name: "membership" },
  {
    membershipId: t.string().primaryKey(),
    tenantId: t.string().index("btree"),
    actorId: t.string().index("btree"),
    roleId: t.string().index("btree"),
    status: t.string(),
    version: t.u64(),
  }
);

const role = table(
  { name: "role" },
  {
    roleId: t.string().primaryKey(),
    tenantId: t.string().index("btree"),
    name: t.string(),
    status: t.string(),
  }
);

const rolePermission = table(
  { name: "role_permission" },
  {
    rolePermissionId: t.string().primaryKey(),
    roleId: t.string().index("btree"),
    permission: t.string().index("btree"),
  }
);

const sessionContext = table(
  { name: "session_context" },
  {
    sessionId: t.string().primaryKey(),
    actorId: t.string().index("btree"),
    tenantId: t.string().index("btree"),
    status: t.string(),
    membershipVersion: t.u64(),
    expiresAtMillis: t.u64(),
  }
);

const impersonationGrant = table(
  { name: "impersonation_grant" },
  {
    impersonationGrantId: t.string().primaryKey(),
    tenantId: t.string().index("btree"),
    adminActorId: t.string().index("btree"),
    targetActorId: t.string().index("btree"),
    scope: t.string().index("btree"),
    status: t.string(),
    expiresAtMillis: t.u64(),
  }
);

const apiKeyGrant = table(
  { name: "api_key_grant" },
  {
    apiKeyGrantId: t.string().primaryKey(),
    tenantId: t.string().index("btree"),
    robotActorId: t.string().index("btree"),
    permission: t.string().index("btree"),
    status: t.string(),
    version: t.u64(),
  }
);

const auditEvent = table(
  { name: "audit_event" },
  {
    auditEventId: t.u64().primaryKey().autoInc(),
    tenantId: t.string().index("btree"),
    actorId: t.string().index("btree"),
    actorKind: t.string(),
    action: t.string().index("btree"),
    resourceRef: t.string(),
    occurredAtMillis: t.u64(),
  }
);

export default schema({
  actor,
  actorIdentityLink,
  tenant,
  membership,
  role,
  rolePermission,
  sessionContext,
  impersonationGrant,
  apiKeyGrant,
  auditEvent,
});
```

These tables are intentionally generic. A real app will add domain tables such as `document`, `project`, `invoice`, `workflow`, or `customer_portal_surface`. Keep those domain rows tenant-scoped and use indexes for every lookup you need from reducers or views.

## Identity linking

SpacetimeDB identities are derived from the OIDC issuer and subject. If you switch identity providers, the same human can arrive with a different issuer/subject pair. Avoid hard-coding authorization directly to one provider's raw subject.

Use `actor_identity_link` to connect trusted issuer/subject pairs and their derived SpacetimeDB identities to your application actor:

```text
issuer=https://auth.example.com
subject=user_123
spacetimeIdentity=0x...
actorId=actor_123

issuer=https://login.example-customer.com
subject=00u4abcd
spacetimeIdentity=0x...
actorId=actor_123
```

Email can help during account linking, but it should not be the durable identity key. Link only after your application has verified the provider assertion, tenant policy, and any migration or account-linking requirements.

## Reducer guard

Reducer guards should verify the token, resolve the actor, resolve the active tenant, check membership or robot grants, and fail closed before modifying domain data.

```typescript title="module.ts"
import { ReducerCtx, SenderError } from "spacetimedb/server";

type AppClaims = {
  token_type?: string;
  actor_ref?: string;
  tenant_id?: string;
  sid?: string;
  membership_version?: number;
  grant_version?: number;
  scope?: string[];
};

type AuthorizedActor = {
  actorId: string;
  actorKind: string;
  tenantId: string;
};

function requireTenantPermission(
  ctx: ReducerCtx<any>,
  permission: string
): AuthorizedActor {
  const jwt = ctx.senderAuth.jwt;
  if (jwt == null) {
    throw new SenderError("Authentication required");
  }
  if (jwt.issuer !== "https://app.example.com/auth") {
    throw new SenderError("Invalid issuer");
  }
  if (!jwt.audience.includes("spacetimedb:app-prod")) {
    throw new SenderError("Invalid audience");
  }

  const claims = jwt.fullPayload as AppClaims;
  if (claims.token_type !== "spacetime-access") {
    throw new SenderError("Invalid token type");
  }
  if (claims.actor_ref == null || claims.tenant_id == null) {
    throw new SenderError("Missing actor context");
  }

  const actorRow = ctx.db.actor.actorId.find(claims.actor_ref);
  if (actorRow == null || actorRow.status !== "active") {
    throw new SenderError("Actor is not active");
  }

  if (actorRow.actorKind === "robot") {
    requireRobotGrant(ctx, actorRow.actorId, claims.tenant_id, permission, claims);
  } else {
    requireHumanMembership(ctx, actorRow.actorId, claims.tenant_id, permission, claims);
  }

  return {
    actorId: actorRow.actorId,
    actorKind: actorRow.actorKind,
    tenantId: claims.tenant_id,
  };
}

function requireHumanMembership(
  ctx: ReducerCtx<any>,
  actorId: string,
  tenantId: string,
  permission: string,
  claims: AppClaims
) {
  const memberships = Array.from(ctx.db.membership.actorId.filter(actorId));
  const activeMembership = memberships.find(row =>
    row.tenantId === tenantId && row.status === "active"
  );

  if (activeMembership == null) {
    throw new SenderError("Tenant membership required");
  }
  if (activeMembership.version !== BigInt(claims.membership_version ?? -1)) {
    throw new SenderError("Stale membership");
  }
  if (!roleHasPermission(ctx, activeMembership.roleId, permission)) {
    throw new SenderError("Missing permission");
  }
}

function requireRobotGrant(
  ctx: ReducerCtx<any>,
  robotActorId: string,
  tenantId: string,
  permission: string,
  claims: AppClaims
) {
  const grants = Array.from(ctx.db.apiKeyGrant.robotActorId.filter(robotActorId));
  const activeGrant = grants.find(row =>
    row.tenantId === tenantId &&
    row.permission === permission &&
    row.status === "active"
  );

  if (activeGrant == null) {
    throw new SenderError("Robot grant required");
  }
  if (activeGrant.version !== BigInt(claims.grant_version ?? -1)) {
    throw new SenderError("Stale robot grant");
  }
}

function roleHasPermission(ctx: ReducerCtx<any>, roleId: string, permission: string) {
  return Array.from(ctx.db.rolePermission.roleId.filter(roleId))
    .some(row => row.permission === permission);
}
```

This guard uses JWT claims only to find candidate rows quickly. The final authorization decision comes from module state.

## Domain reducer

Every write reducer should derive tenant and actor context from trusted state, then write both the domain row and audit row.

```typescript title="module.ts"
const document = table(
  { name: "document" },
  {
    documentId: t.string().primaryKey(),
    tenantId: t.string().index("btree"),
    title: t.string(),
    body: t.string(),
    updatedByActorId: t.string().index("btree"),
    updatedAtMillis: t.u64(),
  }
);

export const updateDocument = spacetimedb.reducer(
  {
    documentId: t.string(),
    title: t.string(),
    body: t.string(),
  },
  (ctx, { documentId, title, body }) => {
    const auth = requireTenantPermission(ctx, "document:update");
    const existing = ctx.db.document.documentId.find(documentId);

    if (existing == null || existing.tenantId !== auth.tenantId) {
      throw new SenderError("Document not found");
    }

    ctx.db.document.documentId.update({
      ...existing,
      title,
      body,
      updatedByActorId: auth.actorId,
      updatedAtMillis: currentTimeMillis(),
    });

    ctx.db.auditEvent.insert({
      tenantId: auth.tenantId,
      actorId: auth.actorId,
      actorKind: auth.actorKind,
      action: "document:update",
      resourceRef: documentId,
      occurredAtMillis: currentTimeMillis(),
    });
  }
);
```

Do not accept `tenantId`, `actorId`, `roleId`, or impersonation fields from browser JSON unless the server has already verified and narrowed them. Even then, reducers should check module tables before writing.

## Sender-filtered views

Views let clients subscribe to safe projections over private tables. Use `ViewContext` and indexes to return only rows the caller can see.

```typescript title="module.ts"
const publicMembership = t.row("PublicMembership", {
  tenantId: t.string(),
  roleId: t.string(),
});

export const my_memberships = spacetimedb.view(
  { name: "my_memberships", public: true },
  t.array(publicMembership),
  (ctx) => {
    const links = Array.from(ctx.db.actorIdentityLink.spacetimeIdentity.filter(ctx.sender));
    const actorLink = links.find(row => row.status === "active");

    if (actorLink == null) {
      return [];
    }

    return Array.from(ctx.db.membership.actorId.filter(actorLink.actorId))
      .filter(row => row.status === "active")
      .map(row => ({
        tenantId: row.tenantId,
        roleId: row.roleId,
      }));
  }
);
```

For large applications, avoid scans in views. Add indexes that match your view lookups, or maintain a projection table keyed by the sender identity or actor ID.

Example tenant-filtered document projection:

```typescript title="module.ts"
const publicDocument = t.row("PublicDocument", {
  documentId: t.string(),
  tenantId: t.string(),
  title: t.string(),
  body: t.string(),
});

export const visible_documents = spacetimedb.view(
  { name: "visible_documents", public: true },
  t.array(publicDocument),
  (ctx) => {
    const actor = resolveActorForSender(ctx);
    if (actor == null) {
      return [];
    }

    const memberships = Array.from(ctx.db.membership.actorId.filter(actor.actorId))
      .filter(row => row.status === "active");

    const out: Array<{
      documentId: string;
      tenantId: string;
      title: string;
      body: string;
    }> = [];

    for (const member of memberships) {
      for (const row of ctx.db.document.tenantId.filter(member.tenantId)) {
        out.push({
          documentId: row.documentId,
          tenantId: row.tenantId,
          title: row.title,
          body: row.body,
        });
      }
    }

    return out;
  }
);
```

Keep sensitive columns out of public view return types. If a browser does not need a field, do not include it in the view.

## Impersonation

Admin impersonation should be explicit, time-bounded, scoped, and audited. A support admin should not become the target user invisibly.

Recommended pattern:

- The app backend authenticates the admin and verifies support permission.
- The backend creates or verifies an `impersonation_grant`.
- The SpacetimeDB access token carries the admin actor as the caller and may include an `act` claim naming the target actor.
- Reducers verify the `impersonation_grant` table before allowing impersonated actions.
- Audit events record both admin actor and target actor.

Do not authorize impersonation from a browser-supplied `targetActorId` alone. The reducer should require an active grant matching admin actor, target actor, tenant, scope, and expiry.

## API keys and robots

API keys should be validated by the application backend. The module should see a short-lived JWT for a stable robot actor plus non-secret references such as `api_key_ref` and `integration_ref`.

In the module:

- Store `api_key_grant` rows with robot actor, tenant, permission, status, and version.
- Reject stale tokens when the grant version changes.
- Keep raw API keys, webhook secrets, client secrets, and refresh tokens out of module tables.
- Audit robot writes separately from human writes.
- Use delegated actor grants when a robot acts on behalf of a human.

This keeps long-lived credential risk in the application auth layer and lets SpacetimeDB enforce domain authorization from auditable state.

## Gateway considerations

For browser apps that use an application server gateway, keep the boundary clear:

- The browser authenticates to the app server with a web session.
- The app server resolves tenant, actor, membership, and permissions.
- The gateway calls reducers with a user-scoped, robot-scoped, or service-scoped SpacetimeDB token.
- Reducers still verify module-local state.
- SSE streams or subscriptions expose only authorized views and projections.

If a membership, role, API-key grant, or impersonation grant changes, increment the relevant version and revoke or narrow active browser streams. Short token lifetimes make stale authorization windows smaller, but reducers should still check the versioned rows.

## Checklist

- Use stable actor IDs and identity-link rows instead of provider-specific subjects everywhere.
- Keep tenant membership, role grants, API-key grants, impersonation grants, and revocation state in tables.
- Keep JWT claims compact and short-lived.
- Check issuer, audience, token type, actor status, tenant status, membership status, permission, and grant version in reducers.
- Store audit events for writes, admin changes, impersonation, API-key use, and authorization failures that matter operationally.
- Use private tables for authorization state and public views for safe projections.
- Design indexes for every reducer and view lookup.
- Keep raw secrets and provider assertions outside SpacetimeDB tables.
- Fail closed on missing claims, missing table rows, stale versions, suspended actors, suspended tenants, and wrong tenant context.

## Related docs

- [Authentication](../00500-authentication.md)
- [Using Auth Claims](./00500-usage.md)
- [Views](../00200-functions/00500-views.md)
- [Access Permissions](../00300-tables/00400-access-permissions.md)
