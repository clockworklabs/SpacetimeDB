# Authentication

SpacetimeDB modules are exposed to the open internet and anyone can connect to
them. Therefore, authentication is a critical part of using SpacetimeDB securely.

SpacetimeDB uses OpenID Connect (OIDC) identity tokens for authentication, making
it compatible with most OIDC providers (e.g., Auth0, Firebase, Clerk, Google,
GitHub, Facebook, and many more). You can choose any OIDC provider that fits your
needs, or even implement your own.

If you're new to OIDC, check out our [blog post about OIDC](https://spacetimedb.com/blog/who-are-you)
to learn more about how OIDC works and why it's a great choice for authentication.

## SpacetimeAuth

To make it easier to get started with authentication, SpacetimeDB offers
[SpacetimeAuth](https://spacetimedb.com/docs/spacetimeauth), a fully managed
OIDC provider built specifically for SpacetimeDB applications. SpacetimeAuth handles
user management, authentication flows, and token issuance, so you don't have to
worry about building and maintaining your own authentication service.

SpacetimeAuth is meant to be simple to use and easy to integrate with SpacetimeDB.
While being production-ready and able to support most common use cases, it is not
as feature-rich as some third-party OIDC providers. If you need advanced features
or customization, you may want to consider using a third-party OIDC provider instead.

## Third-party OIDC providers

You can also use any third-party OIDC provider with SpacetimeDB. Most OIDC
providers offer similar features, such as user management, authentication flows,
and token issuance. When choosing a third-party OIDC provider, consider factors
such as ease of integration, pricing, scalability, and security.

- [Auth0](https://auth0.com/) A managed identity and access
  management service that provides, user management, and extensible login flows
  for applications and APIs.
- [Clerk](https://clerk.com/) A developer-focused
  authentication and user management platform that provides OIDC-compliant
  sign-in, session management, and prebuilt UI components for modern web applications.
- [Keycloak](https://www.keycloak.org/) An open-source and
  self-hosted OIDC provider with extensive features, customization options and integrations.

## Authenticate your services

Sometimes, you may need to authenticate your servers, APIs or other services that
interact with your SpacetimeDB database. OIDC tokens can also be used for this
purpose, allowing secure communication between your services and SpacetimeDB.

To authenticate your services, you have e few options depending on your OIDC provider:

- **Client credentials flow**: Many OIDC providers support the client credentials
  flow, which allows your service to obtain an access token using its own
  credentials (client ID and client secret). This is a common approach for
  service-to-service authentication.
- **Service accounts**: Some OIDC providers offer service accounts, which are
  special user accounts designed for non-human users (e.g., servers, APIs). You
  can create a service account and use its credentials to obtain an access token.

## Authorization in your module

Obtaining an OIDC token is just the first step in securing your SpacetimeDB
module, known as **authentication**. You also need to implement **authorization**
to control what authenticated users can do within your module.

When a client connects to your SpacetimeDB module, the SpacetimeDB server
validates the client's OIDC token and extracts the identity claims. These claims
are then made available to your module's reducers, views and procedures via the context.

[Check out the usage guide](./00500-authentication/00500-usage.md) for more
information on how to access and use authentication claims in your module:
