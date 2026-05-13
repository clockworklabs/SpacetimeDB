---
title: OIDC Identity Migration
slug: /authentication/oidc-identity-migration
---

SpacetimeDB accepts OpenID Connect (OIDC) and JWT tokens from many identity providers. That makes it possible to move from one provider to another, such as from Keycloak to SpacetimeAuth, Auth0, Clerk, a custom issuer, or an application-owned auth service.

The important migration detail is identity continuity. SpacetimeDB computes a client's `Identity` from the token issuer (`iss`) and subject (`sub`) claims. If either claim changes, the same human user or service actor may connect as a different SpacetimeDB identity.

## What can change

An auth migration may change one or more of these values:

| Change | Example | Effect |
| --- | --- | --- |
| Signing keys rotate, issuer stays the same | The same OIDC issuer publishes new JWKS keys. | Existing SpacetimeDB identities can remain stable if `iss` and `sub` stay the same. |
| Subject format changes | A provider changes from email-like subjects to stable UUID subjects. | SpacetimeDB identities change because `sub` changed. |
| Issuer changes | Moving from `https://keycloak.example.com/realms/app` to `https://auth.example.com`. | SpacetimeDB identities change because `iss` changed. |
| Audience changes | A token is minted for a new client or resource audience. | Identity may stay the same, but reducers should reject tokens whose `aud` is not intended for the module. |
| Provider is replaced by an app-owned broker | The web app validates sessions and mints short-lived SpacetimeDB tokens. | Identity stays stable only if the broker deliberately preserves the accepted `iss` and `sub` pair. |

If you need the exact same SpacetimeDB identity after a migration, keep both `iss` and `sub` unchanged. If you cannot keep them unchanged, plan for linked identities instead of assuming rows keyed by the old `Identity` will automatically belong to the new token.

## Prefer stable application actors

For simple applications, it is common to key user tables directly by `Identity`. That works well when one issuer and subject format will be used for the life of the application.

For applications that may change providers, support enterprise SSO, or federate with multiple identity systems, use a stable application actor ID in addition to the SpacetimeDB identity. The application actor is the durable concept your business logic owns. The OIDC issuer and subject are external credentials that prove which actor is connecting.

A typical model has three layers:

```text
External credential
  iss + sub from Keycloak, SpacetimeAuth, Auth0, Clerk, SAML bridge, or app broker
  |
  v
SpacetimeDB Identity
  ctx.sender derived from the validated token
  |
  v
Application actor
  stable user, service account, tenant member, or integration record
```

During migration, link the old and new external credentials to the same application actor only after a verified migration step. Do not merge by email address alone. Email addresses can change, be recycled, be delegated, or appear in multiple identity systems.

## Example link tables

The exact schema depends on your module, but the useful records are usually:

| Table | Purpose |
| --- | --- |
| `actor` | Stable application actor, user, service account, or integration ID. |
| `actor_identity_link` | Maps one SpacetimeDB `Identity` to the stable actor. |
| `federated_identity_link` | Stores external `issuer` plus `subject` metadata for audit and migration checks. |
| `identity_migration_event` | Records who approved, imported, linked, verified, or retired a mapping. |

Keep sensitive source payloads out of these tables. Store hashes, stable references, issuer strings, subjects, lifecycle state, and audit metadata. Do not store raw ID tokens, refresh tokens, SAML assertions, SCIM bearer tokens, or provider admin credentials in SpacetimeDB unless the module is explicitly designed to protect those secrets.

## Migration phases

### 1. Inventory the old issuer

Before changing tokens, identify the issuer and subject values your current clients use. Also identify the SpacetimeDB rows that are keyed by `Identity` or that store identity values in audit records, ownership columns, subscriptions, or reducer guards.

Useful questions:

- Which issuer values are currently accepted?
- Which audiences are currently accepted?
- Are user, membership, profile, document, or audit rows keyed directly by `Identity`?
- Do reducers check `ctx.sender` directly, or do they resolve it to an application actor?
- Do service accounts, API keys, scheduled jobs, or integrations share the same provider as human users?

### 2. Add identity links before cutover

Add a link table before the migration changes login behavior. For existing users, create links from the current SpacetimeDB identity to the stable application actor. For future users, create the link during first authorized connection.

After this step, reducers should prefer:

```text
ctx.sender -> actor_identity_link -> actor -> tenant membership and permissions
```

over authorizing solely from:

```text
ctx.sender -> direct ownership check
```

Direct ownership checks can still be valid for simple resources, but the migration boundary is safer when authorization resolves through an actor link.

### 3. Accept old and new issuers during a transition

During cutover, allow both the old and new issuer only when the audience, token type, tenant context, and actor link checks also pass. Do not simply add a new trusted issuer without narrowing it to the intended audience and module.

For example, a module might accept:

| Issuer | Use |
| --- | --- |
| Old Keycloak realm issuer | Existing production sessions during the transition window. |
| New SpacetimeAuth project issuer | New or migrated users after the cutover starts. |
| App-owned broker issuer | Short-lived tokens minted after the app validates sessions, API keys, or enterprise SSO. |

Keep the transition window as short as practical. Log which issuer is used so you can see when old traffic has stopped.

### 4. Verify the new link

When a user first connects with the new provider, verify that the new external credential belongs to the same application actor as the old credential. The verification method depends on your app:

- A user completes an authenticated migration flow while still signed in with the old provider.
- An administrator imports a provider-backed mapping from a trusted migration source.
- A customer identity admin approves a tenant-scoped SSO or SCIM mapping.
- A service account rotates through an operator-approved credential exchange.

After verification, insert a new `actor_identity_link` for the new SpacetimeDB identity and mark the old link as active, migrated, or retiring according to your policy.

### 5. Migrate rows keyed by identity

If module tables use `Identity` as a primary key or owner column, decide whether to keep old identities as historical actors or rewrite ownership to the stable actor model.

Common choices:

- Keep historical rows keyed by old identity and use actor links for future authorization.
- Add an `actor_id` column and gradually migrate reducer logic from `identity` to `actor_id`.
- Backfill `actor_id` from the identity link table, then make new writes require `actor_id`.
- Keep audit rows immutable and add actor-link context when displaying or querying them.

Avoid bulk rewriting audit history unless your product explicitly requires it. It is often better for audit events to record the issuer and identity that actually performed the action at the time.

### 6. Retire the old issuer

Only remove the old issuer after traffic and audit checks show that active clients no longer depend on it.

Before retirement:

- Confirm no active connections or recent reducer calls use the old issuer.
- Confirm service actors and scheduled jobs have migrated.
- Confirm rollback credentials and JWKS configuration are documented.
- Confirm users who did not migrate have a support path.
- Keep enough identity-link history to explain old audit records.

After retirement, reject old issuer tokens at connection time and keep the old identity links for audit and recovery.

## Reducer checklist

For every reducer or connection handler affected by migration:

- Require a JWT for authenticated workflows.
- Check the expected issuer or allowed transition issuer set.
- Check `aud` so a token issued for another app cannot be replayed.
- Resolve `ctx.sender` to a stable actor before checking tenant membership, permissions, or ownership.
- Treat email, domain, display name, and provider name as hints, not durable identity keys.
- Log or store enough issuer, subject, actor, tenant, and migration state to debug authorization decisions.
- Fail closed when an identity link is missing, duplicated, retired, or assigned to the wrong tenant.

## Planning checklist

- Decide whether the migration can preserve the same `iss` and `sub`.
- Create actor and identity-link tables before changing login behavior.
- Backfill links for existing identities.
- Add tests for old issuer, new issuer, wrong audience, missing link, retired link, and cross-tenant link attempts.
- Run old and new issuers in parallel during a bounded transition window.
- Keep service actors and API keys separate from human users.
- Retire the old issuer only after traffic, audit, and rollback checks are complete.
