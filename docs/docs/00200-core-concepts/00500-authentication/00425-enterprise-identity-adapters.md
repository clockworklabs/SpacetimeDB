---
title: Enterprise Identity Adapters
slug: /authentication/enterprise-identity-adapters
---

Enterprise SaaS applications often need more than a single login screen. A customer may want to sign in with Microsoft Entra ID, Google Workspace, Okta, Keycloak, Auth0, a custom SAML provider, or a hosted enterprise identity service. The same customer may also need SCIM or directory sync, verified domains, customer-managed OAuth clients, delegated identity admins, audit records, and offboarding rules.

SpacetimeDB does not need a provider-specific integration for each of those systems. It needs an OpenID Connect (OIDC) or JWT token it can verify, plus module-local authorization state that reducers can trust. Treat enterprise identity providers as adapters at your application boundary, then pass SpacetimeDB a normalized token and store the authorization data your module needs.

## Where SpacetimeAuth fits

[SpacetimeAuth](./00100-spacetimeauth/index.md) is the SpacetimeDB-native managed OIDC provider. It is the easiest path when you want authentication, users, roles, login pages, and token issuance without building or operating a separate auth service. A SpacetimeAuth project can issue tokens for one or more clients, and those tokens can be used directly with SpacetimeDB SDKs.

SpacetimeAuth is still an identity adapter in this architecture. It is a good default when its managed OIDC features match your application needs. It is not required for every deployment, and it does not replace an application-owned enterprise identity plane when you need customer-managed SSO, SCIM, API-key administration, per-tenant OAuth clients, or custom authorization workflows.

There are three common ways to use it:

- Use SpacetimeAuth directly. Browser or native clients sign in with SpacetimeAuth and connect to SpacetimeDB with the resulting token.
- Use SpacetimeAuth as one upstream issuer. Your application server accepts a SpacetimeAuth token, resolves the current tenant or workspace, checks application authorization, and then connects to SpacetimeDB using the topology you choose.
- Use SpacetimeAuth for simple apps or development environments while using another OIDC provider or app-owned broker for enterprise tenants.

Whichever path you choose, reducers should still check `iss`, `aud`, and the module-local authorization data that matters for the reducer being called. The [auth claims usage guide](./00500-usage.md) shows how to inspect those claims inside reducers.

## Adapter choices

| Adapter | Owns | Good fit | What SpacetimeDB should see |
| --- | --- | --- | --- |
| SpacetimeAuth | Managed OIDC projects, clients, users, roles, and token issuance. | SpacetimeDB-native applications that want the fastest supported auth path. | A SpacetimeAuth-issued token with the expected issuer and client audience. |
| App-owned auth service | Web sessions, organizations, API keys, tenant selection, and authorization checks. | Full-stack SaaS apps where the server owns the browser session and SpacetimeDB is the realtime data plane. | A short-lived JWT minted after the app authorizes the actor. |
| Enterprise SSO provider | Customer login through SAML, OIDC, or OAuth. | Customer-managed sign-in with providers such as Microsoft Entra ID, Google Workspace, Okta, Keycloak, or Auth0. | A normalized application token, not the raw SAML assertion or provider callback payload. |
| Directory sync or SCIM provider | User and group provisioning from a customer directory. | Automated onboarding, deprovisioning, and group synchronization. | Updated application membership state, followed by a normal SpacetimeDB token when an actor is authorized. |
| Hosted enterprise identity service | SSO, SCIM, admin portal, provider catalog, and enterprise onboarding workflows. | Teams that want WorkOS-style enterprise identity features without building every connector themselves. | A normalized application token and module-local authorization state, not hosted-provider secrets. |

## Recommended boundary

Keep provider complexity on the application side of the boundary:

```text
Enterprise IdP, SpacetimeAuth, or app auth
  |
  | OIDC, SAML, OAuth, SCIM, or provider API
  v
Application identity plane
  |
  | tenant selection, membership checks, API-key checks, audit policy
  v
SpacetimeDB token broker or direct OIDC token selection
  |
  | short-lived JWT with stable claims
  v
SpacetimeDB reducers, tables, views, and subscriptions
```

The application identity plane may be a small service, a full-stack web server, or a larger enterprise identity control plane. Its job is to normalize provider-specific data into stable application concepts before SpacetimeDB sees it.

Useful application-side records often include:

- `auth_provider_adapter`: the configured provider for a tenant, such as SpacetimeAuth, a custom OIDC provider, a SAML bridge, or a hosted enterprise identity service.
- `enterprise_sso_connection`: tenant-scoped SSO metadata, issuer or entity ID, verified domains, signing requirements, and IdP-initiated SSO policy.
- `directory_sync_connection`: SCIM or directory sync state, secret references, attribute mappings, deprovisioning behavior, checkpoints, and last-run health.
- `federated_identity_link`: a durable mapping from external issuer plus external subject to an application user, actor, or profile.
- `oauth_client_application`: customer-built or first-party client registration, redirect URIs, allowed origins, PKCE policy, scopes, and token lifetimes.
- `customer_identity_admin_grant`: the tenant-scoped authority to configure SSO, SCIM, domains, users, or OAuth clients.

Those records do not all need to live in SpacetimeDB. Store them where your application owns identity configuration. Mirror only the subset that reducers need for authorization, filtering, or audit.

## Token shape

For enterprise adapters, prefer compact and stable JWT claims:

```json
{
  "iss": "https://auth.example.com",
  "sub": "user_123",
  "aud": "spacetimedb:my-module",
  "token_type": "spacetime-access",
  "tenant_id": "tenant_123",
  "actor_ref": "user:user_123",
  "session_ref": "session_123",
  "scope": "spacetime:connect spacetime:write"
}
```

Use claims for stable identity and routing hints. Keep mutable authorization in the application identity plane or in SpacetimeDB tables:

- Put issuer, subject, audience, token type, actor reference, tenant reference, and short-lived session references in the JWT.
- Put membership, roles, feature flags, impersonation grants, billing state, SCIM provisioning state, and revocation-sensitive permissions in tables or application-owned records.
- Keep JWT lifetimes short when the token is derived from a web session, API key, or enterprise SSO login.

Do not send raw IdP assertions, SCIM bearer tokens, provider admin API keys, refresh tokens, or long-lived customer integration secrets to SpacetimeDB unless your module is explicitly designed to store that sensitive data.

## Reducer checks

Inside SpacetimeDB, a token is authentication input, not the full authorization decision. Reducers and connection handlers should verify the claims that make the token relevant to the module:

- Require a JWT when the reducer or subscription needs an authenticated actor.
- Check the expected issuer or allowed issuer set.
- Check the audience so a token issued for another application cannot be replayed against your module.
- Check a token type or scope if your application mints different token classes for browser users, service actors, or delegated actions.
- Resolve the actor and tenant to module-local authorization state before allowing writes.
- Record enough actor, tenant, session, and delegation metadata for audit.

For simple applications, SpacetimeAuth roles or other provider role claims may be enough. For enterprise SaaS applications, treat role claims as hints and prefer module-local authorization tables for mutable or tenant-scoped policy.

## Identity continuity

SpacetimeDB derives the authenticated identity from the issuer and subject. If a user moves from one provider to another, or if a tenant migrates from one enterprise identity adapter to another, the issuer or subject may change. That can produce a different SpacetimeDB identity even when the human user is the same.

Plan migrations with explicit identity links:

- Store the old issuer plus subject and the new issuer plus subject.
- Link both to the same application actor or profile after a verified migration step.
- Run old and new issuers in parallel during cutover when possible.
- Avoid merging accounts by email alone. Email addresses and domains can change and may be shared, recycled, or delegated.
- Keep audit records that show which external identity was used at the time of each sensitive action.

This is especially important when leaving a centralized provider such as Keycloak, moving from a hosted enterprise identity service to self-hosted auth, or introducing customer-specific SSO after users already exist.

## Checklist

- Choose SpacetimeAuth when its managed OIDC project model fits the app.
- Choose an app-owned identity plane when the app must own sessions, organizations, API keys, customer SSO, SCIM, and admin workflows.
- Treat Microsoft Entra ID, Google Workspace, Okta, Keycloak, Auth0, WorkOS-style services, and custom SAML/OIDC systems as provider adapters.
- Normalize provider results before minting or selecting a SpacetimeDB token.
- Check `iss` and `aud` in reducers or connection handlers.
- Keep mutable authorization out of long-lived JWT claims.
- Model issuer-plus-subject identity links before changing providers.
- Keep SCIM tokens, IdP assertions, provider API keys, and refresh tokens outside SpacetimeDB unless the module explicitly needs to store them.
